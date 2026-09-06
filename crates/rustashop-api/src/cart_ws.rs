//! Actix WebSocket endpoint for cart session push.

use actix_web::{web, HttpRequest, HttpResponse};
use actix_ws::Message;
use rustashop_persist::CatalogRepository;
use serde::Deserialize;
use tracing::debug;

use crate::realtime::CartHub;

/// Query string for cart socket auth (`?token=`).
#[derive(Debug, Deserialize)]
pub struct CartWsQuery {
    /// Opaque cart session token from `CartResponse.token`.
    pub token: String,
}

/// Upgrades `GET /v1/carts/{id}/ws` and streams `cart.updated` JSON events.
///
/// # Errors
///
/// Returns an Actix error when the WebSocket handshake fails.
#[allow(clippy::future_not_send)]
pub async fn cart_ws(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
    query: web::Query<CartWsQuery>,
    hub: web::Data<CartHub>,
    catalog: web::Data<CatalogRepository>,
) -> Result<HttpResponse, actix_web::Error> {
    let cart_id = path.into_inner();
    if query.token.is_empty() {
        return Ok(HttpResponse::Unauthorized().finish());
    }
    match catalog.find_cart_by_id(&cart_id).await {
        Ok(Some(cart)) if cart.token == query.token => {}
        Ok(Some(_)) => return Ok(HttpResponse::Forbidden().finish()),
        Ok(None) => return Ok(HttpResponse::NotFound().finish()),
        Err(error) => {
            debug!(%error, "cart ws catalog lookup failed");
            return Ok(HttpResponse::InternalServerError().finish());
        }
    }

    let (response, mut session, mut msg_stream) = actix_ws::handle(&req, stream)?;
    let mut events = hub.subscribe(&cart_id);

    actix_web::rt::spawn(async move {
        loop {
            tokio::select! {
                msg = msg_stream.recv() => {
                    match msg {
                        Some(Ok(Message::Ping(bytes))) => {
                            if session.pong(&bytes).await.is_err() {
                                break;
                            }
                        }
                        Some(Ok(Message::Close(_)) | Err(_)) | None => break,
                        Some(Ok(_)) => {}
                    }
                }
                event = events.recv() => {
                    match event {
                        Ok(payload) => {
                            if session.text(payload).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    }
                }
            }
        }
        let _ = session.close(None).await;
    });

    Ok(response)
}
