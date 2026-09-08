//! Serenade HTTP front controller for commerce JSON and cart WebSocket routes.

use std::path::PathBuf;

use rustashop_persist::CatalogRepository;
use serenade_http::{
    AsyncHttpKernel, Method, Request, Response, Route, RouteCollection, UrlMatcher, box_future,
};
use serenade_http_actix::{conversion_error, from_actix, to_actix};

use crate::admin_auth::{AdminAuthConfig, bearer_from_headers};
use crate::admin_orders::{
    ListOrdersQuery, list_admin_orders_response, patch_admin_order_response,
};
use crate::admin_prefix::DEFAULT_ADMIN_API_PREFIX;
use crate::admin_products::{ListAdminProductsQuery, list_admin_products_response};
use crate::carts::{
    add_cart_line_response, create_cart_response, delete_cart_line_response, get_cart_response,
    update_cart_line_response,
};
use crate::checkout::{idempotency_key_from_headers, place_order_response};
use crate::error::{ApiError, api_error_json_response};
use crate::health::health_json_body;
use crate::install_routes::{install_complete_response, install_status_response};
use crate::openapi::openapi_json_response;
use crate::products::{ListProductsQuery, get_product_response, list_products_response};
use crate::realtime::CartHub;

const HEALTHZ_ROUTE: &str = "healthz";
const LIST_PRODUCTS_ROUTE: &str = "list_products";
const GET_PRODUCT_ROUTE: &str = "get_product";
const CREATE_CART_ROUTE: &str = "create_cart";
const GET_CART_ROUTE: &str = "get_cart";
const ADD_CART_LINE_ROUTE: &str = "add_cart_line";
const UPDATE_CART_LINE_ROUTE: &str = "update_cart_line";
const DELETE_CART_LINE_ROUTE: &str = "delete_cart_line";
const PLACE_ORDER_ROUTE: &str = "place_order";
const OPENAPI_ROUTE: &str = "openapi_json";
const LIST_ADMIN_PRODUCTS_ROUTE: &str = "list_admin_products";
const LIST_ADMIN_ORDERS_ROUTE: &str = "list_admin_orders";
const PATCH_ADMIN_ORDER_ROUTE: &str = "patch_admin_order";
const CREATE_SANDBOX_JOB_ROUTE: &str = "create_sandbox_job";
const GET_SANDBOX_JOB_ROUTE: &str = "get_sandbox_job";
const LIST_SANDBOX_AUDIT_ROUTE: &str = "list_sandbox_audit";
const INSTALL_STATUS_ROUTE: &str = "install_status";
const INSTALL_COMPLETE_ROUTE: &str = "install_complete";
const QUERY_STRING_ATTR: &str = "query_string";

/// Inputs for [`commerce_http_kernel`].
#[derive(Clone, Debug)]
pub struct CommerceFrontConfig {
    /// Catalog store for commerce and admin JSON routes.
    pub catalog: Option<CatalogRepository>,
    /// Admin bearer gate.
    pub admin_auth: AdminAuthConfig,
    /// Operator API path segment only (e.g. `admin`).
    pub admin_prefix: String,
    /// Shop root used for install API disk checks.
    pub install_root: Option<PathBuf>,
    /// Optional cart WebSocket hub for mutation push.
    pub cart_hub: Option<CartHub>,
    /// Optional sandbox job hub for log push.
    pub sandbox_hub: Option<crate::sandbox_realtime::SandboxJobHub>,
    /// Optional in-process sandbox job + audit registry.
    pub sandbox_registry: Option<crate::sandbox_jobs::SandboxJobRegistry>,
}

impl CommerceFrontConfig {
    /// Unit-test defaults: no catalog, empty auth, prefix `admin`, no install root.
    #[must_use]
    pub fn test_default() -> Self {
        Self {
            catalog: None,
            admin_auth: AdminAuthConfig::from_token(""),
            admin_prefix: DEFAULT_ADMIN_API_PREFIX.to_owned(),
            install_root: None,
            cart_hub: None,
            sandbox_hub: None,
            sandbox_registry: None,
        }
    }
}

