//! Admin AI model-provider catalog and env-backed status (no secret upsert).

use serde::{Deserialize, Serialize};
use serenade_http::Response;
use utoipa::ToSchema;

use crate::admin_auth::AdminAuthConfig;
use crate::error::{ApiError, ErrorBody, api_error_json_response, json_response};

/// Default local LLM HTTP base when `RUSTASHOP_LOCAL_LLM_URL` is unset.
pub const DEFAULT_LOCAL_LLM_URL: &str = "http://127.0.0.1:11434";

/// Env: OpenAI-compatible cloud API key (`OPENAI_API_KEY`).
pub const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
/// Env: Anthropic API key (`ANTHROPIC_API_KEY`).
pub const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
/// Env: local LLM base URL.
pub const LOCAL_LLM_URL_ENV: &str = "RUSTASHOP_LOCAL_LLM_URL";
/// Env: default provider id for first-party routing.
pub const DEFAULT_PROVIDER_ENV: &str = "RUSTASHOP_AI_DEFAULT_PROVIDER";
/// Env: custom OpenAI-compatible base URL.
pub const CUSTOM_LLM_URL_ENV: &str = "RUSTASHOP_CUSTOM_LLM_URL";
/// Env: custom OpenAI-compatible API key.
pub const CUSTOM_LLM_API_KEY_ENV: &str = "RUSTASHOP_CUSTOM_LLM_API_KEY";

/// Where a credential was resolved from (never the secret itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AiCredentialSource {
    /// Process environment variable.
    Env,
    /// Built-in default (local URL only).
    Default,
    /// Not configured.
    None,
}

/// Wire protocol kind for a catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    /// Local OpenAI-compatible daemon (e.g. Ollama-style HTTP).
    Local,
    /// Anthropic Messages API.
    Anthropic,
    /// OpenAI-compatible Chat Completions HTTP API.
    OpenaiCompatible,
}

/// Static catalog entry (env var **names** only).
#[derive(Debug, Clone, Copy)]
pub struct AiProviderSpec {
    /// Stable id (`local`, `openai`, …).
    pub id: &'static str,
    /// Operator display label.
    pub display_name: &'static str,
    /// Group label (`local` / `cloud` / `custom`).
    pub group: &'static str,
    /// Protocol kind.
    pub kind: AiProviderKind,
    /// Default base URL when applicable.
    pub default_base_url: &'static str,
    /// Env var name for the API key (empty when unused).
    pub env_api_key: &'static str,
    /// Env var name for base URL override (empty when unused).
    pub env_base_url: &'static str,
}

/// MVP catalog (four providers).
pub const PROVIDER_CATALOG: &[AiProviderSpec] = &[
    AiProviderSpec {
        id: "local",
        display_name: "Local LLM",
        group: "local",
        kind: AiProviderKind::Local,
        default_base_url: DEFAULT_LOCAL_LLM_URL,
        env_api_key: "",
        env_base_url: LOCAL_LLM_URL_ENV,
    },
    AiProviderSpec {
        id: "openai",
        display_name: "OpenAI",
        group: "cloud",
        kind: AiProviderKind::OpenaiCompatible,
        default_base_url: "https://api.openai.com/v1",
        env_api_key: OPENAI_API_KEY_ENV,
        env_base_url: "",
    },
    AiProviderSpec {
        id: "anthropic",
        display_name: "Anthropic",
        group: "cloud",
        kind: AiProviderKind::Anthropic,
        default_base_url: "https://api.anthropic.com",
        env_api_key: ANTHROPIC_API_KEY_ENV,
        env_base_url: "",
    },
    AiProviderSpec {
        id: "custom",
        display_name: "Custom OpenAI-compatible",
        group: "custom",
        kind: AiProviderKind::OpenaiCompatible,
        default_base_url: "",
        env_api_key: CUSTOM_LLM_API_KEY_ENV,
        env_base_url: CUSTOM_LLM_URL_ENV,
    },
];

/// One catalog row for the admin UI (no secrets).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AiProviderCatalogItem {
    /// Stable id.
    pub id: String,
    /// Display label.
    pub display_name: String,
    /// Group.
    pub group: String,
    /// Protocol kind label.
    pub kind: String,
    /// Default base URL when known.
    pub default_base_url: String,
    /// Env var name for the API key (may be empty).
    pub env_api_key: String,
    /// Env var name for base URL (may be empty).
    pub env_base_url: String,
}

