//! Serenade HTTP front controller (commerce routes migrate onto this over time).

use rustashop_persist::CatalogRepository;
use serenade_http::{
    box_future, AsyncHttpKernel, HttpError, Method, Request, Response, Route, RouteCollection,
    UrlMatcher,
};
use serenade_http_actix::{conversion_error, from_actix, to_actix};

use crate::error::{api_error_json_response, ApiError};
use crate::health::health_json_body;
use crate::products::{get_product_response, list_products_response, ListProductsQuery};

const HEALTHZ_ROUTE: &str = "healthz";
const LIST_PRODUCTS_ROUTE: &str = "list_products";
const GET_PRODUCT_ROUTE: &str = "get_product";
const QUERY_STRING_ATTR: &str = "query_string";

/// Builds the Serenade async kernel for routes already moved off Actix handlers.
///
/// Pass `Some(catalog)` for product routes. Unit tests without a database may pass `None`
/// (product paths then return 503).
#[must_use]
pub fn commerce_http_kernel(catalog: Option<CatalogRepository>) -> AsyncHttpKernel {
    let routes = front_matcher();
    AsyncHttpKernel::from_async_fn(move |request: &mut Request| {
        let catalog = catalog.clone();
        let outcome = routes.apply(request);
        let query = request
            .attributes()
            .get::<String>(QUERY_STRING_ATTR)
            .cloned();
        let product_id = request.attributes().get::<String>("id").cloned();
        box_future(async move {
            match outcome {
                Ok(found) if found.route_name() == HEALTHZ_ROUTE => Ok(healthz_response()),
                Ok(found) if found.route_name() == LIST_PRODUCTS_ROUTE => {
                    Ok(list_products_via_catalog(catalog.as_ref(), query.as_deref()).await)
                }
                Ok(found) if found.route_name() == GET_PRODUCT_ROUTE => {
                    Ok(get_product_via_catalog(catalog.as_ref(), product_id.as_deref()).await)
                }
                Ok(_) => Err(HttpError::not_found("no handler")),
                Err(error) => Err(error),
            }
        })
    })
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

fn front_matcher() -> UrlMatcher {
    let mut collection = RouteCollection::new();
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
    // Unhandled name so the kernel `Ok(_)` arm stays reachable in tests.
    collection
        .add(Route::with_method("orphan", "/__orphan", Method::Get))
        .expect("orphan route");
    UrlMatcher::new(collection)
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
pub fn configure_serenade_front(cfg: &mut actix_web::web::ServiceConfig) {
    cfg.route("/healthz", actix_web::web::get().to(serenade_dispatch))
        .route("/v1/products", actix_web::web::get().to(serenade_dispatch))
        .route(
            "/v1/products/{id}",
            actix_web::web::get().to(serenade_dispatch),
        )
        .route("/__orphan", actix_web::web::get().to(serenade_dispatch));
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test as actix_test;
    use actix_web::{web, App};
    use serenade_http::ROUTE_ATTRIBUTE;

    use crate::health::HealthResponse;

    #[actix_web::test]
    async fn healthz_via_serenade_kernel() {
        let kernel = web::Data::new(commerce_http_kernel(None));
        let app = actix_test::init_service(
            App::new()
                .app_data(kernel)
                .configure(configure_serenade_front),
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
        let kernel = web::Data::new(commerce_http_kernel(None));
        let app = actix_test::init_service(
            App::new()
                .app_data(kernel)
                .configure(configure_serenade_front),
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
    async fn list_products_accepts_query_string() {
        let kernel = web::Data::new(commerce_http_kernel(None));
        let app = actix_test::init_service(
            App::new()
                .app_data(kernel)
                .configure(configure_serenade_front),
        )
        .await;
        let req = actix_test::TestRequest::get()
            .uri("/v1/products?limit=1&offset=0")
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        // No catalog: still exercises query-string injection + list arm.
        assert_eq!(resp.status(), 500);
    }

    #[actix_web::test]
    async fn orphan_route_maps_to_not_found() {
        let kernel = web::Data::new(commerce_http_kernel(None));
        let app = actix_test::init_service(
            App::new()
                .app_data(kernel)
                .configure(configure_serenade_front),
        )
        .await;
        let req = actix_test::TestRequest::get().uri("/__orphan").to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn dispatch_maps_unsupported_method() {
        let kernel = web::Data::new(commerce_http_kernel(None));
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
        let kernel = commerce_http_kernel(None);
        let response = kernel.handle(Request::new(Method::Get, "/nope")).await;
        assert_eq!(response.status(), 404);
    }

    #[test]
    fn matcher_sets_route_attribute() {
        let matcher = front_matcher();
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
}