/// Builds the Serenade async kernel for routes already moved off Actix handlers.
#[must_use]
pub fn commerce_http_kernel(config: CommerceFrontConfig) -> AsyncHttpKernel {
    let routes = front_matcher(&config.admin_prefix);
    AsyncHttpKernel::from_async_fn(move |request: &mut Request| {
        let config = config.clone();
        let outcome = routes.apply(request);
        let query = request
            .attributes()
            .get::<String>(QUERY_STRING_ATTR)
            .cloned();
        let id = request.attributes().get::<String>("id").cloned();
        let line_id = request.attributes().get::<String>("line_id").cloned();
        let body = request.body().to_vec();
        let idempotency = idempotency_key_from_headers(request.headers());
        let bearer = bearer_from_headers(request.headers());
        box_future(async move {
            match outcome {
                Ok(found) => Ok(dispatch_route(
                    found.route_name(),
                    &config,
                    DispatchInput {
                        query: query.as_deref(),
                        id: id.as_deref(),
                        line_id: line_id.as_deref(),
                        body: &body,
                        idempotency: idempotency.as_deref(),
                        bearer: bearer.as_deref(),
                    },
                )
                .await),
                Err(error) => Err(error),
            }
        })
    })
}

struct DispatchInput<'req> {
    query: Option<&'req str>,
    id: Option<&'req str>,
    line_id: Option<&'req str>,
    body: &'req [u8],
    idempotency: Option<&'req str>,
    bearer: Option<&'req str>,
}

async fn dispatch_route(
    route_name: &str,
    config: &CommerceFrontConfig,
    input: DispatchInput<'_>,
) -> Response {
    match route_name {
        HEALTHZ_ROUTE => healthz_response(),
        LIST_PRODUCTS_ROUTE => {
            list_products_via_catalog(config.catalog.as_ref(), input.query).await
        }
        GET_PRODUCT_ROUTE => get_product_via_catalog(config.catalog.as_ref(), input.id).await,
        CREATE_CART_ROUTE => {
            create_cart_via_catalog(
                config.catalog.as_ref(),
                config.cart_hub.as_ref(),
                input.body,
            )
            .await
        }
        GET_CART_ROUTE => get_cart_via_catalog(config.catalog.as_ref(), input.id).await,
        ADD_CART_LINE_ROUTE => {
            add_cart_line_via_catalog(
                config.catalog.as_ref(),
                config.cart_hub.as_ref(),
                input.id,
                input.body,
            )
            .await
        }
        UPDATE_CART_LINE_ROUTE => {
            update_cart_line_via_catalog(
                config.catalog.as_ref(),
                config.cart_hub.as_ref(),
                input.id,
                input.line_id,
                input.body,
            )
            .await
        }
        DELETE_CART_LINE_ROUTE => {
            delete_cart_line_via_catalog(
                config.catalog.as_ref(),
                config.cart_hub.as_ref(),
                input.id,
                input.line_id,
            )
            .await
        }
        PLACE_ORDER_ROUTE => {
            place_order_via_catalog(config.catalog.as_ref(), input.body, input.idempotency).await
        }
        OPENAPI_ROUTE => openapi_json_response(),
        LIST_ADMIN_PRODUCTS_ROUTE => {
            list_admin_products_via_catalog(
                &config.admin_auth,
                input.bearer,
                config.catalog.as_ref(),
                input.query,
            )
            .await
        }
        LIST_ADMIN_ORDERS_ROUTE => {
            list_admin_orders_via_catalog(
                &config.admin_auth,
                input.bearer,
                config.catalog.as_ref(),
                input.query,
            )
            .await
        }
        PATCH_ADMIN_ORDER_ROUTE => {
            patch_admin_order_via_catalog(
                &config.admin_auth,
                input.bearer,
                config.catalog.as_ref(),
                input.id,
                input.body,
            )
            .await
        }
        CREATE_SANDBOX_JOB_ROUTE | GET_SANDBOX_JOB_ROUTE | LIST_SANDBOX_AUDIT_ROUTE => {
            dispatch_sandbox_route(route_name, config, &input)
        }
        INSTALL_STATUS_ROUTE => install_status_response(config.install_root.as_deref()),
        INSTALL_COMPLETE_ROUTE => {
            install_complete_response(config.install_root.as_deref(), input.body)
        }
        _ => Response::new(404).with_body(b"no handler".to_vec()),
    }
}

