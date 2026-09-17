//! Cross-process job status mirror (Redis) for external workers.
//!
//! Audit rows stay process-local. Admin `GET …/jobs/{id}` reads Redis when the
//! in-memory map misses (API and worker are different processes).

use crate::registry::SandboxJobResponse;

#[cfg(feature = "redis")]
use crate::messenger::MESSENGER_REDIS_URL_ENV;

#[cfg(feature = "redis")]
const JOB_KEY_PREFIX: &str = "rustashop:sandbox:job:";

#[cfg(feature = "redis")]
fn job_key(job_id: &str) -> String {
    format!("{JOB_KEY_PREFIX}{job_id}")
}

/// Writes job JSON when Redis URL is set (feature `redis`).
#[allow(clippy::missing_const_for_fn)]
pub fn persist_job(response: &SandboxJobResponse) {
    #[cfg(feature = "redis")]
    {
        if let Ok(url) = std::env::var(MESSENGER_REDIS_URL_ENV)
            && let Err(error) = persist_redis(&url, response)
        {
            tracing::warn!(%error, job_id = %response.id, "sandbox job status: redis persist failed");
        }
    }
    #[cfg(not(feature = "redis"))]
    {
        let _ = response;
    }
}

/// Loads job JSON from Redis when configured; `None` on miss or error.
#[must_use]
#[allow(clippy::missing_const_for_fn)]
pub fn load_job(job_id: &str) -> Option<SandboxJobResponse> {
    #[cfg(feature = "redis")]
    {
        let url = std::env::var(MESSENGER_REDIS_URL_ENV).ok()?;
        match load_redis(&url, job_id) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, %job_id, "sandbox job status: redis load failed");
                None
            }
        }
    }
    #[cfg(not(feature = "redis"))]
    {
        let _ = job_id;
        None
    }
}

#[cfg(feature = "redis")]
fn persist_redis(url: &str, response: &SandboxJobResponse) -> Result<(), String> {
    use redis::Commands;
    let client = redis::Client::open(url).map_err(|error| error.to_string())?;
    let mut conn = client.get_connection().map_err(|error| error.to_string())?;
    let payload = serde_json::to_string(response).map_err(|error| error.to_string())?;
    let _: () = conn
        .set(job_key(&response.id), payload)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(feature = "redis")]
fn load_redis(url: &str, job_id: &str) -> Result<Option<SandboxJobResponse>, String> {
    use redis::Commands;
    let client = redis::Client::open(url).map_err(|error| error.to_string())?;
    let mut conn = client.get_connection().map_err(|error| error.to_string())?;
    let raw: Option<String> = conn
        .get(job_key(job_id))
        .map_err(|error| error.to_string())?;
    raw.map(|json| serde_json::from_str(&json).map_err(|error| error.to_string()))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::SandboxJobStatus;

    #[test]
    fn persist_and_load_noop_without_redis_feature_or_url() {
        let response = SandboxJobResponse {
            id: "job-1".into(),
            job_type: "quote".into(),
            status: SandboxJobStatus::Running,
            source_hash: "h".into(),
            adjustments: None,
            proposal: None,
            error: None,
        };
        persist_job(&response);
        assert!(load_job("job-1").is_none());
    }
}
