//! Integration: admin sandbox job HTTP + WS (#39).

#![cfg(feature = "persist-sqlx")]

use std::time::Duration;

use actix_web::{App, test, web};
use futures_util::{SinkExt, StreamExt};
use rustashop_api::{
    AdminAuthConfig, CartHub, CartResponse, CommerceFrontConfig, CommerceListenData,
    CommitSandboxJobResponse, DEFAULT_ADMIN_API_PREFIX, SandboxJobHub, SandboxJobRegistry,
    SandboxJobResponse, SandboxJobStatus, bind_commerce_server, commerce_app, commerce_http_kernel,
    routes,
};
use rustashop_persist::CatalogRepository;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

const ADMIN_TOKEN: &str = "sandbox-admin-token";

#[actix_web::test]
async fn sandbox_jobs_require_bearer_and_run_quote() {
    let registry = SandboxJobRegistry::new();
    let hub = SandboxJobHub::new();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(commerce_http_kernel(CommerceFrontConfig {
                admin_auth: AdminAuthConfig::from_token(ADMIN_TOKEN),
                sandbox_hub: Some(hub.clone()),
                sandbox_registry: Some(registry.clone()),
                ..CommerceFrontConfig::test_default()
            })))
            .configure(routes),
    )
    .await;

    let denied = test::TestRequest::post()
        .uri("/v1/admin/sandbox/jobs")
        .set_json(json!({
            "job_type": "quote",
            "currency": "EUR",
            "lines": [{"sku": "HOODIE-M", "quantity": 2, "unit_price_minor": 5000}]
        }))
        .to_request();
    assert_eq!(test::call_service(&app, denied).await.status(), 401);

    let bad_type = test::TestRequest::post()
        .uri("/v1/admin/sandbox/jobs")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .set_json(json!({
            "job_type": "other",
            "currency": "EUR",
            "lines": [{"sku": "X", "quantity": 1, "unit_price_minor": 100}]
        }))
        .to_request();
    assert_eq!(test::call_service(&app, bad_type).await.status(), 422);

    let empty_lines = test::TestRequest::post()
        .uri("/v1/admin/sandbox/jobs")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .set_json(json!({
            "job_type": "quote",
            "currency": "EUR",
            "lines": []
        }))
        .to_request();
    assert_eq!(test::call_service(&app, empty_lines).await.status(), 422);

    let create = test::TestRequest::post()
        .uri("/v1/admin/sandbox/jobs")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .set_json(json!({
            "job_type": "quote",
            "currency": "EUR",
            "lines": [{"sku": "HOODIE-M", "quantity": 2, "unit_price_minor": 5000}]
        }))
        .to_request();
    let create_resp = test::call_service(&app, create).await;
    assert_eq!(create_resp.status(), 202);
    let job: SandboxJobResponse = test::read_body_json(create_resp).await;
    assert_eq!(job.job_type, "quote");

    let mut final_job = job;
    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let get = test::TestRequest::get()
            .uri(&format!("/v1/admin/sandbox/jobs/{}", final_job.id))
            .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
            .to_request();
        let get_resp = test::call_service(&app, get).await;
        assert!(get_resp.status().is_success());
        final_job = test::read_body_json(get_resp).await;
        if final_job.status != SandboxJobStatus::Running {
            break;
        }
    }
    assert_eq!(
        final_job.status,
        SandboxJobStatus::Succeeded,
        "error={:?}",
        final_job.error
    );
    let adjustments = final_job.adjustments.expect("adjustments");
    assert_eq!(adjustments.len(), 1);
    assert_eq!(adjustments[0].label, "volume-discount");

    let missing = test::TestRequest::get()
        .uri("/v1/admin/sandbox/jobs/deadbeef")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .to_request();
    assert_eq!(test::call_service(&app, missing).await.status(), 404);

    let audit_denied = test::TestRequest::get()
        .uri("/v1/admin/sandbox/audit")
        .to_request();
    assert_eq!(test::call_service(&app, audit_denied).await.status(), 401);

    let audit = test::TestRequest::get()
        .uri("/v1/admin/sandbox/audit")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .to_request();
    let audit_resp = test::call_service(&app, audit).await;
    assert!(audit_resp.status().is_success());
    let rows: Vec<serde_json::Value> = test::read_body_json(audit_resp).await;
    assert_ne!(rows.len(), 0);
    assert_eq!(rows[0]["status"], "succeeded");
}

