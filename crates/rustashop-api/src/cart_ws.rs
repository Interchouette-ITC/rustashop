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

/// Authorizes a cart socket with the session token.
///
/// Returns `Ok(())` or an HTTP status code to return without upgrading.
async fn authorize_cart_socket(
    catalog: &CatalogRepository,
    cart_id: &str,
    token: &str,
) -> Result<(), u16> {
    if token.is_empty() {
        return Err(401);
    }
    match catalog.find_cart_by_id(cart_id).await {
        Ok(Some(cart)) if cart.token == token => Ok(()),
        Ok(Some(_)) => Err(403),
        Ok(None) => Err(404),
        Err(error) => {
            debug!(%error, "cart ws catalog lookup failed");
            Err(500)
        }
    }
}

fn auth_failure_response(status: u16) -> HttpResponse {
    match status {
        401 => HttpResponse::Unauthorized().finish(),
        403 => HttpResponse::Forbidden().finish(),
        404 => HttpResponse::NotFound().finish(),
        _ => HttpResponse::InternalServerError().finish(),
    }
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
    if let Err(status) = authorize_cart_socket(catalog.get_ref(), &cart_id, &query.token).await {
        return Ok(auth_failure_response(status));
    }

    let (response, session, msg_stream) = actix_ws::handle(&req, stream)?;
    let events = hub.subscribe(&cart_id);

    actix_web::rt::spawn(async move {
        run_cart_ws_session(session, msg_stream, events).await;
    });

    Ok(response)
}

#[allow(clippy::future_not_send)]
async fn run_cart_ws_session(
    mut session: actix_ws::Session,
    mut msg_stream: actix_ws::MessageStream,
    mut events: tokio::sync::broadcast::Receiver<String>,
) {
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
}