fn dispatch_sandbox_route(
    route_name: &str,
    config: &CommerceFrontConfig,
    input: &DispatchInput<'_>,
) -> Response {
    match route_name {
        CREATE_SANDBOX_JOB_ROUTE => create_sandbox_job_via_registry(
            &config.admin_auth,
            input.bearer,
            config.sandbox_registry.as_ref(),
            config.sandbox_hub.as_ref(),
            input.body,
        ),
        GET_SANDBOX_JOB_ROUTE => get_sandbox_job_via_registry(
            &config.admin_auth,
            input.bearer,
            config.sandbox_registry.as_ref(),
            input.id,
        ),
        LIST_SANDBOX_AUDIT_ROUTE => list_sandbox_audit_via_registry(
            &config.admin_auth,
            input.bearer,
            config.sandbox_registry.as_ref(),
        ),
        _ => Response::new(404).with_body(b"no handler".to_vec()),
    }
}

async fn list_products_via_catalog(
    catalog: Option<&CatalogRepository>,
    query: Option<&str>,
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    list_products_response(catalog, &ListProductsQuery::from_query_string(query)).await
}

async fn get_product_via_catalog(
    catalog: Option<&CatalogRepository>,
    product_id: Option<&str>,
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(product_id) = product_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    get_product_response(catalog, product_id).await
}

async fn create_cart_via_catalog(
    catalog: Option<&CatalogRepository>,
    hub: Option<&CartHub>,
    body: &[u8],
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    create_cart_response(catalog, hub, body).await
}

async fn get_cart_via_catalog(
    catalog: Option<&CatalogRepository>,
    cart_id: Option<&str>,
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(cart_id) = cart_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    get_cart_response(catalog, cart_id).await
}

async fn add_cart_line_via_catalog(
    catalog: Option<&CatalogRepository>,
    hub: Option<&CartHub>,
    cart_id: Option<&str>,
    body: &[u8],
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(cart_id) = cart_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    add_cart_line_response(catalog, hub, cart_id, body).await
}

async fn update_cart_line_via_catalog(
    catalog: Option<&CatalogRepository>,
    hub: Option<&CartHub>,
    cart_id: Option<&str>,
    line_id: Option<&str>,
    body: &[u8],
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(cart_id) = cart_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    let Some(line_id) = line_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    update_cart_line_response(catalog, hub, cart_id, line_id, body).await
}

async fn delete_cart_line_via_catalog(
    catalog: Option<&CatalogRepository>,
    hub: Option<&CartHub>,
    cart_id: Option<&str>,
    line_id: Option<&str>,
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(cart_id) = cart_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    let Some(line_id) = line_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    delete_cart_line_response(catalog, hub, cart_id, line_id).await
}

async fn place_order_via_catalog(
    catalog: Option<&CatalogRepository>,
    body: &[u8],
    idempotency_key: Option<&str>,
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    place_order_response(catalog, body, idempotency_key).await
}

async fn list_admin_products_via_catalog(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    catalog: Option<&CatalogRepository>,
    query: Option<&str>,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    list_admin_products_response(
        auth,
        bearer,
        catalog,
        &ListAdminProductsQuery::from_query_string(query),
    )
    .await
}

