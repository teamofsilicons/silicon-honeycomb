//! Durable notification delivery. Test-plane mail is always captured, never sent.
use crate::{
    State,
    api::{context, key},
    error::{Error, Result},
    now,
};
use axum::{
    Json,
    extract::State as S,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub message: String,
    pub pr: Option<String>,
}
pub async fn report(
    S(s): S<State>,
    h: HeaderMap,
    Json(body): Json<Report>,
) -> Result<(StatusCode, Json<Value>)> {
    let k = key(&h)?;
    let c = context(&s, &h).await?;
    let actor = c
        .identity
        .as_ref()
        .map(|i| i.principal_id.as_str())
        .unwrap_or("anonymous");
    if !(10..=20000).contains(&body.message.trim().len()) {
        return Err(Error::bad(
            "Describe the bug in 10–20000 characters, including what happened and how to reproduce it",
        ));
    }
    if let Some(pr) = &body.pr {
        let url =
            url::Url::parse(pr).map_err(|_| Error::bad("pr must be a GitHub pull request URL"))?;
        let parts = url
            .path_segments()
            .map(|v| v.collect::<Vec<_>>())
            .unwrap_or_default();
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || parts.len() != 4
            || parts[2] != "pull"
            || parts[3].parse::<u64>().is_err()
        {
            return Err(Error::bad("pr must be an HTTPS GitHub pull request URL"));
        }
    }
    let digest = hex::encode(Sha256::digest(
        serde_json::to_vec(&body).map_err(|e| anyhow::anyhow!(e))?,
    ));
    if let Some(row) = sqlx::query(
        "SELECT id,request_hash FROM reports WHERE plane=? AND actor=? AND idempotency_key=?",
    )
    .bind(&c.plane)
    .bind(actor)
    .bind(k)
    .fetch_optional(&s.db)
    .await?
    {
        if row.get::<String, _>("request_hash") != digest {
            return Err(Error::conflict(
                "Idempotency key already used for another report",
            ));
        }
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({"id":row.get::<String,_>("id"),"saved":true})),
        ));
    }
    let mut tx = s.db.begin().await?;
    let recent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM reports WHERE plane=? AND actor=? AND created_at>?",
    )
    .bind(&c.plane)
    .bind(actor)
    .bind(now() - 3600)
    .fetch_one(&mut *tx)
    .await?;
    if recent >= 10 {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            honeycomb_core::ApiError {
                code: "report_limit".into(),
                message: "Report limit reached; retry after one hour".into(),
                details: vec![],
            },
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO reports(id,plane,actor,idempotency_key,request_hash,message,pr,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(&id).bind(&c.plane).bind(actor).bind(k).bind(digest).bind(&body.message).bind(&body.pr).bind(now()).execute(&mut *tx).await?;
    let mail_state = if c.plane == "production" {
        "pending"
    } else {
        "captured"
    };
    sqlx::query("INSERT INTO outbox(id,plane,event_key,kind,payload,state,created_at) VALUES(?,?,?,'report.created',?,?,?)").bind(&id).bind(&c.plane).bind(format!("report:{id}")).bind(json!({"report_id":id,"actor":actor,"message":body.message,"pr":body.pr}).to_string()).bind(mail_state).bind(now()).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(
            json!({"id":id,"saved":true,"notification":mail_state,"contribute":if body.pr.is_none(){Some("You can also submit a fix at https://github.com/teamofsilicons/silicon-honeycomb")}else{None}}),
        ),
    ))
}

