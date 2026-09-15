#![forbid(unsafe_code)]
pub mod api;
pub mod auth;
pub mod control;
pub mod error;
pub mod integration;
pub mod storage;

use aes_gcm::{
    Aes256Gcm, KeyInit,
    aead::{Aead, AeadCore, OsRng},
};
use auth::IdentityProvider;
use error::{Error, Result};
use integration::{ArchiveStorage, Management};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::{path::Path, str::FromStr, sync::Arc};

#[derive(Clone)]
pub struct State {
    pub db: SqlitePool,
    pub identity: Arc<dyn IdentityProvider>,
    pub management: Arc<dyn Management>,
    pub storage: Arc<dyn ArchiveStorage>,
    pub app_id: String,
    pub iam_login_url: String,
    pub encryption_key: [u8; 32],
    pub webhook_secret: String,
}
impl State {
    pub fn encrypt(&self, value: &str) -> Result<String> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|_| Error::unavailable("Encryption key is invalid"))?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let encrypted = cipher
            .encrypt(&nonce, value.as_bytes())
            .map_err(|_| Error::unavailable("Secret encryption failed"))?;
        Ok(format!("{}:{}", hex::encode(nonce), hex::encode(encrypted)))
    }
    pub fn decrypt(&self, value: &str) -> Result<String> {
        let (nonce, data) = value
            .split_once(':')
            .ok_or_else(|| Error::unavailable("Encrypted secret is invalid"))?;
        let nonce =
            hex::decode(nonce).map_err(|_| Error::unavailable("Encrypted nonce is invalid"))?;
        if nonce.len() != 12 {
            return Err(Error::unavailable("Encrypted nonce is invalid"));
        }
        let data =
            hex::decode(data).map_err(|_| Error::unavailable("Encrypted data is invalid"))?;
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|_| Error::unavailable("Encryption key is invalid"))?;
        let plain = cipher
            .decrypt(nonce.as_slice().into(), data.as_slice())
            .map_err(|_| Error::unavailable("Secret decryption failed"))?;
        String::from_utf8(plain).map_err(|_| Error::unavailable("Decrypted secret is invalid"))
    }
}
pub async fn database(path: &str) -> anyhow::Result<SqlitePool> {
    if !path.starts_with("sqlite:")
        && let Some(parent) = Path::new(path).parent()
    {
        tokio::fs::create_dir_all(parent).await?;
    }
    let options = if path.starts_with("sqlite:") {
        SqliteConnectOptions::from_str(path)?
    } else {
        SqliteConnectOptions::new().filename(path)
    }
    .create_if_missing(true)
    .foreign_keys(true)
    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
    .busy_timeout(std::time::Duration::from_secs(10));
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&db).await?;
    Ok(db)
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
