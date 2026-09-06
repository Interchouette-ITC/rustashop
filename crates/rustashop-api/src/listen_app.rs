//! Bind Actix with Serenade kernel default service plus cart WebSocket route.

use std::net::{SocketAddr, ToSocketAddrs};

use actix_web::body::MessageBody;
use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{web, App, Error, HttpRequest, HttpResponse, HttpServer};
use rustashop_persist::CatalogRepository;
use serenade_http::AsyncHttpKernel;
use serenade_http_actix::dispatch_async;

use crate::cart_ws::cart_ws;
use crate::realtime::CartHub;

/// Builds the production Actix app: cart WS route + Serenade kernel catch-all.
#[must_use]
pub fn commerce_app(
    kernel: web::Data<AsyncHttpKernel>,
    hub: web::Data<CartHub>,
    catalog: web::Data<CatalogRepository>,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<impl MessageBody>,
        Error = Error,
        InitError = (),
    >,
> {
    App::new()
        .app_data(kernel)
        .app_data(hub)
        .app_data(catalog)
        .route("/v1/carts/{id}/ws", web::get().to(cart_ws))
        .default_service(web::to(kernel_service))
}

/// Bound commerce server plus the addresses it listens on.
pub struct BoundCommerce {
    /// Actix server future (run with [`serenade_http_actix::await_bound`]).
    pub server: actix_web::dev::Server,
    /// Bound socket addresses.
    pub addrs: Vec<SocketAddr>,
}

/// Binds `addr` with commerce HTTP + cart WebSocket (does not await).
///
/// # Errors
///
/// Propagates bind errors.
pub fn bind_commerce_server(
    addr: impl ToSocketAddrs,
    kernel: AsyncHttpKernel,
    hub: CartHub,
    catalog: CatalogRepository,
) -> std::io::Result<BoundCommerce> {
    let kernel = web::Data::new(kernel);
    let hub = web::Data::new(hub);
    let catalog = web::Data::new(catalog);
    let http = HttpServer::new(move || commerce_app(kernel.clone(), hub.clone(), catalog.clone()))
        .bind(addr)?;
    let addrs = http.addrs();
    Ok(BoundCommerce {
        server: http.run(),
        addrs,
    })
}

#[allow(clippy::future_not_send)]
async fn kernel_service(
    request: HttpRequest,
    body: web::Bytes,
    kernel: web::Data<AsyncHttpKernel>,
) -> HttpResponse {
    dispatch_async(kernel.get_ref(), &request, body).await
}