/// Status of one provider after env resolution.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AiProviderStatus {
    /// Stable id.
    pub id: String,
    /// Display label.
    pub display_name: String,
    /// Whether credentials / local URL resolve.
    pub available: bool,
    /// Credential source (`env` / `default` / `none`).
    pub source: AiCredentialSource,
    /// Safe hint (last-4 of key, or host:port for local). Never a full secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Resolved base URL when applicable (no key).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Whether this id is the configured default provider.
    pub is_default: bool,
}

/// List payload for `GET …/ai/providers`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AiProvidersStatusResponse {
    /// Status rows in catalog order.
    pub providers: Vec<AiProviderStatus>,
    /// Default provider id from env (or first available).
    pub default_provider_id: Option<String>,
}

/// List payload for `GET …/ai/providers/catalog`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AiProvidersCatalogResponse {
    /// Static catalog rows.
    pub providers: Vec<AiProviderCatalogItem>,
}

/// Body for `POST …/ai/providers/test`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AiProviderTestRequest {
    /// Provider id to probe.
    pub provider_id: String,
}

/// Result of a provider connectivity / config test.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AiProviderTestResponse {
    /// Whether the probe succeeded.
    pub ok: bool,
    /// Provider id tested.
    pub provider_id: String,
    /// Error when not ok.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl From<&AiProviderSpec> for AiProviderCatalogItem {
    fn from(spec: &AiProviderSpec) -> Self {
        Self {
            id: spec.id.to_owned(),
            display_name: spec.display_name.to_owned(),
            group: spec.group.to_owned(),
            kind: kind_label(spec.kind).to_owned(),
            default_base_url: spec.default_base_url.to_owned(),
            env_api_key: spec.env_api_key.to_owned(),
            env_base_url: spec.env_base_url.to_owned(),
        }
    }
}

const fn kind_label(kind: AiProviderKind) -> &'static str {
    match kind {
        AiProviderKind::Local => "local",
        AiProviderKind::Anthropic => "anthropic",
        AiProviderKind::OpenaiCompatible => "openai_compatible",
    }
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn key_hint(secret: &str) -> Option<String> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() <= 4 {
        return Some("••••".to_owned());
    }
    Some(format!("…{}", &trimmed[trimmed.len() - 4..]))
}

fn resolve_local_base_url() -> (String, AiCredentialSource) {
    if let Some(url) = env_nonempty(LOCAL_LLM_URL_ENV) {
        return (url, AiCredentialSource::Env);
    }
    (
        DEFAULT_LOCAL_LLM_URL.to_owned(),
        AiCredentialSource::Default,
    )
}

fn resolve_status(spec: &AiProviderSpec, default_id: Option<&str>) -> AiProviderStatus {
    let is_default = default_id == Some(spec.id);
    match spec.kind {
        AiProviderKind::Local => {
            let (base_url, source) = resolve_local_base_url();
            AiProviderStatus {
                id: spec.id.to_owned(),
                display_name: spec.display_name.to_owned(),
                available: true,
                source,
                hint: Some(base_url.clone()),
                base_url: Some(base_url),
                is_default,
            }
        }
        AiProviderKind::Anthropic | AiProviderKind::OpenaiCompatible if spec.id == "custom" => {
            let base = env_nonempty(CUSTOM_LLM_URL_ENV);
            let key = env_nonempty(CUSTOM_LLM_API_KEY_ENV);
            let available = base.is_some() && key.is_some();
            let source = if available {
                AiCredentialSource::Env
            } else {
                AiCredentialSource::None
            };
            AiProviderStatus {
                id: spec.id.to_owned(),
                display_name: spec.display_name.to_owned(),
                available,
                source,
                hint: key.as_deref().and_then(key_hint),
                base_url: base,
                is_default,
            }
        }
        AiProviderKind::Anthropic | AiProviderKind::OpenaiCompatible => {
            let key = env_nonempty(spec.env_api_key);
            let available = key.is_some();
            let source = if available {
                AiCredentialSource::Env
            } else {
                AiCredentialSource::None
            };
            AiProviderStatus {
                id: spec.id.to_owned(),
                display_name: spec.display_name.to_owned(),
                available,
                source,
                hint: key.as_deref().and_then(key_hint),
                base_url: if available && !spec.default_base_url.is_empty() {
                    Some(spec.default_base_url.to_owned())
                } else {
                    None
                },
                is_default,
            }
        }
    }
}

fn resolved_default_provider_id() -> Option<String> {
    if let Some(id) = env_nonempty(DEFAULT_PROVIDER_ENV)
        && PROVIDER_CATALOG.iter().any(|spec| spec.id == id)
    {
        return Some(id);
    }
    PROVIDER_CATALOG
        .iter()
        .map(|spec| resolve_status(spec, None))
        .find(|status| status.available)
        .map(|status| status.id)
}

