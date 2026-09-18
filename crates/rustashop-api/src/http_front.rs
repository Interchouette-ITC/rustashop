//! Serenade HTTP front controller for commerce JSON and cart WebSocket routes.

use std::path::PathBuf;

use rustashop_persist::CatalogRepository;
use serenade_http::{
    AsyncHttpKernel, AsyncRequestIdMiddleware, Method, Readiness, Request, Response, Route,
    RouteCollection, UrlMatcher, box_future, readyz,
};
use serenade_http_actix::{conversion_error, from_actix, to_actix};

use crate::admin_auth::{AdminAuthConfig, bearer_from_headers};
use crate::admin_orders::{
    ListOrdersQuery, list_admin_orders_response, patch_admin_order_response,
};
use crate::admin_prefix::DEFAULT_ADMIN_API_PREFIX;
use crate::admin_products::{
    ListAdminProductsQuery, list_admin_products_response, patch_admin_product_response,
};
use crate::ai_tools::{list_ai_tools_response, list_shop_ai_tools_response};
use crate::async_firewall::AsyncFirewallMiddleware;
use crate::carts::{
    add_cart_line_response, create_cart_response, delete_cart_line_response, get_cart_response,
    update_cart_line_response,
};
use crate::catalog_cache::CatalogCache;
use crate::checkout::{idempotency_key_from_headers, place_order_response};
use crate::error::{ApiError, api_error_json_response};
use crate::health::health_json_body;
use crate::install_routes::{install_complete_response, install_status_response};
use crate::model_providers::{
    list_ai_providers_catalog_response, list_ai_providers_response, test_ai_provider_response,
};
use crate::openapi::openapi_json_response;
use crate::order_mail::OrderMailer;
use crate::products::{ListProductsQuery, get_product_response, list_products_response};
use crate::public_rate_limit::{PublicWriteRateLimiter, client_key_from_headers};
use crate::realtime::{CartHub, OrderHub};
use crate::session_http::{csrf_manager_from_env, try_browser_security_route};
use serenade_security::AsyncSessionTokenMiddleware;
use serenade_session::{AsyncSessionMiddleware, CookieSession, MemorySessionStore, SessionStore};
use std::sync::Arc;

const HEALTHZ_ROUTE: &str = "healthz";
const READYZ_ROUTE: &str = "readyz";
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
const PATCH_ADMIN_PRODUCT_ROUTE: &str = "patch_admin_product";
const LIST_ADMIN_ORDERS_ROUTE: &str = "list_admin_orders";
const PATCH_ADMIN_ORDER_ROUTE: &str = "patch_admin_order";
const CREATE_SANDBOX_JOB_ROUTE: &str = "create_sandbox_job";
const GET_SANDBOX_JOB_ROUTE: &str = "get_sandbox_job";
const LIST_SANDBOX_AUDIT_ROUTE: &str = "list_sandbox_audit";
const COMMIT_SANDBOX_JOB_ROUTE: &str = "commit_sandbox_job";
const DISCARD_SANDBOX_JOB_ROUTE: &str = "discard_sandbox_job";
const LIST_AI_TOOLS_ROUTE: &str = "list_ai_tools";
const LIST_SHOP_AI_TOOLS_ROUTE: &str = "list_shop_ai_tools";
const LIST_AI_PROVIDERS_ROUTE: &str = "list_ai_providers";
const LIST_AI_PROVIDERS_CATALOG_ROUTE: &str = "list_ai_providers_catalog";
const TEST_AI_PROVIDER_ROUTE: &str = "test_ai_provider";
const INSTALL_STATUS_ROUTE: &str = "install_status";
const INSTALL_COMPLETE_ROUTE: &str = "install_complete";
const INSTALL_FORM_GET_ROUTE: &str = "install_form_get";
const INSTALL_FORM_POST_ROUTE: &str = "install_form_post";
const SESSION_STATUS_ROUTE: &str = "session_status";
const SESSION_LOGIN_GET_ROUTE: &str = "session_login_get";
const SESSION_LOGIN_POST_ROUTE: &str = "session_login_post";
const SESSION_LOGOUT_GET_ROUTE: &str = "session_logout_get";
const SESSION_LOGOUT_POST_ROUTE: &str = "session_logout_post";
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
    /// Optional order WebSocket hub for status push.
    pub order_hub: Option<OrderHub>,
    /// Optional sandbox job hub for log push.
    pub sandbox_hub: Option<crate::sandbox_realtime::SandboxJobHub>,
    /// Optional in-process sandbox job + audit registry.
    pub sandbox_registry: Option<crate::sandbox_jobs::SandboxJobRegistry>,
    /// Optional Serenade messenger for sandbox job enqueue.
    pub sandbox_messenger: Option<crate::sandbox_messenger::SandboxJobMessenger>,
    /// Optional storefront catalog cache (`serenade-cache`).
    pub catalog_cache: Option<CatalogCache>,
    /// Optional rate limiter for public cart/checkout writes.
    pub public_rate_limiter: Option<PublicWriteRateLimiter>,
    /// Optional order confirmation mailer (`serenade-mailer`).
    pub order_mailer: Option<OrderMailer>,
    /// Shared readiness flag for `GET /readyz` (flip before HTTP drain).
    pub readiness: Readiness,
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
            order_hub: None,
            sandbox_hub: None,
            sandbox_registry: None,
            sandbox_messenger: None,
            catalog_cache: Some(CatalogCache::with_ttl(None)),
            public_rate_limiter: Some(PublicWriteRateLimiter::with_policy(
                10_000,
                std::time::Duration::from_secs(60),
            )),
            order_mailer: Some(OrderMailer::null()),
            readiness: Readiness::new(),
        }
    }
}

