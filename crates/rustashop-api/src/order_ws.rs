//! Actix WebSocket endpoint for admin order status push.

use actix_web::{HttpRequest, HttpResponse, web};

use crate::admin_auth::AdminAuthConfig;
use crate::cart_ws_session::run_cart_ws_session;
use crate::realtime::OrderHub;

/// Query string for order socket auth (`?token=` = admin bearer).
#[derive(Debug, serde::Deserialize)]
pub struct OrderWsQuery {
    /// Admin API bearer secret.
    pub token: String,
}

/// Upgrades `GET /v1/{admin}/orders/{id}/ws` and streams `order.updated` JSON events.
///
/// # Errors
///
/// Returns an Actix error when the WebSocket handshake fails.
#[allow(clippy::future_not_send)]
pub async fn order_ws(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
    query: web::Query<OrderWsQuery>,
    hub: web::Data<OrderHub>,
    auth: web::Data<AdminAuthConfig>,
) -> Result<HttpResponse, actix_web::Error> {
    let order_id = path.into_inner();
    if auth.authorize_bearer(Some(query.token.as_str())).is_err() {
        return Ok(HttpResponse::Unauthorized().finish());
    }

    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;
    let events = hub.subscribe(&order_id);

    actix_web::rt::spawn(async move {
        run_cart_ws_session(session, msg_stream, events).await;
    });

    Ok(response)
}
