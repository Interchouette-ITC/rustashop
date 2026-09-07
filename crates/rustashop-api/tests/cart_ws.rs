//! Integration: HTTP cart mutation pushes a WebSocket `cart.updated` event.

#![cfg(feature = "persist-sqlx")]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rustashop_api::{
    AdminAuthConfig, CartHub, CartResponse, CommerceFrontConfig, DEFAULT_ADMIN_API_PREFIX,
    SandboxJobHub, bind_commerce_server, commerce_http_kernel,
};
use rustashop_persist::CatalogRepository;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";
const SCHEMA_LOCK: i64 = 874_514;

#[tokio::test]
async fn cart_line_add_pushes_ws_event() {
    let Ok(_) = std::env::var("DATABASE_URL") else {
        eprintln!("skip: DATABASE_URL is not set");
        return;
    };
    let catalog = exclusive_seeded_catalog().await;
    let hub = CartHub::new();
    let kernel = commerce_http_kernel(CommerceFrontConfig {
        catalog: Some(catalog.clone()),
        cart_hub: Some(hub.clone()),
        ..CommerceFrontConfig::test_default()
    });
    let bound = bind_commerce_server(
        "127.0.0.1:0",
        kernel,
        hub,
        catalog,
        SandboxJobHub::new(),
        AdminAuthConfig::from_token(""),
        DEFAULT_ADMIN_API_PREFIX,
    )
    .expect("bind");
    let addr = bound.addrs[0];
    let handle = bound.server.handle();
    tokio::spawn(bound.server);

    let http = reqwest::Client::new();
    let create = http
        .post(format!("http://{addr}/v1/carts"))
        .json(&json!({ "currency": "EUR" }))
        .send()
        .await
        .expect("create cart");
    assert_eq!(create.status(), 201);
    let cart: CartResponse = create.json().await.expect("cart json");

    let ws_url = format!("ws://{addr}/v1/carts/{}/ws?token={}", cart.id, cart.token);
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("ws connect");

    tokio::time::sleep(Duration::from_millis(50)).await;

    let add = http
        .post(format!("http://{addr}/v1/carts/{}/lines", cart.id))
        .json(&json!({ "variant_id": HOODIE_VARIANT, "quantity": 2 }))
        .send()
        .await
        .expect("add line");
    assert_eq!(add.status(), 200);

    let event = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let msg = ws.next().await.expect("ws stream").expect("ws msg");
            match msg {
                Message::Text(text) => break text,
                Message::Ping(payload) => {
                    ws.send(Message::Pong(payload)).await.expect("pong");
                }
                other => panic!("unexpected ws frame: {other:?}"),
            }
        }
    })
    .await
    .expect("ws event timeout");

    let parsed: serde_json::Value = serde_json::from_str(&event).expect("event json");
    assert_eq!(parsed["type"], "cart.updated");
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["cart"]["id"], cart.id);
    assert_eq!(parsed["cart"]["items_total"]["amount_minor"], 9000);

    ws.send(Message::Ping(vec![b'x'].into()))
        .await
        .expect("client ping");
    tokio::time::sleep(Duration::from_millis(50)).await;
    ws.close(None).await.expect("client close");

    handle.stop(true).await;
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