/// Builds the Serenade async kernel for routes already moved off Actix handlers.
#[must_use]
pub fn commerce_http_kernel(config: CommerceFrontConfig) -> AsyncHttpKernel {
    let routes = front_matcher(&config.admin_prefix);
    let authenticator = config.admin_auth.authenticator();
    let csrf = csrf_manager_from_env();
    let session_cookies =
        CookieSession::new(Arc::new(MemorySessionStore::new()) as Arc<dyn SessionStore>);
    let mut kernel = AsyncHttpKernel::from_async_fn(move |request: &mut Request| {
        let config = config.clone();
        let csrf = Arc::clone(&csrf);
        let outcome = routes.apply(request);
        if let Ok(found) = &outcome
            && let Some(response) = try_browser_security_route(
                found.route_name(),
                request,
                &config.admin_auth,
                csrf.as_ref(),
                config.install_root.as_deref(),
            )
        {
            return box_future(async move { Ok(response) });
        }
        let query = request
            .attributes()
            .get::<String>(QUERY_STRING_ATTR)
            .cloned();
        let id = request.attributes().get::<String>("id").cloned();
        let line_id = request.attributes().get::<String>("line_id").cloned();
        let body = request.body().to_vec();
        let idempotency = idempotency_key_from_headers(request.headers());
        let bearer = bearer_from_headers(request.headers());
        let client_key = client_key_from_headers(request.headers());
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
                        client_key: client_key.as_str(),
                    },
                )
                .await),
                Err(error) => Err(error),
            }
        })
    });
    // First pushed = outermost.
    kernel.push_middleware(AsyncRequestIdMiddleware);
    kernel.push_middleware(AsyncSessionMiddleware::new(session_cookies));
    kernel.push_middleware(AsyncSessionTokenMiddleware::new());
    kernel.push_middleware(
        AsyncFirewallMiddleware::new("Authorization", authenticator).allow_anonymous(true),
    );
    kernel
}

struct DispatchInput<'req> {
    query: Option<&'req str>,
    id: Option<&'req str>,
    line_id: Option<&'req str>,
    body: &'req [u8],
    idempotency: Option<&'req str>,
    bearer: Option<&'req str>,
    client_key: &'req str,
}

fn is_public_mutating_route(route_name: &str) -> bool {
    matches!(
        route_name,
        CREATE_CART_ROUTE
            | ADD_CART_LINE_ROUTE
            | UPDATE_CART_LINE_ROUTE
            | DELETE_CART_LINE_ROUTE
            | PLACE_ORDER_ROUTE
    )
}

async fn dispatch_route(
    route_name: &str,
    config: &CommerceFrontConfig,
    input: DispatchInput<'_>,
) -> Response {
    if is_public_mutating_route(route_name)
        && let Some(limiter) = config.public_rate_limiter.as_ref()
        && let Err(response) = limiter.check(input.client_key)
    {
        return response;
    }
    match route_name {
        HEALTHZ_ROUTE => healthz_response(),
        READYZ_ROUTE => readyz(config.readiness.is_ready()),
        LIST_PRODUCTS_ROUTE => {
            list_products_via_catalog(
                config.catalog.as_ref(),
                config.catalog_cache.as_ref(),
                input.query,
            )
            .await
        }
        GET_PRODUCT_ROUTE => {
            get_product_via_catalog(
                config.catalog.as_ref(),
                config.catalog_cache.as_ref(),
                input.id,
            )
            .await
        }
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
            place_order_via_catalog(
                config.catalog.as_ref(),
                config.order_mailer.as_ref(),
                input.body,
                input.idempotency,
            )
            .await
        }
        OPENAPI_ROUTE => openapi_json_response(),
        _ => dispatch_operator_route(route_name, config, &input).await,
    }
}

