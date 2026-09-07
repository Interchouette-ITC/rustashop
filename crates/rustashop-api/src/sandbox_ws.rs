//! Actix WebSocket endpoint for sandbox job logs.

use actix_web::{HttpRequest, HttpResponse, web};

use crate::admin_auth::AdminAuthConfig;
use crate::cart_ws_session::run_cart_ws_session;
use crate::sandbox_realtime::SandboxJobHub;

/// Upgrades sandbox job WS with admin bearer auth via `?token=`.
///
/// # Errors
///
/// Returns an Actix error when the WebSocket handshake fails.
#[allow(clippy::future_not_send)]
pub async fn sandbox_job_ws(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
    query: web::Query<SandboxWsQuery>,
    hub: web::Data<SandboxJobHub>,
    auth: web::Data<AdminAuthConfig>,
) -> Result<HttpResponse, actix_web::Error> {
    let job_id = path.into_inner();
    if auth.authorize_bearer(Some(query.token.as_str())).is_err() {
        return Ok(HttpResponse::Unauthorized().finish());
    }

    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;
    let events = hub.subscribe(&job_id);

    actix_web::rt::spawn(async move {
        run_cart_ws_session(session, msg_stream, events).await;
    });

    Ok(response)
}

/// Query string for sandbox job socket auth (`?token=`).
#[derive(Debug, serde::Deserialize)]
pub struct SandboxWsQuery {
    /// Admin API bearer secret.
    pub token: String,
}
