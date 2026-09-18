use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::Order;
use crate::order::use_order;

/// Order confirmation after `POST /v1/checkout` (Angular `checkout_page` analog).
#[component]
pub fn CheckoutPage() -> impl IntoView {
    let params = use_params_map();
    let order_ctx = use_order();

    view! {
        <section class="shop" aria-labelledby="checkout-heading">
            <h1 id="checkout-heading" class="shop__title">"Order placed"</h1>
            {move || {
                let order_id = params.with(|p| p.get("orderId"));
                let stored = order_ctx.last.get();
                checkout_body(order_id.as_deref(), stored)
            }}
        </section>
    }
}

fn checkout_body(order_id: Option<&str>, stored: Option<Order>) -> AnyView {
    let matched = stored.filter(|order| order_id.is_none_or(|id| id == order.id));
    let Some(order) = matched else {
        return view! {
            <p class="shop__error" role="alert">
                "No order details in this session. Place a new order from the cart."
            </p>
            <p class="shop__tagline">
                <A href="/cart">"Back to cart"</A>
            </p>
        }
        .into_any();
    };
    let total = order.total.display();
    let items = order.items_total.display();
    let line_count = order.lines.len();
    let first_line = order.lines.first().map(|line| {
        format!(
            "{} ({}) · {} · unit {} · line {}",
            line.product_name,
            line.variant_sku,
            line.quantity,
            line.unit_price.display(),
            line.line_total.display(),
        )
    });
    let first_id = order.lines.first().map(|line| line.id.clone());
    view! {
        <div class="shop__cart-lines">
            <p class="shop__tagline">
                "Order id: "
                <code>{order.id.clone()}</code>
            </p>
            <p class="shop__tagline">
                "Number: "
                <strong>{order.number.clone()}</strong>
            </p>
            <p class="shop__tagline">
                {format!(
                    "State: {} · Payment: {} · Currency: {}",
                    order.state, order.payment_status, order.currency
                )}
            </p>
            <p class="shop__tagline">
                {format!("Lines: {line_count} · Items total: {items}")}
            </p>
            {first_id.map(|id| {
                view! { <p class="shop__tagline">{format!("First line id: {id}")}</p> }
            })}
            {first_line.map(|summary| {
                view! { <p class="shop__tagline">{summary}</p> }
            })}
            <p class="shop__title">{format!("Total: {total}")}</p>
        </div>
        <p class="shop__tagline">
            <A href="/">"Back to catalog"</A>
        </p>
    }
    .into_any()
}