async fn list_admin_orders_via_catalog(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    catalog: Option<&CatalogRepository>,
    query: Option<&str>,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    list_admin_orders_response(
        auth,
        bearer,
        catalog,
        &ListOrdersQuery::from_query_string(query),
    )
    .await
}

async fn patch_admin_order_via_catalog(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    catalog: Option<&CatalogRepository>,
    order_id: Option<&str>,
    body: &[u8],
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(order_id) = order_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    patch_admin_order_response(auth, bearer, catalog, order_id, body).await
}

fn create_sandbox_job_via_registry(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: Option<&crate::sandbox_jobs::SandboxJobRegistry>,
    hub: Option<&crate::sandbox_realtime::SandboxJobHub>,
    body: &[u8],
) -> Response {
    let Some(registry) = registry else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(hub) = hub else {
        return api_error_json_response(&ApiError::Internal);
    };
    crate::sandbox_jobs::create_sandbox_job_response(auth, bearer, registry, hub, body)
}

fn get_sandbox_job_via_registry(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: Option<&crate::sandbox_jobs::SandboxJobRegistry>,
    job_id: Option<&str>,
) -> Response {
    let Some(registry) = registry else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(job_id) = job_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    crate::sandbox_jobs::get_sandbox_job_response(auth, bearer, registry, job_id)
}

fn list_sandbox_audit_via_registry(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: Option<&crate::sandbox_jobs::SandboxJobRegistry>,
) -> Response {
    let Some(registry) = registry else {
        return api_error_json_response(&ApiError::Internal);
    };
    crate::sandbox_jobs::list_sandbox_audit_response(auth, bearer, registry)
}

fn front_matcher(admin_prefix: &str) -> UrlMatcher {
    let mut collection = RouteCollection::new();
    add_storefront_routes(&mut collection);
    add_admin_and_ops_routes(&mut collection, admin_prefix);
    // Unhandled name so the unknown-route arm stays reachable in tests.
    collection
        .add(Route::with_method("orphan", "/__orphan", Method::Get))
        .expect("orphan route");
    UrlMatcher::new(collection)
}

fn add_storefront_routes(collection: &mut RouteCollection) {
    collection
        .add(Route::with_method(HEALTHZ_ROUTE, "/healthz", Method::Get))
        .expect("healthz route");
    collection
        .add(Route::with_method(
            LIST_PRODUCTS_ROUTE,
            "/v1/products",
            Method::Get,
        ))
        .expect("list products route");
    collection
        .add(Route::with_method(
            GET_PRODUCT_ROUTE,
            "/v1/products/{id}",
            Method::Get,
        ))
        .expect("get product route");
    collection
        .add(Route::with_method(
            CREATE_CART_ROUTE,
            "/v1/carts",
            Method::Post,
        ))
        .expect("create cart route");
    collection
        .add(Route::with_method(
            GET_CART_ROUTE,
            "/v1/carts/{id}",
            Method::Get,
        ))
        .expect("get cart route");
    collection
        .add(Route::with_method(
            ADD_CART_LINE_ROUTE,
            "/v1/carts/{id}/lines",
            Method::Post,
        ))
        .expect("add cart line route");
    collection
        .add(Route::with_method(
            UPDATE_CART_LINE_ROUTE,
            "/v1/carts/{id}/lines/{line_id}",
            Method::Patch,
        ))
        .expect("update cart line route");
    collection
        .add(Route::with_method(
            DELETE_CART_LINE_ROUTE,
            "/v1/carts/{id}/lines/{line_id}",
            Method::Delete,
        ))
        .expect("delete cart line route");
    collection
        .add(Route::with_method(
            PLACE_ORDER_ROUTE,
            "/v1/checkout",
            Method::Post,
        ))
        .expect("place order route");
}

