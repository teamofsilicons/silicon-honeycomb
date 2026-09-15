//! Organization-owned logo uploads; only sanitized images reach delegated storage.
use crate::{
    State,
    api::{context, key},
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State as S},
    http::HeaderMap,
};
use image::{ImageFormat, ImageReader, Limits};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::io::Cursor;

pub const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024;

fn normalize(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.is_empty() || bytes.len() > MAX_LOGO_BYTES {
        return Err(Error::bad(
            "Choose a PNG, JPEG or WebP logo no larger than 2 MiB",
        ));
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| Error::bad("Cannot read this image"))?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)
    ) {
        return Err(Error::bad("Choose a PNG, JPEG or WebP logo"));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| Error::bad("Invalid image or dimensions exceed 2048 × 2048 pixels"))?;
    // Re-encoding pixels removes metadata, trailing payloads and animation.
    let mut png = Cursor::new(Vec::new());
    decoded
        .to_rgba8()
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|_| Error::bad("Cannot convert this logo to PNG"))?;
    let png = png.into_inner();
    if png.len() > MAX_LOGO_BYTES {
        return Err(Error::bad(
            "Converted logo exceeds 2 MiB; choose a smaller image",
        ));
    }
    Ok(png)
}

pub async fn upload(
    S(s): S<State>,
    h: HeaderMap,
    Path(org): Path<String>,
    body: Bytes,
) -> Result<Json<Value>> {
    let c = context(&s, &h).await?;
    c.admin(&org)?;
    let actor = &c.identity()?.principal_id;
    let idem = key(&h)?;
    let digest = hex::encode(Sha256::digest(&body));
    let request_hash = hex::encode(Sha256::digest(format!("logo.upload:{org}:{digest}")));
    let png = tokio::task::spawn_blocking(move || normalize(&body))
        .await
        .map_err(|e| anyhow::anyhow!(e))??;
    let mut tx = s.db.begin().await?;
    let prior =
        sqlx::query("SELECT * FROM operations WHERE plane=? AND actor=? AND idempotency_key=?")
            .bind(&c.plane)
            .bind(actor)
            .bind(idem)
            .fetch_optional(&mut *tx)
            .await?;
    let operation = if let Some(row) = prior {
        if row.get::<String, _>("request_hash") != request_hash {
            return Err(Error::conflict(
                "Idempotency key was used for another request",
            ));
        }
        if row.get::<String, _>("state") == "accepted" {
            return Ok(Json(
                serde_json::from_str(&row.get::<String, _>("result"))
                    .map_err(|e| anyhow::anyhow!(e))?,
            ));
        }
        row.get::<String, _>("id")
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,?,?,?,'logo.upload',?,?,0,?)")
            .bind(&id).bind(&c.plane).bind(actor).bind(idem).bind(format!("organization:{org}")).bind(request_hash).bind(now()).execute(&mut *tx).await?;
        id
    };
    let lease = uuid::Uuid::new_v4().to_string();
    let claimed = sqlx::query("UPDATE operations SET lease_token=?,lease_until=? WHERE id=? AND (lease_until IS NULL OR lease_until<=?)")
        .bind(&lease).bind(now()+900).bind(&operation).bind(now()).execute(&mut *tx).await?;
    if claimed.rows_affected() != 1 {
        return Err(Error::conflict(
            "Logo upload is already running; retry the same request later",
        ));
    }
    tx.commit().await?;
    let result: Result<Value> = async {
        let temp = tempfile::tempdir().map_err(|e|anyhow::anyhow!(e))?;
        let path = temp.path().join("logo.png");
        tokio::fs::write(&path, &png).await.map_err(|e|anyhow::anyhow!(e))?;
        let logo_url = s.storage.upload_logo(&org, &path, c.token.as_deref().ok_or_else(Error::unauthorized)?, c.environment.as_deref(), &operation).await?;
        let parsed = url::Url::parse(&logo_url).map_err(|_|Error::unavailable("Briefcase returned an invalid logo URL"))?;
        if parsed.scheme()!="https" || !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(Error::unavailable("Briefcase logo URL must use HTTPS without credentials"));
        }
        Ok(json!({"id":operation,"state":"accepted","logo_url":logo_url,"sha256":hex::encode(Sha256::digest(&png))}))
    }.await;
    match result {
        Ok(value) => {
            let saved = sqlx::query("UPDATE operations SET state='accepted',result=?,error=NULL,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?")
                .bind(value.to_string()).bind(&operation).bind(&lease).execute(&s.db).await?;
            if saved.rows_affected() != 1 {
                return Err(Error::conflict(
                    "Upload lease changed; retry the same request to reconcile",
                ));
            }
            Ok(Json(value))
        }
        Err(error) => {
            sqlx::query("UPDATE operations SET error=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?")
                .bind(&error.1.message).bind(&operation).bind(&lease).execute(&s.db).await?;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_pixels_and_rejects_active_or_oversized_content() {
        let mut png = Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(2, 2, image::Rgba([23, 54, 184, 255]))
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let clean = png.into_inner();
        let mut trailing = clean.clone();
        trailing.extend_from_slice(b"<script>trailing</script>");
        assert_eq!(normalize(&trailing).unwrap(), normalize(&clean).unwrap());
        assert!(normalize(b"<svg onload='alert(1)'/>").is_err());
        assert!(normalize(&vec![0; MAX_LOGO_BYTES + 1]).is_err());
        let mut wide = Cursor::new(Vec::new());
        image::RgbaImage::new(2049, 1)
            .write_to(&mut wide, ImageFormat::Png)
            .unwrap();
        assert!(normalize(wide.get_ref()).is_err());
    }
}