async fn dispatch_operator_route(
    route_name: &str,
    config: &CommerceFrontConfig,
    input: &DispatchInput<'_>,
) -> Response {
    match route_name {
        LIST_ADMIN_PRODUCTS_ROUTE => {
            list_admin_products_via_catalog(
                &config.admin_auth,
                input.bearer,
                config.catalog.as_ref(),
                input.query,
            )
            .await
        }
        PATCH_ADMIN_PRODUCT_ROUTE => {
            patch_admin_product_via_catalog(
                &config.admin_auth,
                input.bearer,
                config.catalog.as_ref(),
                config.catalog_cache.as_ref(),
                input.id,
                input.body,
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
                config.order_hub.as_ref(),
            )
            .await
        }
        CREATE_SANDBOX_JOB_ROUTE
        | GET_SANDBOX_JOB_ROUTE
        | LIST_SANDBOX_AUDIT_ROUTE
        | COMMIT_SANDBOX_JOB_ROUTE
        | DISCARD_SANDBOX_JOB_ROUTE => dispatch_sandbox_route(route_name, config, input).await,
        LIST_AI_TOOLS_ROUTE => list_ai_tools_response(&config.admin_auth, input.bearer),
        LIST_SHOP_AI_TOOLS_ROUTE => list_shop_ai_tools_response(),
        LIST_AI_PROVIDERS_ROUTE => list_ai_providers_response(&config.admin_auth, input.bearer),
        LIST_AI_PROVIDERS_CATALOG_ROUTE => {
            list_ai_providers_catalog_response(&config.admin_auth, input.bearer)
        }
        TEST_AI_PROVIDER_ROUTE => {
            test_ai_provider_response(&config.admin_auth, input.bearer, input.body)
        }
        INSTALL_STATUS_ROUTE => install_status_response(config.install_root.as_deref()),
        INSTALL_COMPLETE_ROUTE => {
            install_complete_response(config.install_root.as_deref(), input.body)
        }
        _ => Response::new(404).with_body(b"no handler".to_vec()),
    }
}

async fn dispatch_sandbox_route(
    route_name: &str,
    config: &CommerceFrontConfig,
    input: &DispatchInput<'_>,
) -> Response {
    match route_name {
        CREATE_SANDBOX_JOB_ROUTE => create_sandbox_job_from_front(config, input).await,
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
        COMMIT_SANDBOX_JOB_ROUTE => {
            commit_sandbox_job_via_registry(
                &config.admin_auth,
                input.bearer,
                config.sandbox_registry.as_ref(),
                config.sandbox_hub.as_ref(),
                config.cart_hub.as_ref(),
                config.catalog.as_ref(),
                input.id,
            )
            .await
        }
        DISCARD_SANDBOX_JOB_ROUTE => discard_sandbox_job_via_registry(
            &config.admin_auth,
            input.bearer,
            config.sandbox_registry.as_ref(),
            config.sandbox_hub.as_ref(),
            input.id,
        ),
        _ => Response::new(404).with_body(b"no handler".to_vec()),
    }
}

#[rustfmt::skip]
async fn create_sandbox_job_from_front(
    config: &CommerceFrontConfig,
    input: &DispatchInput<'_>,
) -> Response {
    create_sandbox_job_via_registry(&config.admin_auth, input.bearer, config.sandbox_registry.as_ref(), config.sandbox_hub.as_ref(), config.sandbox_messenger.as_ref(), input.body).await
}

async fn list_products_via_catalog(
    catalog: Option<&CatalogRepository>,
    cache: Option<&CatalogCache>,
    query: Option<&str>,
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    list_products_response(catalog, &ListProductsQuery::from_query_string(query), cache).await
}