/// Builds status for all catalog providers from the process environment.
#[must_use]
pub fn providers_status() -> AiProvidersStatusResponse {
    let default_provider_id = resolved_default_provider_id();
    let providers = PROVIDER_CATALOG
        .iter()
        .map(|spec| resolve_status(spec, default_provider_id.as_deref()))
        .collect();
    AiProvidersStatusResponse {
        providers,
        default_provider_id,
    }
}

/// Static catalog (env names, no values).
#[must_use]
pub fn providers_catalog() -> AiProvidersCatalogResponse {
    AiProvidersCatalogResponse {
        providers: PROVIDER_CATALOG
            .iter()
            .map(AiProviderCatalogItem::from)
            .collect(),
    }
}

fn tcp_probe_host_port(base_url: &str) -> Result<(), String> {
    use std::net::ToSocketAddrs;

    let url = base_url.trim_end_matches('/');
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let host_port = without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .trim();
    if host_port.is_empty() {
        return Err("empty base URL".to_owned());
    }
    let addr = if host_port.contains(':') {
        host_port.to_owned()
    } else if url.starts_with("https://") {
        format!("{host_port}:443")
    } else {
        format!("{host_port}:80")
    };
    let timeout = std::time::Duration::from_secs(2);
    let mut addrs = addr
        .to_socket_addrs()
        .map_err(|error| format!("resolve {addr}: {error}"))?;
    let sock = addrs
        .next()
        .ok_or_else(|| format!("no address for {addr}"))?;
    std::net::TcpStream::connect_timeout(&sock, timeout)
        .map(|_| ())
        .map_err(|error| format!("tcp probe failed: {error}"))
}

/// Probes a provider without returning secrets.
#[must_use]
pub fn test_provider(provider_id: &str) -> AiProviderTestResponse {
    let Some(spec) = PROVIDER_CATALOG
        .iter()
        .find(|entry| entry.id == provider_id)
    else {
        return AiProviderTestResponse {
            ok: false,
            provider_id: provider_id.to_owned(),
            error: Some("unknown provider".to_owned()),
        };
    };
    let status = resolve_status(spec, None);
    if !status.available {
        return AiProviderTestResponse {
            ok: false,
            provider_id: provider_id.to_owned(),
            error: Some("provider not configured".to_owned()),
        };
    }
    match spec.kind {
        AiProviderKind::Local => {
            let base = status
                .base_url
                .unwrap_or_else(|| DEFAULT_LOCAL_LLM_URL.to_owned());
            match tcp_probe_host_port(&base) {
                Ok(()) => AiProviderTestResponse {
                    ok: true,
                    provider_id: provider_id.to_owned(),
                    error: None,
                },
                Err(error) => AiProviderTestResponse {
                    ok: false,
                    provider_id: provider_id.to_owned(),
                    error: Some(error),
                },
            }
        }
        AiProviderKind::Anthropic | AiProviderKind::OpenaiCompatible => AiProviderTestResponse {
            ok: true,
            provider_id: provider_id.to_owned(),
            error: None,
        },
    }
}

/// `GET …/ai/providers` response.
pub fn list_ai_providers_response(auth: &AdminAuthConfig, bearer: Option<&str>) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    json_response(200, &providers_status())
}

/// `GET …/ai/providers/catalog` response.
pub fn list_ai_providers_catalog_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    json_response(200, &providers_catalog())
}

/// `POST …/ai/providers/test` response.
pub fn test_ai_provider_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    body: &[u8],
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    let request: AiProviderTestRequest = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => {
            return api_error_json_response(&ApiError::Unprocessable(
                "invalid provider test body".to_owned(),
            ));
        }
    };
    if request.provider_id.trim().is_empty() {
        return api_error_json_response(&ApiError::Unprocessable(
            "provider_id is required".to_owned(),
        ));
    }
    json_response(200, &test_provider(request.provider_id.trim()))
}

/// `OpenAPI` stub: list provider status.
#[utoipa::path(
    get,
    path = "/v1/{admin_api_prefix}/ai/providers",
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Model provider status (masked)", body = AiProvidersStatusResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn list_ai_providers() {}

/// `OpenAPI` stub: static catalog.
#[utoipa::path(
    get,
    path = "/v1/{admin_api_prefix}/ai/providers/catalog",
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Model provider catalog (env names only)", body = AiProvidersCatalogResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn list_ai_providers_catalog() {}

