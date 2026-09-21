//! Integration: admin order status PATCH pushes `order.updated` over WebSocket.

#![cfg(feature = "persist-sqlx")]

use std::net::SocketAddr;
use std::time::Duration;

use actix_web::{test, web};
use futures_util::{SinkExt, StreamExt};
use rustashop_api::{
    AdminAuthConfig, CartHub, CartResponse, CommerceFrontConfig, CommerceListenData,
    DEFAULT_ADMIN_API_PREFIX, OrderHub, OrderResponse, SandboxJobHub, bind_commerce_server,
    commerce_app, commerce_http_kernel,
};
use rustashop_persist::CatalogRepository;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{WebSocketStream, connect_async};

const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";
/// Distinct from other rustashop-api integration locks.
const SCHEMA_LOCK: i64 = 874_540;
const ADMIN_TOKEN: &str = "order-ws-admin-token";

static ORDER_WS_SCHEMA_GATE: Mutex<()> = Mutex::const_new(());

#[tokio::test]
async fn admin_order_patch_pushes_ws_event() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let _gate = ORDER_WS_SCHEMA_GATE.lock().await;
    let catalog = exclusive_seeded_catalog().await;
    let (addr, handle) = spawn_commerce(catalog);
    let http = reqwest::Client::new();
    let placed = place_order(&http, addr).await;

    let ws_url = format!(
        "ws://{addr}/v1/admin/orders/{}/ws?token={ADMIN_TOKEN}",
        placed.id
    );
    let (mut ws, _) = connect_async(&ws_url).await.expect("ws connect");
    tokio::time::sleep(Duration::from_millis(50)).await;

    patch_status(&http, addr, &placed.id, "paid").await;
    patch_status(&http, addr, &placed.id, "shipped").await;
    let event = wait_for_shipped(&mut ws).await;
    assert_order_updated_shipped(&event, &placed.id);

    ws.close(None).await.expect("client close");
    handle.stop(true).await;
}

#[actix_web::test]
async fn order_ws_rejects_bad_token() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    // Auth fails before catalog IO; connect only (no DROP SCHEMA) so we do not race the
    // happy-path integration test that reseeds the same database.
    let catalog = connected_catalog().await;
    let auth = AdminAuthConfig::from_token(ADMIN_TOKEN);
    let kernel = commerce_http_kernel(CommerceFrontConfig {
        admin_auth: auth.clone(),
        order_hub: Some(OrderHub::new()),
        ..CommerceFrontConfig::test_default()
    });
    let app = test::init_service(commerce_app(CommerceListenData {
        kernel: web::Data::new(kernel),
        cart_hub: web::Data::new(CartHub::new()),
        order_hub: web::Data::new(OrderHub::new()),
        catalog: web::Data::new(catalog),
        sandbox_hub: web::Data::new(SandboxJobHub::new()),
        admin_auth: web::Data::new(auth),
        admin_prefix: DEFAULT_ADMIN_API_PREFIX.to_owned(),
    }))
    .await;

    let bad = test::TestRequest::get()
        .uri("/v1/admin/orders/00000000-0000-0000-0000-000000000001/ws?token=wrong")
        .to_request();
    assert_eq!(test::call_service(&app, bad).await.status(), 401);
}

fn spawn_commerce(catalog: CatalogRepository) -> (SocketAddr, actix_web::dev::ServerHandle) {
    let order_hub = OrderHub::new();
    let auth = AdminAuthConfig::from_token(ADMIN_TOKEN);
    let kernel = commerce_http_kernel(CommerceFrontConfig {
        catalog: Some(catalog.clone()),
        admin_auth: auth.clone(),
        order_hub: Some(order_hub.clone()),
        ..CommerceFrontConfig::test_default()
    });
    let bound = bind_commerce_server(
        "127.0.0.1:0",
        CommerceListenData {
            kernel: web::Data::new(kernel),
            cart_hub: web::Data::new(CartHub::new()),
            order_hub: web::Data::new(order_hub),
            catalog: web::Data::new(catalog),
            sandbox_hub: web::Data::new(SandboxJobHub::new()),
            admin_auth: web::Data::new(auth),
            admin_prefix: DEFAULT_ADMIN_API_PREFIX.to_owned(),
        },
    )
    .expect("bind");
    let addr = bound.addrs[0];
    let handle = bound.server.handle();
    tokio::spawn(bound.server);
    (addr, handle)
}

async fn place_order(http: &reqwest::Client, addr: SocketAddr) -> OrderResponse {
    let create = http
        .post(format!("http://{addr}/v1/carts"))
        .json(&json!({ "currency": "EUR" }))
        .send()
        .await
        .expect("create cart");
    assert_eq!(
        create.status(),
        201,
        "body={}",
        create.text().await.unwrap_or_default()
    );
    let cart: CartResponse = create.json().await.expect("cart json");
    let add = http
        .post(format!("http://{addr}/v1/carts/{}/lines", cart.id))
        .json(&json!({ "variant_id": HOODIE_VARIANT, "quantity": 1 }))
        .send()
        .await
        .expect("add line");
    assert_eq!(add.status(), 200);
    let checkout = http
        .post(format!("http://{addr}/v1/checkout"))
        .json(&json!({ "cart_id": cart.id }))
        .send()
        .await
        .expect("checkout");
    assert_eq!(checkout.status(), 201);
    checkout.json().await.expect("order json")
}

async fn patch_status(http: &reqwest::Client, addr: SocketAddr, order_id: &str, status: &str) {
    let patch = http
        .patch(format!("http://{addr}/v1/admin/orders/{order_id}"))
        .header("Authorization", format!("Bearer {ADMIN_TOKEN}"))
        .json(&json!({ "status": status }))
        .send()
        .await
        .expect("patch");
    assert_eq!(patch.status(), 200);
}

async fn wait_for_shipped(
    ws: &mut WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
) -> String {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let msg = ws.next().await.expect("ws stream").expect("ws msg");
            match msg {
                Message::Text(text) => {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&text).expect("event json");
                    if parsed["type"] == "order.updated" && parsed["order"]["state"] == "shipped" {
                        break text.to_string();
                    }
                }
                Message::Ping(payload) => {
                    ws.send(Message::Pong(payload)).await.expect("pong");
                }
                other => panic!("unexpected ws frame: {other:?}"),
            }
        }
    })
    .await
    .expect("ws event timeout")
}

fn assert_order_updated_shipped(event: &str, order_id: &str) {
    let parsed: serde_json::Value = serde_json::from_str(event).expect("event json");
    assert_eq!(parsed["type"], "order.updated");
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["order"]["id"], order_id);
    assert_eq!(parsed["order"]["state"], "shipped");
    assert!(parsed["order"]["total"]["amount_minor"].as_i64().is_some());
}

async fn connected_catalog() -> CatalogRepository {
    use rustashop_persist_sqlx::SqlxCatalogRepository;
    use sqlx::postgres::PgPoolOptions;

    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("connect");
    SqlxCatalogRepository::new(pool)
}

async fn exclusive_seeded_catalog() -> CatalogRepository {
    use rustashop_persist_sqlx::{SqlxCatalogRepository, migrate, seed_catalog};
    use sqlx::postgres::PgPoolOptions;

    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("connect");
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SCHEMA_LOCK)
        .execute(&pool)
        .await
        .expect("lock");
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
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SCHEMA_LOCK)
        .execute(&pool)
        .await
        .expect("unlock");
    SqlxCatalogRepository::new(pool)
}