async fn get_product_via_catalog(
    catalog: Option<&CatalogRepository>,
    cache: Option<&CatalogCache>,
    product_id: Option<&str>,
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(product_id) = product_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    get_product_response(catalog, product_id, cache).await
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
    mailer: Option<&OrderMailer>,
    body: &[u8],
    idempotency_key: Option<&str>,
) -> Response {
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    place_order_response(catalog, mailer, body, idempotency_key).await
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

async fn patch_admin_product_via_catalog(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    catalog: Option<&CatalogRepository>,
    cache: Option<&CatalogCache>,
    product_id: Option<&str>,
    body: &[u8],
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(product_id) = product_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    patch_admin_product_response(auth, bearer, catalog, product_id, body, cache).await
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
    hub: Option<&OrderHub>,
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
    patch_admin_order_response(auth, bearer, catalog, order_id, body, hub).await
}

async fn create_sandbox_job_via_registry(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: Option<&crate::sandbox_jobs::SandboxJobRegistry>,
    hub: Option<&crate::sandbox_realtime::SandboxJobHub>,
    messenger: Option<&crate::sandbox_messenger::SandboxJobMessenger>,
    body: &[u8],
) -> Response {
    let Some(registry) = registry else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(hub) = hub else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(messenger) = messenger else {
        return api_error_json_response(&ApiError::Internal);
    };
    crate::sandbox_jobs::create_sandbox_job_response(auth, bearer, registry, hub, messenger, body)
        .await
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

async fn commit_sandbox_job_via_registry(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: Option<&crate::sandbox_jobs::SandboxJobRegistry>,
    hub: Option<&crate::sandbox_realtime::SandboxJobHub>,
    cart_hub: Option<&CartHub>,
    catalog: Option<&CatalogRepository>,
    job_id: Option<&str>,
) -> Response {
    let Some(registry) = registry else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(hub) = hub else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(catalog) = catalog else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(job_id) = job_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    crate::sandbox_autonomous::commit_sandbox_job_response(
        crate::sandbox_autonomous::CommitSandboxJobContext {
            auth,
            bearer,
            registry,
            hub,
            cart_hub,
            catalog,
            job_id,
        },
    )
    .await
}

fn discard_sandbox_job_via_registry(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: Option<&crate::sandbox_jobs::SandboxJobRegistry>,
    hub: Option<&crate::sandbox_realtime::SandboxJobHub>,
    job_id: Option<&str>,
) -> Response {
    let Some(registry) = registry else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(hub) = hub else {
        return api_error_json_response(&ApiError::Internal);
    };
    let Some(job_id) = job_id else {
        return api_error_json_response(&ApiError::NotFound);
    };
    crate::sandbox_autonomous::discard_sandbox_job_response(auth, bearer, registry, hub, job_id)
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
        .add(Route::with_method(READYZ_ROUTE, "/readyz", Method::Get))
        .expect("readyz route");
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
    collection
        .add(Route::with_method(
            LIST_SHOP_AI_TOOLS_ROUTE,
            "/v1/ai/tools",
            Method::Get,
        ))
        .expect("list shop ai tools route");
}

fn add_admin_and_ops_routes(collection: &mut RouteCollection, admin_prefix: &str) {
    collection
        .add(Route::with_method(
            OPENAPI_ROUTE,
            "/openapi.json",
            Method::Get,
        ))
        .expect("openapi route");
    add_admin_catalog_routes(collection, admin_prefix);
    add_sandbox_admin_routes(collection, admin_prefix);
    add_ai_admin_routes(collection, admin_prefix);
    add_install_and_session_routes(collection);
}

fn add_admin_catalog_routes(collection: &mut RouteCollection, admin_prefix: &str) {
    let admin_products = format!("/v1/{admin_prefix}/products");
    collection
        .add(Route::with_method(
            LIST_ADMIN_PRODUCTS_ROUTE,
            &admin_products,
            Method::Get,
        ))
        .expect("list admin products route");
    let admin_product = format!("/v1/{admin_prefix}/products/{{id}}");
    collection
        .add(Route::with_method(
            PATCH_ADMIN_PRODUCT_ROUTE,
            &admin_product,
            Method::Patch,
        ))
        .expect("patch admin product route");
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
}

fn add_install_and_session_routes(collection: &mut RouteCollection) {
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
    collection
        .add(Route::with_method(
            INSTALL_FORM_GET_ROUTE,
            "/install/form",
            Method::Get,
        ))
        .expect("install form get route");
    collection
        .add(Route::with_method(
            INSTALL_FORM_POST_ROUTE,
            "/install/form",
            Method::Post,
        ))
        .expect("install form post route");
    collection
        .add(Route::with_method(
            SESSION_STATUS_ROUTE,
            "/session",
            Method::Get,
        ))
        .expect("session status route");
    collection
        .add(Route::with_method(
            SESSION_LOGIN_GET_ROUTE,
            "/session/login",
            Method::Get,
        ))
        .expect("session login get route");
    collection
        .add(Route::with_method(
            SESSION_LOGIN_POST_ROUTE,
            "/session/login",
            Method::Post,
        ))
        .expect("session login post route");
    collection
        .add(Route::with_method(
            SESSION_LOGOUT_GET_ROUTE,
            "/session/logout",
            Method::Get,
        ))
        .expect("session logout get route");
    collection
        .add(Route::with_method(
            SESSION_LOGOUT_POST_ROUTE,
            "/session/logout",
            Method::Post,
        ))
        .expect("session logout post route");
}

fn add_sandbox_admin_routes(collection: &mut RouteCollection, admin_prefix: &str) {
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
    let sandbox_commit = format!("/v1/{admin_prefix}/sandbox/jobs/{{id}}/commit");
    collection
        .add(Route::with_method(
            COMMIT_SANDBOX_JOB_ROUTE,
            &sandbox_commit,
            Method::Post,
        ))
        .expect("commit sandbox job route");
    let sandbox_discard = format!("/v1/{admin_prefix}/sandbox/jobs/{{id}}/discard");
    collection
        .add(Route::with_method(
            DISCARD_SANDBOX_JOB_ROUTE,
            &sandbox_discard,
            Method::Post,
        ))
        .expect("discard sandbox job route");
    let sandbox_audit = format!("/v1/{admin_prefix}/sandbox/audit");
    collection
        .add(Route::with_method(
            LIST_SANDBOX_AUDIT_ROUTE,
            &sandbox_audit,
            Method::Get,
        ))
        .expect("list sandbox audit route");
}

fn add_ai_admin_routes(collection: &mut RouteCollection, admin_prefix: &str) {
    let ai_tools = format!("/v1/{admin_prefix}/ai/tools");
    collection
        .add(Route::with_method(
            LIST_AI_TOOLS_ROUTE,
            &ai_tools,
            Method::Get,
        ))
        .expect("list ai tools route");
    let ai_providers = format!("/v1/{admin_prefix}/ai/providers");
    collection
        .add(Route::with_method(
            LIST_AI_PROVIDERS_ROUTE,
            &ai_providers,
            Method::Get,
        ))
        .expect("list ai providers route");
    let ai_providers_catalog = format!("/v1/{admin_prefix}/ai/providers/catalog");
    collection
        .add(Route::with_method(
            LIST_AI_PROVIDERS_CATALOG_ROUTE,
            &ai_providers_catalog,
            Method::Get,
        ))
        .expect("list ai providers catalog route");
    let ai_providers_test = format!("/v1/{admin_prefix}/ai/providers/test");
    collection
        .add(Route::with_method(
            TEST_AI_PROVIDER_ROUTE,
            &ai_providers_test,
            Method::Post,
        ))
        .expect("test ai provider route");
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
    let sandbox_commit = format!("/v1/{admin_prefix}/sandbox/jobs/{{id}}/commit");
    let sandbox_discard = format!("/v1/{admin_prefix}/sandbox/jobs/{{id}}/discard");
    let sandbox_audit = format!("/v1/{admin_prefix}/sandbox/audit");
    let ai_tools = format!("/v1/{admin_prefix}/ai/tools");
    let ai_providers = format!("/v1/{admin_prefix}/ai/providers");
    let ai_providers_catalog = format!("/v1/{admin_prefix}/ai/providers/catalog");
    let ai_providers_test = format!("/v1/{admin_prefix}/ai/providers/test");
    cfg.route("/healthz", actix_web::web::get().to(serenade_dispatch))
        .route("/readyz", actix_web::web::get().to(serenade_dispatch))
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
        .route("/v1/ai/tools", actix_web::web::get().to(serenade_dispatch))
        .route("/openapi.json", actix_web::web::get().to(serenade_dispatch))
        .route(&admin_products, actix_web::web::get().to(serenade_dispatch))
        .route(&admin_orders, actix_web::web::get().to(serenade_dispatch))
        .route(&admin_order, actix_web::web::patch().to(serenade_dispatch))
        .route(&sandbox_jobs, actix_web::web::post().to(serenade_dispatch))
        .route(&sandbox_job, actix_web::web::get().to(serenade_dispatch))
        .route(
            &sandbox_commit,
            actix_web::web::post().to(serenade_dispatch),
        )
        .route(
            &sandbox_discard,
            actix_web::web::post().to(serenade_dispatch),
        )
        .route(&sandbox_audit, actix_web::web::get().to(serenade_dispatch))
        .route(&ai_tools, actix_web::web::get().to(serenade_dispatch))
        .route(&ai_providers, actix_web::web::get().to(serenade_dispatch))
        .route(
            &ai_providers_catalog,
            actix_web::web::get().to(serenade_dispatch),
        )
        .route(
            &ai_providers_test,
            actix_web::web::post().to(serenade_dispatch),
        )
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
    async fn readyz_via_serenade_kernel() {
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let req = actix_test::TestRequest::get().uri("/readyz").to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body = actix_test::read_body(resp).await;
        assert_eq!(body.as_ref(), b"ready");
    }

    #[actix_web::test]
    async fn readyz_not_ready_returns_503() {
        let config = CommerceFrontConfig::test_default();
        config.readiness.mark_not_ready();
        let app = actix_test::init_service(
            App::new()
                .app_data(web::Data::new(commerce_http_kernel(config)))
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let req = actix_test::TestRequest::get().uri("/readyz").to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 503);
        let body = actix_test::read_body(resp).await;
        assert_eq!(body.as_ref(), b"not ready");
    }

    #[actix_web::test]
    async fn request_id_echoed_on_healthz() {
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let req = actix_test::TestRequest::get()
            .uri("/healthz")
            .insert_header(("x-request-id", "dogfood-w1"))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let echoed = resp
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok());
        assert_eq!(echoed, Some("dogfood-w1"));
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
    async fn shop_ai_tools_public() {
        let app = actix_test::init_service(
            App::new()
                .app_data(test_kernel())
                .configure(|cfg| configure_serenade_front(cfg, DEFAULT_ADMIN_API_PREFIX)),
        )
        .await;
        let req = actix_test::TestRequest::get()
            .uri("/v1/ai/tools")
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body: Vec<crate::ai_tools::AiToolResponse> = actix_test::read_body_json(resp).await;
        assert!(!body.is_empty());
        assert!(body.iter().all(|tool| tool.scope == "shop"));
        assert!(body.iter().any(|tool| tool.name == "list_products"));
    }

    #[actix_web::test]
    async fn ai_providers_admin_routes() {
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

        let denied = actix_test::TestRequest::get()
            .uri("/v1/admin/ai/providers")
            .to_request();
        assert_eq!(actix_test::call_service(&app, denied).await.status(), 401);

        let status = actix_test::TestRequest::get()
            .uri("/v1/admin/ai/providers")
            .insert_header(("Authorization", "Bearer tok"))
            .to_request();
        assert_eq!(actix_test::call_service(&app, status).await.status(), 200);

        let catalog = actix_test::TestRequest::get()
            .uri("/v1/admin/ai/providers/catalog")
            .insert_header(("Authorization", "Bearer tok"))
            .to_request();
        assert_eq!(actix_test::call_service(&app, catalog).await.status(), 200);

        let probe = actix_test::TestRequest::post()
            .uri("/v1/admin/ai/providers/test")
            .insert_header(("Authorization", "Bearer tok"))
            .set_json(serde_json::json!({ "provider_id": "local" }))
            .to_request();
        assert_eq!(actix_test::call_service(&app, probe).await.status(), 200);
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
            patch_admin_order_via_catalog(&auth, Some("tok"), None, Some("id"), b"{}", None)
                .await
                .status(),
            500
        );
        assert_eq!(
            patch_admin_order_via_catalog(&auth, None, None, None, b"{}", None)
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
            patch_admin_order_via_catalog(&auth, Some("tok"), Some(&catalog), None, b"{}", None)
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
        let response = get_product_via_catalog(None, None, None).await;
        assert_eq!(response.status(), 500);
        let response = get_product_via_catalog(None, None, Some("x")).await;
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
        let response = get_product_via_catalog(Some(&catalog), None, None).await;
        assert_eq!(response.status(), 404);
    }

    #[cfg(feature = "persist-sqlx")]
    #[actix_web::test]
    async fn patch_admin_product_via_catalog_with_catalog() {
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
        let auth = AdminAuthConfig::from_token("secret");
        assert_eq!(
            patch_admin_product_via_catalog(
                &auth,
                Some("secret"),
                Some(&catalog),
                None,
                None,
                br#"{"enabled":false}"#,
            )
            .await
            .status(),
            404
        );
        // Unknown id still enters the catalog+id path (covers the success dispatch line).
        let status = patch_admin_product_via_catalog(
            &auth,
            Some("secret"),
            Some(&catalog),
            Some(&CatalogCache::with_ttl(None)),
            Some("22222222-2222-2222-2222-222222222299"),
            br#"{"enabled":false}"#,
        )
        .await
        .status();
        assert!(
            status == 404 || status == 500,
            "expected not-found or persist error, got {status}"
        );
    }

    #[actix_web::test]
    async fn kernel_rejects_unknown_path() {
        let kernel = commerce_http_kernel(CommerceFrontConfig::test_default());
        let response = kernel.handle(Request::new(Method::Get, "/nope")).await;
        assert_eq!(response.status(), 404);
    }

    #[actix_web::test]
    async fn public_write_rate_limit_returns_429() {
        let kernel = commerce_http_kernel(CommerceFrontConfig {
            public_rate_limiter: Some(PublicWriteRateLimiter::with_policy(
                1,
                std::time::Duration::from_secs(60),
            )),
            ..CommerceFrontConfig::test_default()
        });
        let first = Request::new(Method::Post, "/v1/carts")
            .with_header("x-client-id", "rate-limit-test")
            .with_header("content-type", "application/json")
            .with_body(br#"{"currency":"EUR"}"#.to_vec());
        let second = Request::new(Method::Post, "/v1/carts")
            .with_header("x-client-id", "rate-limit-test")
            .with_header("content-type", "application/json")
            .with_body(br#"{"currency":"EUR"}"#.to_vec());
        // First may be 500 (no catalog) or 201; second must be 429 when limited.
        let _ = kernel.handle(first).await;
        let limited = kernel.handle(second).await;
        assert_eq!(limited.status(), 429);
    }

    #[actix_web::test]
    async fn patch_admin_product_via_kernel() {
        let auth = AdminAuthConfig::from_token("secret");
        let kernel = commerce_http_kernel(CommerceFrontConfig {
            admin_auth: auth.clone(),
            catalog_cache: Some(CatalogCache::with_ttl(None)),
            ..CommerceFrontConfig::test_default()
        });
        let denied = Request::new(
            Method::Patch,
            "/v1/admin/products/22222222-2222-2222-2222-222222222221",
        )
        .with_header("content-type", "application/json")
        .with_body(br#"{"enabled":false}"#.to_vec());
        assert_eq!(kernel.handle(denied).await.status(), 401);

        let missing_catalog = Request::new(
            Method::Patch,
            "/v1/admin/products/22222222-2222-2222-2222-222222222221",
        )
        .with_header("authorization", "Bearer secret")
        .with_header("content-type", "application/json")
        .with_body(br#"{"enabled":false}"#.to_vec());
        assert_eq!(kernel.handle(missing_catalog).await.status(), 500);

        assert_eq!(
            patch_admin_product_via_catalog(
                &auth,
                Some("secret"),
                None,
                None,
                Some("22222222-2222-2222-2222-222222222221"),
                br#"{"enabled":false}"#,
            )
            .await
            .status(),
            500
        );
        assert_eq!(
            patch_admin_product_via_catalog(
                &auth,
                Some("secret"),
                None,
                None,
                None,
                br#"{"enabled":false}"#,
            )
            .await
            .status(),
            500
        );
    }

    #[actix_web::test]
    async fn rate_limit_keys_off_forwarded_for() {
        let kernel = commerce_http_kernel(CommerceFrontConfig {
            public_rate_limiter: Some(PublicWriteRateLimiter::with_policy(
                1,
                std::time::Duration::from_secs(60),
            )),
            ..CommerceFrontConfig::test_default()
        });
        let first = Request::new(Method::Post, "/v1/carts")
            .with_header("x-forwarded-for", "203.0.113.9, 10.0.0.1")
            .with_header("content-type", "application/json")
            .with_body(br#"{"currency":"EUR"}"#.to_vec());
        let second = Request::new(Method::Post, "/v1/carts")
            .with_header("x-forwarded-for", "203.0.113.9")
            .with_header("content-type", "application/json")
            .with_body(br#"{"currency":"EUR"}"#.to_vec());
        let _ = kernel.handle(first).await;
        assert_eq!(kernel.handle(second).await.status(), 429);
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

    #[tokio::test]
    async fn dispatch_sandbox_unknown_route_is_404() {
        let config = CommerceFrontConfig::test_default();
        let input = DispatchInput {
            query: None,
            id: None,
            line_id: None,
            body: &[],
            idempotency: None,
            bearer: None,
            client_key: "anon",
        };
        let response = dispatch_sandbox_route("not_sandbox", &config, &input).await;
        assert_eq!(response.status(), 404);
    }

    #[test]
    fn get_sandbox_job_via_registry_requires_job_id() {
        let auth = AdminAuthConfig::from_token("tok");
        let registry = crate::sandbox_jobs::SandboxJobRegistry::new();
        let response = get_sandbox_job_via_registry(&auth, Some("tok"), Some(&registry), None);
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn create_sandbox_job_via_registry_requires_messenger() {
        let auth = AdminAuthConfig::from_token("tok");
        let registry = crate::sandbox_jobs::SandboxJobRegistry::new();
        let hub = crate::sandbox_realtime::SandboxJobHub::new();
        let body = br#"{"job_type":"quote","currency":"EUR","lines":[{"sku":"a","quantity":1,"unit_price_minor":1}]}"#;
        assert_eq!(
            create_sandbox_job_via_registry(
                &auth,
                Some("tok"),
                Some(&registry),
                Some(&hub),
                None,
                body,
            )
            .await
            .status(),
            500
        );
        let messenger = crate::sandbox_messenger::SandboxJobMessenger::new();
        assert_eq!(
            create_sandbox_job_via_registry(
                &auth,
                Some("tok"),
                Some(&registry),
                Some(&hub),
                Some(&messenger),
                body,
            )
            .await
            .status(),
            202
        );
    }

    #[tokio::test]
    async fn dispatch_create_sandbox_job_route_uses_messenger() {
        let registry = crate::sandbox_jobs::SandboxJobRegistry::new();
        let hub = crate::sandbox_realtime::SandboxJobHub::new();
        let messenger = crate::sandbox_messenger::SandboxJobMessenger::new();
        let config = CommerceFrontConfig {
            admin_auth: AdminAuthConfig::from_token("tok"),
            sandbox_registry: Some(registry),
            sandbox_hub: Some(hub),
            sandbox_messenger: Some(messenger.clone()),
            ..CommerceFrontConfig::test_default()
        };
        let body = br#"{"job_type":"quote","currency":"EUR","lines":[{"sku":"a","quantity":1,"unit_price_minor":1}]}"#;
        let input = DispatchInput {
            query: None,
            id: None,
            line_id: None,
            body,
            idempotency: None,
            bearer: Some("tok"),
            client_key: "anon",
        };
        let response = dispatch_sandbox_route(CREATE_SANDBOX_JOB_ROUTE, &config, &input).await;
        assert_eq!(response.status(), 202);
        assert_eq!(messenger.transport().len(), 1);
    }

    #[tokio::test]
    async fn commit_and_discard_via_registry_cover_missing_deps() {
        let auth = AdminAuthConfig::from_token("tok");
        let registry = crate::sandbox_jobs::SandboxJobRegistry::new();
        let hub = crate::sandbox_realtime::SandboxJobHub::new();

        assert_eq!(
            commit_sandbox_job_via_registry(
                &auth,
                Some("tok"),
                None,
                Some(&hub),
                None,
                None,
                Some("j")
            )
            .await
            .status(),
            500
        );
        assert_eq!(
            commit_sandbox_job_via_registry(
                &auth,
                Some("tok"),
                Some(&registry),
                None,
                None,
                None,
                Some("j")
            )
            .await
            .status(),
            500
        );
        assert_eq!(
            commit_sandbox_job_via_registry(
                &auth,
                Some("tok"),
                Some(&registry),
                Some(&hub),
                None,
                None,
                Some("j")
            )
            .await
            .status(),
            500
        );
        assert_eq!(
            commit_sandbox_job_via_registry(
                &auth,
                Some("tok"),
                Some(&registry),
                Some(&hub),
                None,
                None,
                None
            )
            .await
            .status(),
            500
        );

        assert_eq!(
            discard_sandbox_job_via_registry(&auth, Some("tok"), None, Some(&hub), Some("j"))
                .status(),
            500
        );
        assert_eq!(
            discard_sandbox_job_via_registry(&auth, Some("tok"), Some(&registry), None, Some("j"))
                .status(),
            500
        );
        assert_eq!(
            discard_sandbox_job_via_registry(&auth, Some("tok"), Some(&registry), Some(&hub), None)
                .status(),
            404
        );
    }

    #[cfg(feature = "persist-sqlx")]
    #[tokio::test]
    async fn commit_via_registry_missing_job_id_is_not_found() {
        use rustashop_persist_sqlx::SqlxCatalogRepository;
        use sqlx::postgres::PgPoolOptions;

        let auth = AdminAuthConfig::from_token("tok");
        let registry = crate::sandbox_jobs::SandboxJobRegistry::new();
        let hub = crate::sandbox_realtime::SandboxJobHub::new();
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://rustashop:rustashop@127.0.0.1:5432/rustashop".into());
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        let catalog = SqlxCatalogRepository::new(pool);
        assert_eq!(
            commit_sandbox_job_via_registry(
                &auth,
                Some("tok"),
                Some(&registry),
                Some(&hub),
                None,
                Some(&catalog),
                None
            )
            .await
            .status(),
            404
        );
    }

    #[tokio::test]
    async fn dispatch_commit_and_discard_routes() {
        let auth = AdminAuthConfig::from_token("tok");
        let registry = crate::sandbox_jobs::SandboxJobRegistry::new();
        let hub = crate::sandbox_realtime::SandboxJobHub::new();
        let job = registry.start_job("cart_quantity", "hash", "admin-bearer");
        registry.set_awaiting_commit(
            &job.id,
            crate::sandbox_jobs::SandboxProposalResponse {
                event_type: "cart.line_quantity_proposed".into(),
                cart_id: "c1".into(),
                product_id: "v1".into(),
                quantity: 1,
                operator: "set".into(),
            },
        );
        let config = CommerceFrontConfig {
            admin_auth: auth,
            sandbox_registry: Some(registry),
            readiness: serenade_http::Readiness::new(),
            sandbox_hub: Some(hub),
            ..CommerceFrontConfig::test_default()
        };
        let discard_input = DispatchInput {
            query: None,
            id: Some(job.id.as_str()),
            line_id: None,
            body: &[],
            idempotency: None,
            bearer: Some("tok"),
            client_key: "anon",
        };
        let discarded =
            dispatch_sandbox_route(DISCARD_SANDBOX_JOB_ROUTE, &config, &discard_input).await;
        assert_eq!(discarded.status(), 200);

        let commit_input = DispatchInput {
            query: None,
            id: Some("missing"),
            line_id: None,
            body: &[],
            idempotency: None,
            bearer: Some("tok"),
            client_key: "anon",
        };
        // Catalog missing → Internal before NotFound on job.
        let committed =
            dispatch_sandbox_route(COMMIT_SANDBOX_JOB_ROUTE, &config, &commit_input).await;
        assert_eq!(committed.status(), 500);
    }

    #[cfg(feature = "persist-sqlx")]
    #[actix_web::test]
    async fn commit_via_registry_dispatches_when_catalog_present() {
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
        let registry = crate::sandbox_jobs::SandboxJobRegistry::new();
        let hub = crate::sandbox_realtime::SandboxJobHub::new();
        let job = registry.start_job("cart_quantity", "hash", "admin-bearer");
        registry.set_awaiting_commit(
            &job.id,
            crate::sandbox_jobs::SandboxProposalResponse {
                event_type: "cart.line_quantity_proposed".into(),
                cart_id: "11111111-1111-1111-1111-111111111111".into(),
                product_id: "v1".into(),
                quantity: 1,
                operator: "set".into(),
            },
        );
        // Happy path through via_registry deps; cart/schema may be missing → 404 or persist 500.
        let commit_status = commit_sandbox_job_via_registry(
            &auth,
            Some("tok"),
            Some(&registry),
            Some(&hub),
            None,
            Some(&catalog),
            Some(&job.id),
        )
        .await
        .status();
        assert!(
            commit_status == 404 || commit_status == 500,
            "unexpected commit status {commit_status}"
        );
        let discard_status = discard_sandbox_job_via_registry(
            &auth,
            Some("tok"),
            Some(&registry),
            Some(&hub),
            Some(&job.id),
        )
        .status();
        assert!(
            discard_status == 200 || discard_status == 422,
            "unexpected discard status {discard_status}"
        );
    }
}
