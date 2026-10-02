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
    pub html: String,
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
    /// Positive receipts can resolve an uncertain send. Absence is never proof
    /// that delivery failed and must not trigger an automatic resend.
    async fn receipt(&self, _event_id: &str) -> Option<String> {
        None
    }
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
            .json(&json!({"From":"honeycomb@teamofsilicons.com","To":email.recipients.first(),"Bcc":email.recipients.iter().skip(1).cloned().collect::<Vec<_>>().join(","),"Subject":email.subject,"TextBody":email.text,"HtmlBody":email.html,"MessageStream":"outbound","TrackOpens":false,"TrackLinks":"None","Metadata":{"honeycomb_event_id":email.event_id}})).send().await;
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
    async fn receipt(&self, event_id: &str) -> Option<String> {
        let mut url = url::Url::parse(&self.endpoint).ok()?;
        url.set_path("/messages/outbound");
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("count", "100")
            .append_pair("offset", "0")
            .append_pair("metadata_honeycomb_event_id", event_id)
            .append_pair("messagestream", "outbound");
        let response = self
            .http
            .get(url)
            .header("X-Postmark-Server-Token", &self.token)
            .header("Accept", "application/json")
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let data: Value = response.json().await.ok()?;
        data["Messages"].as_array()?.iter().find_map(|message| {
            if message["Metadata"]["honeycomb_event_id"].as_str() != Some(event_id)
                || !matches!(
                    message["Status"].as_str(),
                    Some("Sent" | "Processed" | "Queued")
                )
            {
                return None;
            }
            message["MessageID"]
                .as_str()
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        })
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
        let (link, _) = notification_link(kind, payload)?;
        let (subject, text) = match kind {
            "release.created" => (
                format!("{app}: new release"),
                format!(
                    "{app} {} has been uploaded. Open the release page to check its review and publication status.\n\n{link}",
                    payload["version"].as_str().unwrap_or("")
                ),
            ),
            "publication.review" => (
                format!("{app}: application review requested"),
                format!(
                    "{app} is awaiting your review. Sign in to inspect the requested access and approve or decline the request.\n\n{link}"
                ),
            ),
            "publication.message" => (
                format!("{app}: review discussion updated"),
                format!("A new reply is available in {app}'s review discussion.\n\n{link}"),
            ),
            "publication.status" => (
                format!("{app}: publication status updated"),
                format!(
                    "Publication status: {}\n\n{link}",
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
    let (link, action) = notification_link(kind, payload)?;
    let html = email_html(&subject, &text, &link, action);
    Ok(Email {
        recipients,
        subject,
        text,
        html,
        event_id: id.into(),
    })
}
fn notification_link(kind: &str, payload: &Value) -> Result<(String, &'static str)> {
    let console = "https://console.honeycomb.teamofsilicons.com";
    let app = payload["app_id"].as_str().unwrap_or("application");
    let mut url = url::Url::parse(console).expect("fixed valid origin");
    let action = if kind == "release.created" {
        url.set_host(Some("honeycomb.teamofsilicons.com"))
            .expect("fixed valid hostname");
        let channel = payload["channel"].as_str().unwrap_or("prod");
        let version = payload["version"].as_str().unwrap_or("");
        url.path_segments_mut()
            .expect("https URL")
            .clear()
            .extend(["apps", app, "releases", channel, version]);
        "View release"
    } else if kind.starts_with("publication.") {
        let request = payload["request_id"]
            .as_str()
            .ok_or_else(|| Error::bad("Notification has no request"))?;
        if let Some(provider) = payload["review_provider"].as_str() {
            url.set_path("/requests/received");
            url.query_pairs_mut()
                .append_pair("request", request)
                .append_pair("provider", provider);
        } else {
            // The request route resolves the viewer's authorized sent/received
            // view; recipient lists never encode authority in an email URL.
            url.set_path("/requests");
            url.query_pairs_mut()
                .append_pair("request", request)
                .append_pair("app", app);
        }
        "Open request"
    } else {
        url.set_path("/my-apps");
        "Open Honeycomb"
    };
    Ok((url.into(), action))
}
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn email_html(subject: &str, text: &str, link: &str, action: &str) -> String {
    let title = escape_html(subject);
    let body = text
        .split("\n\n")
        .filter(|part| *part != link)
        .map(|part| {
            format!(
                "<p style=\"margin:0 0 18px;line-height:1.65;color:#475569\">{}</p>",
                escape_html(part).replace('\n', "<br>")
            )
        })
        .collect::<String>();
    let link = escape_html(link);
    format!(
        r##"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"></head><body style="margin:0;background:#f7f9fc;font-family:Arial,Helvetica,sans-serif;color:#101827"><table role="presentation" width="100%" cellspacing="0" cellpadding="0"><tr><td align="center" style="padding:40px 20px"><table role="presentation" width="560" style="width:100%;max-width:560px" cellspacing="0" cellpadding="0"><tr><td style="padding:0 0 24px;font-size:15px;font-weight:700;letter-spacing:2px;color:#2563eb">HONEYCOMB</td></tr><tr><td style="padding:36px;background:#fff;border:1px solid #e4e9f1;border-radius:16px"><h1 style="font-size:25px;line-height:1.3;margin:0 0 20px;letter-spacing:-.5px">{title}</h1>{body}<p style="margin:28px 0"><a href="{link}" style="display:inline-block;background:#2563eb;color:#fff;text-decoration:none;padding:13px 22px;border-radius:8px;font-weight:600">{action}</a></p><p style="font-size:12px;line-height:1.6;color:#64748b">If the button does not open, use this link:<br><a href="{link}" style="color:#2563eb;word-break:break-all">{link}</a></p></td></tr><tr><td style="padding:22px 0;font-size:12px;color:#64748b;line-height:1.6">Silicon Honeycomb · Applications, thoughtfully connected.<br>Sign in to view the current status and available actions.</td></tr></table></td></tr></table></body></html>"##
    )
}

async fn notification_is_current(s: &State, kind: &str, payload: &Value) -> Result<bool> {
    if !matches!(kind, "publication.review" | "publication.status") {
        return Ok(true);
    }
    let Some(id) = payload["request_id"].as_str() else {
        return Ok(true);
    };
    let row = sqlx::query("SELECT p.state,p.revision,a.revision AS current_revision FROM publication_requests p JOIN applications a ON a.plane=p.plane AND a.app_id=p.app_id WHERE p.id=? AND p.plane='production'")
        .bind(id).fetch_optional(&s.db).await?;
    let Some(row) = row else {
        return Ok(false);
    };
    if row.get::<i64, _>("revision") != row.get::<i64, _>("current_revision") {
        return Ok(false);
    }
    let state: String = row.get("state");
    if kind == "publication.status" {
        return Ok(payload["state"].as_str() == Some(state.as_str()));
    }
    let Some(provider) = payload["review_provider"].as_str() else {
        return Ok(true);
    };
    let pending:bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM review_gates WHERE request_id=? AND provider=? AND state='pending')")
        .bind(id).bind(provider).fetch_one(&s.db).await?;
    Ok(pending
        && if provider == "honeycomb" {
            state == "awaiting_validator"
        } else {
            state == "awaiting_scope_review"
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
        sqlx::query("UPDATE outbox SET error='Email delivery is not configured: set POSTMARK_SERVER_TOKEN',next_attempt=? WHERE id=? AND state='pending'")
            .bind(now()+300).bind(&id).execute(&s.db).await?;
        return Ok(true);
    };
    let payload: Value =
        serde_json::from_str(&row.get::<String, _>("payload")).map_err(|e| anyhow::anyhow!(e))?;
    if !notification_is_current(s, &row.get::<String, _>("kind"), &payload).await? {
        sqlx::query(
            "UPDATE outbox SET state='superseded',error=NULL WHERE id=? AND state='pending'",
        )
        .bind(&id)
        .execute(&s.db)
        .await?;
        return Ok(true);
    }
    let email = match render(s, &id, &row.get::<String, _>("kind"), &payload).await {
        Ok(email) => email,
        Err(error) => {
            sqlx::query("UPDATE outbox SET error=?,attempts=attempts+1,next_attempt=? WHERE id=?")
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
pub async fn reconcile_once(s: &State, mailer: &dyn Mailer) -> Result<bool> {
    let id: Option<String> = sqlx::query_scalar("SELECT id FROM outbox WHERE plane='production' AND state='uncertain' AND next_attempt<=? ORDER BY next_attempt,id LIMIT 1")
        .bind(now()).fetch_optional(&s.db).await?;
    let Some(id) = id else {
        return Ok(false);
    };
    if let Some(receipt) = mailer.receipt(&id).await {
        sqlx::query("UPDATE outbox SET state='sent',provider_id=?,error=NULL WHERE id=? AND state='uncertain'")
            .bind(receipt).bind(&id).execute(&s.db).await?;
    } else {
        sqlx::query("UPDATE outbox SET next_attempt=? WHERE id=? AND state='uncertain'")
            .bind(now() + 900)
            .bind(&id)
            .execute(&s.db)
            .await?;
    }
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
        if let Some(mailer) = &mailer
            && let Err(error) = reconcile_once(&s, mailer).await
        {
            tracing::warn!(error=%error.1.message,"Email receipt reconciliation failed");
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
    #[test]
    fn mail_links_and_html_keep_untrusted_content_inert() {
        let (link, action) = notification_link("publication.review",&json!({"request_id":"request&other=bad","app_id":"sample","review_provider":"files>app"})).unwrap();
        let url = url::Url::parse(&link).unwrap();
        assert_eq!(url.path(), "/requests/received");
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "request").unwrap().1,
            "request&other=bad"
        );
        let html = email_html("<img onerror=bad>", "<script>bad</script>", &link, action);
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img"));
        assert!(html.contains("&lt;img"));
        assert!(html.contains("&amp;provider="));
        let (release, _) = notification_link(
            "release.created",
            &json!({"app_id":"waveform","channel":"dev","version":"2.0.0-beta.1"}),
        )
        .unwrap();
        assert_eq!(
            release,
            "https://honeycomb.teamofsilicons.com/apps/waveform/releases/dev/2.0.0-beta.1"
        );
    }
    #[tokio::test]
    async fn receipt_reconciliation_requires_exact_event_metadata() {
        use wiremock::matchers::query_param;
        let server = MockServer::start().await;
        let mailer = Postmark {
            http: reqwest::Client::new(),
            token: "fixture".into(),
            endpoint: format!("{}/email", server.uri()),
        };
        Mock::given(method("GET")).and(path("/messages/outbound")).and(query_param("metadata_honeycomb_event_id","known-event"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Messages":[
                {"MessageID":"wrong","Status":"Sent","Metadata":{"honeycomb_event_id":"different"}},
                {"MessageID":"receipt","Status":"Sent","Metadata":{"honeycomb_event_id":"known-event"}}
            ]}))).mount(&server).await;
        assert_eq!(
            mailer.receipt("known-event").await.as_deref(),
            Some("receipt")
        );
        server.reset().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"Messages":[]})))
            .mount(&server)
            .await;
        assert_eq!(mailer.receipt("known-event").await, None);
    }
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
            html: "<p>No external mail is sent.</p>".into(),
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
