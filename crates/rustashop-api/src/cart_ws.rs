//! Actix WebSocket endpoint for cart session push.

use actix_web::{web, HttpRequest, HttpResponse};
use rustashop_persist::CatalogRepository;
use serde::Deserialize;
use tracing::debug;

use crate::cart_ws_session::run_cart_ws_session;
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

#[cfg(test)]
mod tests {
    use super::auth_failure_response;

    #[test]
    fn auth_failure_maps_known_statuses() {
        assert_eq!(auth_failure_response(401).status(), 401);
        assert_eq!(auth_failure_response(403).status(), 403);
        assert_eq!(auth_failure_response(404).status(), 404);
        assert_eq!(auth_failure_response(500).status(), 500);
        assert_eq!(auth_failure_response(418).status(), 500);
    }
}
