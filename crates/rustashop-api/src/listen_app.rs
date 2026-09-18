//! Bind Actix with Serenade kernel default service plus WebSocket routes.

use std::net::{SocketAddr, ToSocketAddrs};

use actix_web::body::MessageBody;
use actix_web::dev::{ServiceFactory, ServiceRequest, ServiceResponse};
use actix_web::{App, Error, HttpRequest, HttpResponse, HttpServer, web};
use rustashop_persist::CatalogRepository;
use serenade_http::AsyncHttpKernel;
use serenade_http_actix::dispatch_async;

use crate::admin_auth::AdminAuthConfig;
use crate::cart_ws::cart_ws;
use crate::order_ws::order_ws;
use crate::realtime::{CartHub, OrderHub};
use crate::sandbox_realtime::SandboxJobHub;
use crate::sandbox_ws::sandbox_job_ws;

/// Shared Actix app data for commerce listen.
#[derive(Clone)]
pub struct CommerceListenData {
    /// Serenade HTTP kernel.
    pub kernel: web::Data<AsyncHttpKernel>,
    /// Cart push hub.
    pub cart_hub: web::Data<CartHub>,
    /// Order push hub.
    pub order_hub: web::Data<OrderHub>,
    /// Catalog for cart WS auth.
    pub catalog: web::Data<CatalogRepository>,
    /// Sandbox job push hub.
    pub sandbox_hub: web::Data<SandboxJobHub>,
    /// Admin bearer for sandbox / order WS.
    pub admin_auth: web::Data<AdminAuthConfig>,
    /// Operator API path segment (sandbox / order WS path).
    pub admin_prefix: String,
}

/// Builds the production Actix app: optional `OpenAPI` explorers, WS routes, kernel catch-all.
#[must_use]
pub fn commerce_app(
    data: CommerceListenData,
) -> App<
    impl ServiceFactory<
        ServiceRequest,
        Config = (),
        Response = ServiceResponse<impl MessageBody>,
        Error = Error,
        InitError = (),
    >,
> {
    let sandbox_ws_path = format!("/v1/{}/sandbox/jobs/{{id}}/ws", data.admin_prefix);
    let order_ws_path = format!("/v1/{}/orders/{{id}}/ws", data.admin_prefix);
    let app = App::new()
        .app_data(data.kernel)
        .app_data(data.cart_hub)
        .app_data(data.order_hub)
        .app_data(data.catalog)
        .app_data(data.sandbox_hub)
        .app_data(data.admin_auth);
    #[cfg(feature = "openapi-ui")]
    let app = app.configure(|cfg| crate::openapi::configure_openapi_ui(cfg, None));
    app.route("/v1/carts/{id}/ws", web::get().to(cart_ws))
        .route(&sandbox_ws_path, web::get().to(sandbox_job_ws))
        .route(&order_ws_path, web::get().to(order_ws))
        .default_service(web::to(kernel_service))
}

/// Bound commerce server plus the addresses it listens on.
pub struct BoundCommerce {
    /// Actix server future (run with [`serenade_http_actix::await_bound`]).
    pub server: actix_web::dev::Server,
    /// Bound socket addresses.
    pub addrs: Vec<SocketAddr>,
}

/// Binds `addr` with commerce HTTP + WebSocket routes (does not await).
///
/// # Errors
///
/// Propagates bind errors.
pub fn bind_commerce_server(
    addr: impl ToSocketAddrs,
    data: CommerceListenData,
) -> std::io::Result<BoundCommerce> {
    let http = HttpServer::new(move || commerce_app(data.clone())).bind(addr)?;
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