fn add_admin_and_ops_routes(collection: &mut RouteCollection, admin_prefix: &str) {
    collection
        .add(Route::with_method(
            OPENAPI_ROUTE,
            "/openapi.json",
            Method::Get,
        ))
        .expect("openapi route");
    let admin_products = format!("/v1/{admin_prefix}/products");
    collection
        .add(Route::with_method(
            LIST_ADMIN_PRODUCTS_ROUTE,
            &admin_products,
            Method::Get,
        ))
        .expect("list admin products route");
    let admin_orders = format!("/v1/{admin_prefix}/orders");
    collection
        .add(Route::with_method(
            LIST_ADMIN_ORDERS_ROUTE,
            &admin_orders,
            Method::Get,
        ))
        .expect("list admin orders route");
    let admin_order = format!("/v1/{admin_prefix}/orders/{{id}}");
    collection
        .add(Route::with_method(
            PATCH_ADMIN_ORDER_ROUTE,
            &admin_order,
            Method::Patch,
        ))
        .expect("patch admin order route");
    let sandbox_jobs = format!("/v1/{admin_prefix}/sandbox/jobs");
    collection
        .add(Route::with_method(
            CREATE_SANDBOX_JOB_ROUTE,
            &sandbox_jobs,
            Method::Post,
        ))
        .expect("create sandbox job route");
    let sandbox_job = format!("/v1/{admin_prefix}/sandbox/jobs/{{id}}");
    collection
        .add(Route::with_method(
            GET_SANDBOX_JOB_ROUTE,
            &sandbox_job,
            Method::Get,
        ))
        .expect("get sandbox job route");
    let sandbox_audit = format!("/v1/{admin_prefix}/sandbox/audit");
    collection
        .add(Route::with_method(
            LIST_SANDBOX_AUDIT_ROUTE,
            &sandbox_audit,
            Method::Get,
        ))
        .expect("list sandbox audit route");
    collection
        .add(Route::with_method(
            INSTALL_STATUS_ROUTE,
            "/install/api/status",
            Method::Get,
        ))
        .expect("install status route");
    collection
        .add(Route::with_method(
            INSTALL_COMPLETE_ROUTE,
            "/install/api/complete",
            Method::Post,
        ))
        .expect("install complete route");
}

fn healthz_response() -> Response {
    Response::new(200)
        .with_header("content-type", "application/json")
        .with_body(health_json_body())
}

/// Actix service that forwards to the Serenade kernel (injects query string for list routes).
#[allow(clippy::future_not_send)]
pub async fn serenade_dispatch(
    request: actix_web::HttpRequest,
    body: actix_web::web::Bytes,
    kernel: actix_web::web::Data<AsyncHttpKernel>,
) -> actix_web::HttpResponse {
    match from_actix(&request, body) {
        Ok(mut serenade) => {
            if let Some(query) = request.uri().query() {
                serenade
                    .attributes_mut()
                    .insert(QUERY_STRING_ATTR, query.to_owned());
            }
            to_actix(&kernel.handle(serenade).await)
        }
        Err(error) => conversion_error(&error),
    }
}