#[actix_web::test]
async fn sandbox_without_registry_returns_internal() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(commerce_http_kernel(CommerceFrontConfig {
                admin_auth: AdminAuthConfig::from_token(ADMIN_TOKEN),
                ..CommerceFrontConfig::test_default()
            })))
            .configure(routes),
    )
    .await;

    let create = test::TestRequest::post()
        .uri("/v1/admin/sandbox/jobs")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .set_json(json!({
            "job_type": "quote",
            "currency": "EUR",
            "lines": [{"sku": "X", "quantity": 1, "unit_price_minor": 100}]
        }))
        .to_request();
    assert_eq!(test::call_service(&app, create).await.status(), 500);

    let get = test::TestRequest::get()
        .uri("/v1/admin/sandbox/jobs/x")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .to_request();
    assert_eq!(test::call_service(&app, get).await.status(), 500);

    let audit = test::TestRequest::get()
        .uri("/v1/admin/sandbox/audit")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .to_request();
    assert_eq!(test::call_service(&app, audit).await.status(), 500);
}

#[actix_web::test]
async fn sandbox_create_without_hub_returns_internal() {
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(commerce_http_kernel(CommerceFrontConfig {
                admin_auth: AdminAuthConfig::from_token(ADMIN_TOKEN),
                sandbox_registry: Some(SandboxJobRegistry::new()),
                ..CommerceFrontConfig::test_default()
            })))
            .configure(routes),
    )
    .await;

    let create = test::TestRequest::post()
        .uri("/v1/admin/sandbox/jobs")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .set_json(json!({
            "job_type": "quote",
            "currency": "EUR",
            "lines": [{"sku": "X", "quantity": 1, "unit_price_minor": 100}]
        }))
        .to_request();
    assert_eq!(test::call_service(&app, create).await.status(), 500);
}

#[actix_web::test]
async fn sandbox_job_ws_rejects_bad_token() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let (catalog, _db_lock) = exclusive_seeded_catalog().await;
    let hub = SandboxJobHub::new();
    let auth = AdminAuthConfig::from_token(ADMIN_TOKEN);
    let kernel = commerce_http_kernel(CommerceFrontConfig {
        admin_auth: auth.clone(),
        sandbox_hub: Some(hub.clone()),
        sandbox_registry: Some(SandboxJobRegistry::new()),
        ..CommerceFrontConfig::test_default()
    });
    let app = test::init_service(commerce_app(CommerceListenData {
        kernel: web::Data::new(kernel),
        cart_hub: web::Data::new(CartHub::new()),
        catalog: web::Data::new(catalog),
        sandbox_hub: web::Data::new(hub),
        admin_auth: web::Data::new(auth),
        admin_prefix: DEFAULT_ADMIN_API_PREFIX.to_owned(),
    }))
    .await;

    let bad = test::TestRequest::get()
        .uri("/v1/admin/sandbox/jobs/job-1/ws?token=wrong")
        .to_request();
    assert_eq!(test::call_service(&app, bad).await.status(), 401);
}

#[tokio::test]
async fn sandbox_job_ws_streams_finished_event() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let (catalog, _db_lock) = exclusive_seeded_catalog().await;
    let hub = SandboxJobHub::new();
    let registry = SandboxJobRegistry::new();
    let auth = AdminAuthConfig::from_token(ADMIN_TOKEN);
    let kernel = commerce_http_kernel(CommerceFrontConfig {
        admin_auth: auth.clone(),
        sandbox_hub: Some(hub.clone()),
        sandbox_registry: Some(registry),
        ..CommerceFrontConfig::test_default()
    });
    let bound = bind_commerce_server(
        "127.0.0.1:0",
        kernel,
        CartHub::new(),
        catalog,
        hub,
        auth,
        DEFAULT_ADMIN_API_PREFIX,
    )
    .expect("bind");
    let addr = bound.addrs[0];
    let handle = bound.server.handle();
    tokio::spawn(bound.server);

    let http = reqwest::Client::new();
    let create = http
        .post(format!("http://{addr}/v1/admin/sandbox/jobs"))
        .header("Authorization", format!("Bearer {ADMIN_TOKEN}"))
        .json(&json!({
            "job_type": "quote",
            "currency": "EUR",
            "lines": [{"sku": "HOODIE-M", "quantity": 2, "unit_price_minor": 5000}]
        }))
        .send()
        .await
        .expect("create");
    assert_eq!(create.status(), 202);
    let job: SandboxJobResponse = create.json().await.expect("job json");

    let ws_url = format!(
        "ws://{addr}/v1/admin/sandbox/jobs/{}/ws?token={ADMIN_TOKEN}",
        job.id
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("ws connect");

    let mut saw_finished = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while tokio::time::Instant::now() < deadline {
        let next = tokio::time::timeout(Duration::from_secs(5), ws.next()).await;
        let Ok(Some(Ok(msg))) = next else {
            continue;
        };
        match msg {
            Message::Text(text) => {
                let value: serde_json::Value = serde_json::from_str(&text).expect("json");
                if value["type"] == "job.finished" {
                    saw_finished = true;
                    break;
                }
            }
            Message::Ping(payload) => {
                ws.send(Message::Pong(payload)).await.expect("pong");
            }
            _ => {}
        }
    }
    let _ = ws.close(None).await;
    handle.stop(true).await;
    assert!(saw_finished, "expected job.finished on websocket");
}