#[derive(Clone, Debug)]
pub struct Email {
    pub recipients: Vec<String>,
    pub subject: String,
    pub text: String,
    pub event_id: String,
}
#[derive(Debug)]
pub enum Delivery {
    Sent(String),
    Retry(String),
    Uncertain(String),
}
#[async_trait::async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, email: &Email) -> Delivery;
}
pub struct Postmark {
    http: reqwest::Client,
    token: String,
    endpoint: String,
}
impl Postmark {
    pub fn new(token: String) -> anyhow::Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            token,
            endpoint: "https://api.postmarkapp.com/email".into(),
        })
    }
}
#[async_trait::async_trait]
impl Mailer for Postmark {
    async fn send(&self, email: &Email) -> Delivery {
        let response=self.http.post(&self.endpoint).header("X-Postmark-Server-Token",&self.token).header("Accept","application/json")
            .json(&json!({"From":"honeycomb@teamofsilicons.com","To":email.recipients.join(","),"Subject":email.subject,"TextBody":email.text,"MessageStream":"outbound","TrackOpens":false,"TrackLinks":"None","Metadata":{"honeycomb_event_id":email.event_id}})).send().await;
        match response {
            Ok(response) if response.status().is_success()=>match response.json::<Value>().await {
                Ok(value) if value["ErrorCode"]==0 && value["MessageID"].is_string()=>Delivery::Sent(value["MessageID"].as_str().unwrap().into()),
                Ok(_)=>Delivery::Retry("Postmark rejected the message; check sender and recipient configuration".into()),
                Err(_)=>Delivery::Uncertain("Postmark accepted HTTP but its receipt was unreadable; reconcile before resending".into()),
            },
            Ok(response) if response.status().is_server_error()=>Delivery::Uncertain("Postmark server error; reconcile delivery before resending".into()),
            Ok(response)=>Delivery::Retry(format!("Postmark returned HTTP {}",response.status().as_u16())),
            Err(error) if error.is_connect()=>Delivery::Retry("Could not connect to Postmark".into()),
            Err(_)=>Delivery::Uncertain("Postmark delivery response was lost; reconcile before resending".into()),
        }
    }
}
async fn render(s: &State, id: &str, kind: &str, payload: &Value) -> Result<Email> {
    let (recipients, subject, text) = if kind == "report.created" {
        (
            vec![
                "saketdev12@gmail.com".into(),
                "shubhastro2@gmails.com".into(),
                "bugs@teamofsilicons.com".into(),
            ],
            "Honeycomb bug report".into(),
            format!(
                "Report {id}\nReporter: {}\n\n{}\n\nPull request: {}",
                payload["actor"].as_str().unwrap_or("anonymous"),
                payload["message"].as_str().unwrap_or(""),
                payload["pr"].as_str().unwrap_or("Not supplied")
            ),
        )
    } else {
        let recipients = if kind.starts_with("publication.") {
            let request = payload["request_id"]
                .as_str()
                .ok_or_else(|| Error::bad("Publication notification has no request"))?;
            let app = payload["app_id"]
                .as_str()
                .ok_or_else(|| Error::bad("Publication notification has no application"))?;
            let plan: Option<String> = sqlx::query_scalar(
                "SELECT plan_id FROM publication_requests WHERE id=? AND plane='production' AND app_id=?",
            ).bind(request).bind(app).fetch_optional(&s.db).await?.flatten();
            let plan = plan
                .ok_or_else(|| Error::unavailable("Waiting for IAM's publication review plan"))?;
            let mut providers = std::collections::BTreeSet::new();
            if let Some(provider) = payload["review_provider"].as_str() {
                providers.insert(provider.to_owned());
            } else {
                providers.insert("owners".to_owned());
                if let Some(values) = payload.get("review_providers") {
                    let values: Vec<String> = serde_json::from_value(values.clone())
                        .map_err(|_| Error::bad("Notification has invalid review providers"))?;
                    providers.extend(values);
                }
            }
            let mut recipients = Vec::new();
            for provider in providers {
                recipients.extend(
                    s.management
                        .review_notification_recipients(&plan, &provider)
                        .await?,
                );
            }
            // One discussion event delivers once even when an administrator is
            // both a requester and a provider reviewer.
            recipients.sort_by_key(|email| email.to_ascii_lowercase());
            recipients.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            recipients
        } else {
            let org = payload["org_id"]
                .as_str()
                .ok_or_else(|| Error::bad("Notification has no organization"))?;
            s.management.notification_recipients(org).await?
        };
        let app = payload["app_id"].as_str().unwrap_or("application");
        let (subject, text) = match kind {
            "release.created" => (
                format!("{app}: new release"),
                format!(
                    "Congratulations on releasing {app} {}.\n\nManage your application at https://console.honeycomb.teamofsilicons.com",
                    payload["version"].as_str().unwrap_or("")
                ),
            ),
            "publication.review" => (
                format!("{app}: application review requested"),
                "An application is awaiting your review. Sign in to inspect its requested access and discussion.\n\nhttps://console.honeycomb.teamofsilicons.com".into(),
            ),
            "publication.message" => (
                format!("{app}: review discussion updated"),
                "A new reply is available in the application's review discussion.\n\nhttps://console.honeycomb.teamofsilicons.com".to_string(),
            ),
            "publication.status" => (
                format!("{app}: publication status updated"),
                format!(
                    "Publication status: {}\n\nhttps://console.honeycomb.teamofsilicons.com",
                    payload["state"].as_str().unwrap_or("pending")
                ),
            ),
            _ => return Err(Error::bad("Unsupported notification kind")),
        };
        (recipients, subject, text)
    };
    if recipients.is_empty()
        || recipients.len() > 50
        || recipients
            .iter()
            .any(|email| !email.contains('@') || email.contains([',', ';', '\r', '\n']))
    {
        return Err(Error::unavailable(
            "IAM returned an invalid notification recipient set",
        ));
    }
    Ok(Email {
        recipients,
        subject,
        text,
        event_id: id.into(),
    })
}
/// Process one queued event. A lost provider receipt never causes an automatic duplicate.
pub async fn dispatch_once(s: &State, mailer: Option<&dyn Mailer>) -> Result<bool> {
    let row=sqlx::query("SELECT * FROM outbox WHERE state='pending' AND next_attempt<=? ORDER BY created_at,id LIMIT 1").bind(now()).fetch_optional(&s.db).await?;
    let Some(row) = row else { return Ok(false) };
    let id: String = row.get("id");
    if row.get::<String, _>("plane") != "production" {
        sqlx::query("UPDATE outbox SET state='captured' WHERE id=? AND state='pending'")
            .bind(id)
            .execute(&s.db)
            .await?;
        return Ok(true);
    }
    let Some(mailer) = mailer else {
        return Ok(false);
    };
    let payload: Value =
        serde_json::from_str(&row.get::<String, _>("payload")).map_err(|e| anyhow::anyhow!(e))?;
    let email = match render(s, &id, &row.get::<String, _>("kind"), &payload).await {
        Ok(email) => email,
        Err(error) => {
            sqlx::query("UPDATE outbox SET error=?,next_attempt=? WHERE id=?")
                .bind(error.1.message)
                .bind(now() + 300)
                .bind(id)
                .execute(&s.db)
                .await?;
            return Ok(true);
        }
    };
    let claimed=sqlx::query("UPDATE outbox SET state='sending',attempts=attempts+1,next_attempt=? WHERE id=? AND state='pending'").bind(now()+60).bind(&id).execute(&s.db).await?;
    if claimed.rows_affected() == 0 {
        return Ok(true);
    }
    let (state, error, provider) = match mailer.send(&email).await {
        Delivery::Sent(id) => ("sent", None, Some(id)),
        Delivery::Retry(error) => ("pending", Some(error), None),
        Delivery::Uncertain(error) => ("uncertain", Some(error), None),
    };
    sqlx::query("UPDATE outbox SET state=?,error=?,provider_id=?,next_attempt=? WHERE id=?")
        .bind(state)
        .bind(error)
        .bind(provider)
        .bind(now() + 300)
        .bind(id)
        .execute(&s.db)
        .await?;
    Ok(true)
}
pub async fn run(s: State, mailer: Option<Postmark>) {
    loop {
        // An interrupted send may have reached Postmark. Preserve it for reconciliation.
        let _=sqlx::query("UPDATE outbox SET state='uncertain',error='Worker interrupted during delivery; reconcile with Postmark before resending' WHERE state='sending' AND next_attempt<?").bind(now()).execute(&s.db).await;
        for _ in 0..20 {
            if !matches!(
                dispatch_once(&s, mailer.as_ref().map(|m| m as &dyn Mailer)).await,
                Ok(true)
            ) {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, header, method, path},
    };
    #[tokio::test]
    async fn postmark_contract_requires_success_receipt() {
        let server = MockServer::start().await;
        let mailer = Postmark {
            http: reqwest::Client::new(),
            token: "fixture-only".into(),
            endpoint: format!("{}/email", server.uri()),
        };
        Mock::given(method("POST")).and(path("/email")).and(header("X-Postmark-Server-Token","fixture-only"))
            .and(body_partial_json(json!({"To":"recipient@example.com","TrackOpens":false,"Metadata":{"honeycomb_event_id":"fixture-event"}})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ErrorCode":0,"MessageID":"fixture-message"}))).expect(1).mount(&server).await;
        let email = Email {
            recipients: vec!["recipient@example.com".into()],
            subject: "Fixture".into(),
            text: "No external mail is sent.".into(),
            event_id: "fixture-event".into(),
        };
        assert!(matches!(mailer.send(&email).await,Delivery::Sent(id) if id=="fixture-message"));
        server.reset().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("invalid receipt"))
            .mount(&server)
            .await;
        assert!(matches!(mailer.send(&email).await, Delivery::Uncertain(_)));
    }
}