/// Registers Serenade-fronted routes on an Actix config (compose with leftover Actix commerce).
pub fn configure_serenade_front(cfg: &mut actix_web::web::ServiceConfig, admin_prefix: &str) {
    let admin_products = format!("/v1/{admin_prefix}/products");
    let admin_orders = format!("/v1/{admin_prefix}/orders");
    let admin_order = format!("/v1/{admin_prefix}/orders/{{id}}");
    let sandbox_jobs = format!("/v1/{admin_prefix}/sandbox/jobs");
    let sandbox_job = format!("/v1/{admin_prefix}/sandbox/jobs/{{id}}");
    let sandbox_audit = format!("/v1/{admin_prefix}/sandbox/audit");
    cfg.route("/healthz", actix_web::web::get().to(serenade_dispatch))
        .route("/v1/products", actix_web::web::get().to(serenade_dispatch))
        .route(
            "/v1/products/{id}",
            actix_web::web::get().to(serenade_dispatch),
        )
        .route("/v1/carts", actix_web::web::post().to(serenade_dispatch))
        .route(
            "/v1/carts/{id}",
            actix_web::web::get().to(serenade_dispatch),
        )
        .route(
            "/v1/carts/{id}/lines",
            actix_web::web::post().to(serenade_dispatch),
        )
        .route(
            "/v1/carts/{id}/lines/{line_id}",
            actix_web::web::patch().to(serenade_dispatch),
        )
        .route(
            "/v1/carts/{id}/lines/{line_id}",
            actix_web::web::delete().to(serenade_dispatch),
        )
        .route("/v1/checkout", actix_web::web::post().to(serenade_dispatch))
        .route("/openapi.json", actix_web::web::get().to(serenade_dispatch))
        .route(&admin_products, actix_web::web::get().to(serenade_dispatch))
        .route(&admin_orders, actix_web::web::get().to(serenade_dispatch))
        .route(&admin_order, actix_web::web::patch().to(serenade_dispatch))
        .route(&sandbox_jobs, actix_web::web::post().to(serenade_dispatch))
        .route(&sandbox_job, actix_web::web::get().to(serenade_dispatch))
        .route(&sandbox_audit, actix_web::web::get().to(serenade_dispatch))
        .route(
            "/install/api/status",
            actix_web::web::get().to(serenade_dispatch),
        )
        .route(
            "/install/api/complete",
            actix_web::web::post().to(serenade_dispatch),
        )
        .route("/__orphan", actix_web::web::get().to(serenade_dispatch));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test as actix_test;
    use actix_web::{App, web};
    use serenade_http::ROUTE_ATTRIBUTE;

    use crate::health::HealthResponse;

    fn test_kernel() -> web::Data<AsyncHttpKernel> {
        web::Data::new(commerce_http_kernel(CommerceFrontConfig::test_default()))
    }

    #[actix_web::test]
    async fn healthz_via_serenade_kernel() {
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let req = actix_test::TestRequest::get().uri("/healthz").to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: HealthResponse = actix_test::read_body_json(resp).await;
        assert_eq!(body.status, "ok");
        assert_eq!(body.kernel, rustashop::kernel_status());
    }

    #[actix_web::test]
    async fn products_without_catalog_return_internal() {
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let list = actix_test::TestRequest::get()
            .uri("/v1/products")
            .to_request();
        let list_resp = actix_test::call_service(&app, list).await;
        assert_eq!(list_resp.status(), 500);

        let get = actix_test::TestRequest::get()
            .uri("/v1/products/22222222-2222-2222-2222-222222222221")
            .to_request();
        let get_resp = actix_test::call_service(&app, get).await;
        assert_eq!(get_resp.status(), 500);
    }

    #[actix_web::test]
    async fn cart_checkout_without_catalog_return_internal() {
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let create = actix_test::TestRequest::post()
            .uri("/v1/carts")
            .set_json(serde_json::json!({ "currency": "EUR" }))
            .to_request();
        assert_eq!(actix_test::call_service(&app, create).await.status(), 500);

        let get = actix_test::TestRequest::get()
            .uri("/v1/carts/11111111-1111-1111-1111-111111111111")
            .to_request();
        assert_eq!(actix_test::call_service(&app, get).await.status(), 500);

        let checkout = actix_test::TestRequest::post()
            .uri("/v1/checkout")
            .set_json(serde_json::json!({
                "cart_id": "11111111-1111-1111-1111-111111111111"
            }))
            .to_request();
        assert_eq!(actix_test::call_service(&app, checkout).await.status(), 500);
    }

    #[actix_web::test]
    async fn list_products_accepts_query_string() {
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let req = actix_test::TestRequest::get()
            .uri("/v1/products?limit=1&offset=0")
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 500);
    }

    #[actix_web::test]
    async fn openapi_and_admin_without_catalog() {
        let kernel = web::Data::new(commerce_http_kernel(CommerceFrontConfig {
            admin_auth: AdminAuthConfig::from_token("tok"),
            ..CommerceFrontConfig::test_default()
        }));
        let app = actix_test::init_service(
            App::new()
                .app_data(kernel)
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;

        let openapi = actix_test::TestRequest::get()
            .uri("/openapi.json")
            .to_request();
        assert!(
            actix_test::call_service(&app, openapi)
                .await
                .status()
                .is_success()
        );

        let denied = actix_test::TestRequest::get()
            .uri("/v1/admin/products")
            .to_request();
        assert_eq!(actix_test::call_service(&app, denied).await.status(), 401);

        let admin = actix_test::TestRequest::get()
            .uri("/v1/admin/products")
            .insert_header(("Authorization", "Bearer tok"))
            .to_request();
        assert_eq!(actix_test::call_service(&app, admin).await.status(), 500);
    }

    #[actix_web::test]
    async fn install_status_absent_without_root() {
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let req = actix_test::TestRequest::get()
            .uri("/install/api/status")
            .to_request();
        assert_eq!(actix_test::call_service(&app, req).await.status(), 404);
    }

    #[actix_web::test]
    async fn admin_and_install_helpers_cover_edge_arms() {
        let auth = AdminAuthConfig::from_token("tok");
        assert_eq!(
            list_admin_products_via_catalog(&auth, None, None, None)
                .await
                .status(),
            401
        );
        assert_eq!(
            list_admin_products_via_catalog(&auth, Some("tok"), None, None)
                .await
                .status(),
            500
        );
        assert_eq!(
            list_admin_orders_via_catalog(&auth, Some("tok"), None, Some("limit=1"))
                .await
                .status(),
            500
        );
        assert_eq!(
            patch_admin_order_via_catalog(&auth, Some("tok"), None, Some("id"), b"{}")
                .await
                .status(),
            500
        );
        assert_eq!(
            patch_admin_order_via_catalog(&auth, None, None, None, b"{}")
                .await
                .status(),
            401
        );

        let complete = actix_test::TestRequest::post()
            .uri("/install/api/complete")
            .set_json(serde_json::json!({ "wipe_confirmed": true }))
            .to_request();
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        assert_eq!(actix_test::call_service(&app, complete).await.status(), 404);
    }

    #[cfg(feature = "persist-sqlx")]
    #[actix_web::test]
    async fn patch_admin_requires_id_when_catalog_present() {
        use rustashop_persist_sqlx::SqlxCatalogRepository;
        use sqlx::postgres::PgPoolOptions;

        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skip: DATABASE_URL is not set");
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        let catalog = SqlxCatalogRepository::new(pool);
        let auth = AdminAuthConfig::from_token("tok");
        assert_eq!(
            patch_admin_order_via_catalog(&auth, Some("tok"), Some(&catalog), None, b"{}")
                .await
                .status(),
            404
        );
    }

    #[actix_web::test]
    async fn orphan_route_maps_to_not_found() {
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let req = actix_test::TestRequest::get().uri("/__orphan").to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn dispatch_maps_unsupported_method() {
        let kernel = test_kernel();
        let request = actix_test::TestRequest::default()
            .method(actix_web::http::Method::TRACE)
            .uri("/")
            .to_http_request();
        let response = serenade_dispatch(request, web::Bytes::new(), kernel).await;
        assert_eq!(response.status(), 405);
    }

    #[actix_web::test]
    async fn get_product_via_catalog_requires_id() {
        let response = get_product_via_catalog(None, None).await;
        assert_eq!(response.status(), 500);
        let response = get_product_via_catalog(None, Some("x")).await;
        assert_eq!(response.status(), 500);
    }

    #[actix_web::test]
    async fn cart_helpers_require_path_params() {
        assert_eq!(get_cart_via_catalog(None, None).await.status(), 500);
        assert_eq!(
            add_cart_line_via_catalog(None, None, None, b"{}")
                .await
                .status(),
            500
        );
        assert_eq!(
            update_cart_line_via_catalog(None, None, None, None, b"{}")
                .await
                .status(),
            500
        );
        assert_eq!(
            delete_cart_line_via_catalog(None, None, None, None)
                .await
                .status(),
            500
        );
    }

    #[cfg(feature = "persist-sqlx")]
    #[actix_web::test]
    async fn cart_helpers_missing_path_with_catalog() {
        use rustashop_persist_sqlx::SqlxCatalogRepository;
        use sqlx::postgres::PgPoolOptions;

        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skip: DATABASE_URL is not set");
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        let catalog = SqlxCatalogRepository::new(pool);
        assert_eq!(
            get_cart_via_catalog(Some(&catalog), None).await.status(),
            404
        );
        assert_eq!(
            add_cart_line_via_catalog(Some(&catalog), None, None, b"{}")
                .await
                .status(),
            404
        );
        assert_eq!(
            update_cart_line_via_catalog(Some(&catalog), None, None, Some("l"), b"{}")
                .await
                .status(),
            404
        );
        assert_eq!(
            update_cart_line_via_catalog(Some(&catalog), None, Some("c"), None, b"{}")
                .await
                .status(),
            404
        );
        assert_eq!(
            delete_cart_line_via_catalog(Some(&catalog), None, None, Some("l"))
                .await
                .status(),
            404
        );
        assert_eq!(
            delete_cart_line_via_catalog(Some(&catalog), None, Some("c"), None)
                .await
                .status(),
            404
        );
    }

    #[cfg(feature = "persist-sqlx")]
    #[actix_web::test]
    async fn get_product_via_catalog_missing_id_with_catalog() {
        use rustashop_persist_sqlx::SqlxCatalogRepository;
        use sqlx::postgres::PgPoolOptions;

        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skip: DATABASE_URL is not set");
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        let catalog = SqlxCatalogRepository::new(pool);
        let response = get_product_via_catalog(Some(&catalog), None).await;
        assert_eq!(response.status(), 404);
    }

    #[actix_web::test]
    async fn kernel_rejects_unknown_path() {
        let kernel = commerce_http_kernel(CommerceFrontConfig::test_default());
        let response = kernel.handle(Request::new(Method::Get, "/nope")).await;
        assert_eq!(response.status(), 404);
    }

    #[test]
    fn matcher_sets_route_attribute() {
        let matcher = front_matcher(DEFAULT_ADMIN_API_PREFIX);
        let mut request = Request::new(Method::Get, "/healthz");
        let found = matcher.apply(&mut request).expect("match");
        assert_eq!(found.route_name(), HEALTHZ_ROUTE);
        assert_eq!(
            request
                .attributes()
                .get::<String>(ROUTE_ATTRIBUTE)
                .map(String::as_str),
            Some(HEALTHZ_ROUTE)
        );
    }

    #[test]
    fn dispatch_sandbox_unknown_route_is_404() {
        let config = CommerceFrontConfig::test_default();
        let input = DispatchInput {
            query: None,
            id: None,
            line_id: None,
            body: &[],
            idempotency: None,
            bearer: None,
        };
        let response = dispatch_sandbox_route("not_sandbox", &config, &input);
        assert_eq!(response.status(), 404);
    }

    #[test]
    fn get_sandbox_job_via_registry_requires_job_id() {
        let auth = AdminAuthConfig::from_token("tok");
        let registry = crate::sandbox_jobs::SandboxJobRegistry::new();
        let response = get_sandbox_job_via_registry(&auth, Some("tok"), Some(&registry), None);
        assert_eq!(response.status(), 404);
    }
}