const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";

#[tokio::test]
async fn autonomous_cart_quantity_awaits_commit_then_mutates_cart() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let (catalog, _db_lock) = exclusive_seeded_catalog().await;
    let (addr, handle) = bind_autonomous_server(catalog);
    let http = reqwest::Client::new();
    let cart = seed_cart_with_hoodie(&http, addr).await;
    let job = start_cart_quantity_job(&http, addr, &cart.id, 5).await;

    let ws_url = format!(
        "ws://{addr}/v1/admin/sandbox/jobs/{}/ws?token={ADMIN_TOKEN}",
        job.id
    );
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("ws connect");
    wait_for_awaiting_commit(&http, addr, &job.id, &mut ws).await;

    let before = fetch_cart(&http, addr, &cart.id).await;
    assert_eq!(before.lines[0].quantity, 2);

    let commit = http
        .post(format!(
            "http://{addr}/v1/admin/sandbox/jobs/{}/commit",
            job.id
        ))
        .header("Authorization", format!("Bearer {ADMIN_TOKEN}"))
        .send()
        .await
        .expect("commit");
    assert_eq!(commit.status(), 200);
    let committed: CommitSandboxJobResponse = commit.json().await.expect("commit json");
    assert_eq!(committed.job.status, SandboxJobStatus::Committed);
    assert_eq!(committed.cart.lines[0].quantity, 5);
    assert_eq!(fetch_cart(&http, addr, &cart.id).await.lines[0].quantity, 5);

    let _ = ws.close(None).await;
    handle.stop(true).await;
}

#[tokio::test]
async fn autonomous_cart_quantity_discard_leaves_cart() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let (catalog, _db_lock) = exclusive_seeded_catalog().await;
    let registry = SandboxJobRegistry::new();
    let hub = SandboxJobHub::new();
    let auth = AdminAuthConfig::from_token(ADMIN_TOKEN);
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(commerce_http_kernel(CommerceFrontConfig {
                catalog: Some(catalog.clone()),
                admin_auth: auth,
                sandbox_hub: Some(hub),
                sandbox_registry: Some(registry),
                ..CommerceFrontConfig::test_default()
            })))
            .configure(routes),
    )
    .await;

    let create_cart = test::TestRequest::post()
        .uri("/v1/carts")
        .set_json(json!({ "currency": "EUR" }))
        .to_request();
    let cart_resp = test::call_service(&app, create_cart).await;
    assert_eq!(cart_resp.status(), 201);
    let cart: CartResponse = test::read_body_json(cart_resp).await;

    let add = test::TestRequest::post()
        .uri(&format!("/v1/carts/{}/lines", cart.id))
        .set_json(json!({ "variant_id": HOODIE_VARIANT, "quantity": 2 }))
        .to_request();
    assert_eq!(test::call_service(&app, add).await.status(), 200);

    let create = test::TestRequest::post()
        .uri("/v1/admin/sandbox/jobs")
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .set_json(json!({
            "job_type": "cart_quantity",
            "cart_id": cart.id,
            "variant_id": HOODIE_VARIANT,
            "quantity": 9,
            "operator": "set"
        }))
        .to_request();
    let create_resp = test::call_service(&app, create).await;
    assert_eq!(create_resp.status(), 202);
    let job: SandboxJobResponse = test::read_body_json(create_resp).await;

    let mut waiting = job;
    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let get = test::TestRequest::get()
            .uri(&format!("/v1/admin/sandbox/jobs/{}", waiting.id))
            .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
            .to_request();
        let get_resp = test::call_service(&app, get).await;
        waiting = test::read_body_json(get_resp).await;
        if waiting.status != SandboxJobStatus::Running {
            break;
        }
    }
    assert_eq!(waiting.status, SandboxJobStatus::AwaitingCommit);

    let discard = test::TestRequest::post()
        .uri(&format!("/v1/admin/sandbox/jobs/{}/discard", waiting.id))
        .insert_header(("Authorization", format!("Bearer {ADMIN_TOKEN}")))
        .to_request();
    let discard_resp = test::call_service(&app, discard).await;
    assert_eq!(discard_resp.status(), 200);
    let discarded: SandboxJobResponse = test::read_body_json(discard_resp).await;
    assert_eq!(discarded.status, SandboxJobStatus::Discarded);

    let get_cart = test::TestRequest::get()
        .uri(&format!("/v1/carts/{}", cart.id))
        .to_request();
    let cart_after: CartResponse =
        test::read_body_json(test::call_service(&app, get_cart).await).await;
    assert_eq!(cart_after.lines[0].quantity, 2);
}

