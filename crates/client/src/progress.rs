//! Optional progress notifications. Existing client operations remain silent.
use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug)]
pub enum ProgressEvent {
    Stage(&'static str),
    Download { received: u64, total: Option<u64> },
}

pub(crate) async fn download(
    mut response: reqwest::Response,
    limit: usize,
    total: Option<u64>,
    progress: &(dyn Fn(ProgressEvent) + Sync),
) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        bail!("Release download returned HTTP {}", response.status());
    }
    if response.content_length().is_some_and(|n| n > limit as u64) {
        bail!("Release response exceeds size limit");
    }
    let total = total.or_else(|| response.content_length());
    progress(ProgressEvent::Download { received: 0, total });
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len() + chunk.len() > limit {
            bail!("Release response exceeds size limit");
        }
        bytes.extend_from_slice(&chunk);
        progress(ProgressEvent::Download {
            received: bytes.len() as u64,
            total,
        });
    }
    Ok(bytes)
}