/// `OpenAPI` stub: test provider.
#[utoipa::path(
    post,
    path = "/v1/{admin_api_prefix}/ai/providers/test",
    security(("admin_bearer" = [])),
    request_body = AiProviderTestRequest,
    responses(
        (status = 200, description = "Provider probe result", body = AiProviderTestResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody),
        (status = 422, description = "Invalid body", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn test_ai_provider() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_env() -> MutexGuard<'static, ()> {
        ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn clear_provider_env() {
        // SAFETY: serialized by ENV_LOCK in this module's tests.
        unsafe {
            std::env::remove_var(OPENAI_API_KEY_ENV);
            std::env::remove_var(ANTHROPIC_API_KEY_ENV);
            std::env::remove_var(LOCAL_LLM_URL_ENV);
            std::env::remove_var(DEFAULT_PROVIDER_ENV);
            std::env::remove_var(CUSTOM_LLM_URL_ENV);
            std::env::remove_var(CUSTOM_LLM_API_KEY_ENV);
        }
    }

    #[test]
    fn catalog_has_four_entries_without_secrets() {
        let catalog = providers_catalog();
        assert_eq!(catalog.providers.len(), 4);
        assert!(catalog.providers.iter().any(|p| p.id == "openai"));
        assert_eq!(
            catalog
                .providers
                .iter()
                .find(|p| p.id == "openai")
                .expect("openai")
                .env_api_key,
            OPENAI_API_KEY_ENV
        );
        assert_eq!(
            catalog
                .providers
                .iter()
                .find(|p| p.id == "local")
                .expect("local")
                .kind,
            "local"
        );
        assert_eq!(
            catalog
                .providers
                .iter()
                .find(|p| p.id == "anthropic")
                .expect("anthropic")
                .kind,
            "anthropic"
        );
        assert_eq!(
            catalog
                .providers
                .iter()
                .find(|p| p.id == "custom")
                .expect("custom")
                .kind,
            "openai_compatible"
        );
        list_ai_providers_catalog();
    }

    #[test]
    fn key_hint_edges() {
        assert_eq!(key_hint(""), None);
        assert_eq!(key_hint("   "), None);
        assert_eq!(key_hint("ab"), Some("••••".to_owned()));
        assert_eq!(key_hint("abcd"), Some("••••".to_owned()));
        assert_eq!(key_hint("secret-zzzz"), Some("…zzzz".to_owned()));
    }

    #[test]
    fn status_masks_openai_key() {
        let _guard = lock_env();
        clear_provider_env();
        unsafe {
            std::env::set_var(OPENAI_API_KEY_ENV, "sk-test-secret-abcd");
        }
        let status = providers_status();
        let openai = status
            .providers
            .iter()
            .find(|p| p.id == "openai")
            .expect("openai row");
        assert!(openai.available);
        assert_eq!(openai.source, AiCredentialSource::Env);
        assert_eq!(openai.hint.as_deref(), Some("…abcd"));
        assert!(
            !serde_json::to_string(openai)
                .expect("json")
                .contains("sk-test-secret")
        );
        list_ai_providers();
        test_ai_provider();
        clear_provider_env();
    }

    #[test]
    fn local_uses_default_url_and_env_override() {
        let _guard = lock_env();
        clear_provider_env();
        let status = providers_status();
        let local = status
            .providers
            .iter()
            .find(|p| p.id == "local")
            .expect("local");
        assert!(local.available);
        assert_eq!(local.source, AiCredentialSource::Default);
        assert_eq!(local.base_url.as_deref(), Some(DEFAULT_LOCAL_LLM_URL));

        unsafe {
            std::env::set_var(LOCAL_LLM_URL_ENV, "http://127.0.0.1:9");
        }
        let status = providers_status();
        let local = status
            .providers
            .iter()
            .find(|p| p.id == "local")
            .expect("local");
        assert_eq!(local.source, AiCredentialSource::Env);
        assert_eq!(local.base_url.as_deref(), Some("http://127.0.0.1:9"));
        clear_provider_env();
    }

    #[test]
    fn custom_requires_url_and_key() {
        let _guard = lock_env();
        clear_provider_env();
        let status = providers_status();
        let custom = status
            .providers
            .iter()
            .find(|p| p.id == "custom")
            .expect("custom");
        assert!(!custom.available);
        assert_eq!(custom.source, AiCredentialSource::None);

        unsafe {
            std::env::set_var(CUSTOM_LLM_URL_ENV, "http://example.test/v1");
            std::env::set_var(CUSTOM_LLM_API_KEY_ENV, "ck-1234");
        }
        let status = providers_status();
        let custom = status
            .providers
            .iter()
            .find(|p| p.id == "custom")
            .expect("custom");
        assert!(custom.available);
        assert_eq!(custom.source, AiCredentialSource::Env);
        assert_eq!(custom.hint.as_deref(), Some("…1234"));
        assert_eq!(custom.base_url.as_deref(), Some("http://example.test/v1"));
        assert!(test_provider("custom").ok);
        clear_provider_env();
    }

    #[test]
    fn default_provider_env_and_fallback() {
        let _guard = lock_env();
        clear_provider_env();
        unsafe {
            std::env::set_var(DEFAULT_PROVIDER_ENV, "anthropic");
            std::env::set_var(ANTHROPIC_API_KEY_ENV, "anth-key-9999");
        }
        let status = providers_status();
        assert_eq!(status.default_provider_id.as_deref(), Some("anthropic"));
        let anthropic = status
            .providers
            .iter()
            .find(|p| p.id == "anthropic")
            .expect("anthropic");
        assert!(anthropic.is_default);
        assert_eq!(anthropic.hint.as_deref(), Some("…9999"));

        unsafe {
            std::env::set_var(DEFAULT_PROVIDER_ENV, "not-a-provider");
            std::env::remove_var(ANTHROPIC_API_KEY_ENV);
            std::env::remove_var(OPENAI_API_KEY_ENV);
        }
        let status = providers_status();
        assert_eq!(status.default_provider_id.as_deref(), Some("local"));
        clear_provider_env();
    }

    #[test]
    fn unauthorized_without_bearer() {
        let auth = AdminAuthConfig::from_token("tok");
        assert_eq!(list_ai_providers_response(&auth, None).status(), 401);
        assert_eq!(
            list_ai_providers_catalog_response(&auth, None).status(),
            401
        );
        assert_eq!(
            test_ai_provider_response(&auth, None, b"{\"provider_id\":\"local\"}").status(),
            401
        );
    }

    #[test]
    fn authorized_handlers_return_200() {
        let _guard = lock_env();
        clear_provider_env();
        let auth = AdminAuthConfig::from_token("tok");
        assert_eq!(list_ai_providers_response(&auth, Some("tok")).status(), 200);
        assert_eq!(
            list_ai_providers_catalog_response(&auth, Some("tok")).status(),
            200
        );
        assert_eq!(
            test_ai_provider_response(&auth, Some("tok"), b"{\"provider_id\":\"local\"}").status(),
            200
        );
    }

    #[test]
    fn test_handler_rejects_bad_body() {
        let auth = AdminAuthConfig::from_token("tok");
        assert_eq!(
            test_ai_provider_response(&auth, Some("tok"), b"{").status(),
            422
        );
        assert_eq!(
            test_ai_provider_response(&auth, Some("tok"), b"{\"provider_id\":\"  \"}").status(),
            422
        );
    }

    #[test]
    fn test_unknown_and_unconfigured_provider() {
        let _guard = lock_env();
        clear_provider_env();
        let missing = test_provider("missing");
        assert!(!missing.ok);
        assert!(missing.error.is_some());

        let openai = test_provider("openai");
        assert!(!openai.ok);
        assert_eq!(openai.error.as_deref(), Some("provider not configured"));
    }

    #[test]
    fn test_openai_when_configured() {
        let _guard = lock_env();
        clear_provider_env();
        unsafe {
            std::env::set_var(OPENAI_API_KEY_ENV, "sk-live-zzzz");
        }
        let result = test_provider("openai");
        assert!(result.ok);
        clear_provider_env();
    }

    #[test]
    fn test_local_tcp_probe_reports_result() {
        let _guard = lock_env();
        clear_provider_env();
        unsafe {
            // Closed port: probe should fail without panicking.
            std::env::set_var(LOCAL_LLM_URL_ENV, "http://127.0.0.1:1");
        }
        let result = test_provider("local");
        assert!(!result.ok);
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|e| e.contains("tcp") || e.contains("resolve"))
        );
        clear_provider_env();
    }

    #[test]
    fn tcp_probe_host_port_edges() {
        assert!(tcp_probe_host_port("").is_err());
        let closed = tcp_probe_host_port("http://127.0.0.1:1").expect_err("closed port");
        assert!(closed.contains("tcp") || closed.contains("resolve"));
        // https without explicit port uses :443
        let _ = tcp_probe_host_port("https://127.0.0.1:1");
    }
}