fn bind_autonomous_server(
    catalog: CatalogRepository,
) -> (std::net::SocketAddr, actix_web::dev::ServerHandle) {
    let sandbox_hub = SandboxJobHub::new();
    let cart_hub = CartHub::new();
    let auth = AdminAuthConfig::from_token(ADMIN_TOKEN);
    let kernel = commerce_http_kernel(CommerceFrontConfig {
        catalog: Some(catalog.clone()),
        admin_auth: auth.clone(),
        cart_hub: Some(cart_hub.clone()),
        sandbox_hub: Some(sandbox_hub.clone()),
        sandbox_registry: Some(SandboxJobRegistry::new()),
        ..CommerceFrontConfig::test_default()
    });
    let bound = bind_commerce_server(
        "127.0.0.1:0",
        kernel,
        cart_hub,
        catalog,
        sandbox_hub,
        auth,
        DEFAULT_ADMIN_API_PREFIX,
    )
    .expect("bind");
    let addr = bound.addrs[0];
    let handle = bound.server.handle();
    tokio::spawn(bound.server);
    (addr, handle)
}

async fn seed_cart_with_hoodie(http: &reqwest::Client, addr: std::net::SocketAddr) -> CartResponse {
    let create_cart = http
        .post(format!("http://{addr}/v1/carts"))
        .json(&json!({ "currency": "EUR" }))
        .send()
        .await
        .expect("create cart");
    assert_eq!(create_cart.status(), 201);
    let cart: CartResponse = create_cart.json().await.expect("cart");
    let add = http
        .post(format!("http://{addr}/v1/carts/{}/lines", cart.id))
        .json(&json!({ "variant_id": HOODIE_VARIANT, "quantity": 2 }))
        .send()
        .await
        .expect("add line");
    assert_eq!(add.status(), 200);
    cart
}

async fn start_cart_quantity_job(
    http: &reqwest::Client,
    addr: std::net::SocketAddr,
    cart_id: &str,
    quantity: u32,
) -> SandboxJobResponse {
    let create_job = http
        .post(format!("http://{addr}/v1/admin/sandbox/jobs"))
        .header("Authorization", format!("Bearer {ADMIN_TOKEN}"))
        .json(&json!({
            "job_type": "cart_quantity",
            "cart_id": cart_id,
            "variant_id": HOODIE_VARIANT,
            "quantity": quantity,
            "operator": "set"
        }))
        .send()
        .await
        .expect("create job");
    assert_eq!(create_job.status(), 202);
    create_job.json().await.expect("job")
}

async fn wait_for_awaiting_commit(
    http: &reqwest::Client,
    addr: std::net::SocketAddr,
    job_id: &str,
    ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    while tokio::time::Instant::now() < deadline {
        let get = http
            .get(format!("http://{addr}/v1/admin/sandbox/jobs/{job_id}"))
            .header("Authorization", format!("Bearer {ADMIN_TOKEN}"))
            .send()
            .await
            .expect("get job");
        let current: SandboxJobResponse = get.json().await.expect("job json");
        if current.status == SandboxJobStatus::AwaitingCommit {
            return;
        }
        assert!(
            current.status != SandboxJobStatus::Failed,
            "job failed: {:?}",
            current.error
        );
        let _ = tokio::time::timeout(Duration::from_millis(200), ws.next()).await;
    }
    panic!("expected awaiting_commit");
}

async fn fetch_cart(
    http: &reqwest::Client,
    addr: std::net::SocketAddr,
    cart_id: &str,
) -> CartResponse {
    http.get(format!("http://{addr}/v1/carts/{cart_id}"))
        .send()
        .await
        .expect("get cart")
        .json::<CartResponse>()
        .await
        .expect("cart json")
}

static DB_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn exclusive_seeded_catalog() -> (CatalogRepository, tokio::sync::MutexGuard<'static, ()>) {
    use rustashop_persist_sqlx::{SqlxCatalogRepository, migrate, seed_catalog};
    use sqlx::postgres::PgPoolOptions;

    let guard = DB_MUTEX.lock().await;
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect");
    sqlx::query("DROP SCHEMA public CASCADE")
        .execute(&pool)
        .await
        .expect("drop");
    sqlx::query("CREATE SCHEMA public")
        .execute(&pool)
        .await
        .expect("create");
    migrate(&pool).await.expect("migrate");
    seed_catalog(&pool).await.expect("seed");
    (SqlxCatalogRepository::new(pool), guard)
}
