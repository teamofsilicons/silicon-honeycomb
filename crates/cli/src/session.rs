//! Serialize session rotation across CLI processes and the update daemon.
use super::{now, read, write_private};
use anyhow::{Context, Result, bail};
use fs2::FileExt;
use honeycomb_client::{Client, Mutation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Session {
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_refresh: Option<RefreshAttempt>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct RefreshAttempt {
    key: String,
    started_at: i64,
}

impl Session {
    pub fn from_tokens(tokens: &Value, started_at: i64) -> Result<Self> {
        let access = tokens["access_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("IAM returned no access token; the saved session was retained")?;
        let seconds = tokens["expires_in"].as_i64().filter(|s| *s > 0).context(
            "IAM returned an invalid access token lifetime; the saved session was retained",
        )?;
        let refresh = tokens["refresh_token"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("IAM returned no refresh token; retry to recover the saved session")?;
        Ok(Self {
            access_token: Some(access.into()),
            refresh_token: Some(refresh.to_owned()),
            expires_at: started_at
                .checked_add(seconds)
                .context("Invalid access token lifetime")?,
            pending_refresh: None,
        })
    }
}

pub(crate) struct Store {
    // Lock a separate inode: session.json is replaced atomically on every save.
    _lock: fs::File,
    path: PathBuf,
    pub session: Session,
}

impl Store {
    pub fn open(directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("session.lock"))?;
        lock.lock_exclusive()?;
        let path = directory.join("session.json");
        // Always reload after acquiring the lock; another process may have rotated it.
        let session = read(&path)?;
        Ok(Self {
            _lock: lock,
            path,
            session,
        })
    }

    pub fn save(&mut self, session: Session) -> Result<()> {
        write_private(&self.path, &session)?;
        self.session = session;
        Ok(())
    }

    pub fn clear(&mut self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        self.session = Session::default();
        Ok(())
    }

    /// Validate before dispatching a command. A server-side rejection can precede
    /// the cached expiry; renew once without replaying a command or its input.
    pub async fn verified(&mut self, client: &Client) -> Result<Option<Value>> {
        self.renew(client).await?;
        let Some(access) = self.session.access_token.clone() else {
            return Ok(None);
        };
        let status = client.with_token(&access).login_status().await;
        let rejected = match &status {
            Ok(value) => value["authenticated"] == false,
            Err(error) => error
                .downcast_ref::<honeycomb_client::ApiError>()
                .is_some_and(|error| error.status == 401),
        };
        if !rejected || self.session.refresh_token.is_none() {
            return status.map(Some);
        }
        self.session.expires_at = 0;
        // renew persists the same pending attempt before sending and saves the successor.
        self.renew(client).await?;
        Ok(Some(
            client
                .with_token(
                    self.session
                        .access_token
                        .as_deref()
                        .expect("renewed access"),
                )
                .login_status()
                .await?,
        ))
    }

    pub async fn renew(&mut self, client: &Client) -> Result<()> {
        // A replay after a long offline period can return an already-expired access token.
        // Save its rotated refresh token first, then renew once more with a new operation.
        for _ in 0..2 {
            if self.session.pending_refresh.is_none() && self.session.expires_at > now() + 30 {
                return Ok(());
            }
            let Some(refresh) = self.session.refresh_token.clone() else {
                return Ok(());
            };
            if self.session.pending_refresh.is_none() {
                self.session.pending_refresh = Some(RefreshAttempt {
                    key: uuid::Uuid::new_v4().to_string(),
                    started_at: now(),
                });
                // Persist before sending: a timeout, dropped response or process exit must
                // retry the same rotation, never reuse the credential with a new key.
                write_private(&self.path, &self.session)?;
            }
            let attempt = self
                .session
                .pending_refresh
                .as_ref()
                .expect("saved refresh attempt");
            let tokens = client
                .refresh(
                    &refresh,
                    &Mutation {
                        idempotency_key: attempt.key.clone(),
                        revision: None,
                    },
                )
                .await?;
            let session = Session::from_tokens(&tokens, attempt.started_at)?;
            self.save(session)?;
            if self.session.expires_at > now() {
                return Ok(());
            }
        }
        bail!("IAM returned an expired access token; retry to renew the saved session")
    }
}
