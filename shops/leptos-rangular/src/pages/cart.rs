use leptos::prelude::*;
use leptos_router::NavigateOptions;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::api::{self, Cart};
use crate::cart::{CartCtx, use_cart};
use crate::checkout::{new_idempotency_key, validate_checkout_email};
use crate::order::{OrderCtx, use_order};

struct CartCheckoutUi {
    busy: bool,
    checking_out: bool,
    cart: Option<Cart>,
    email: RwSignal<String>,
    checking_out_sig: RwSignal<bool>,
    page_error: RwSignal<Option<String>>,
    cart_ctx: CartCtx,
    order_ctx: OrderCtx,
}

/// Cart snapshot from the Commerce API (shared `rs.cartId` with Angular).
#[component]
pub fn CartPage() -> impl IntoView {
    let cart_ctx = use_cart();
    let order_ctx = use_order();
    let navigate = use_navigate();
    let email = RwSignal::new(String::new());
    let checking_out = RwSignal::new(false);
    let page_error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            let _ = cart_ctx.refresh().await;
        });
    });

    view! {
        <section class="shop" aria-labelledby="cart-heading">
            <header class="shop__hero">
                <h1 id="cart-heading" class="shop__title">"Cart"</h1>
                <p class="shop__tagline">"Shared cart id with the Angular shop when both use this browser."</p>
            </header>
            <p class="shop__tagline">
                <A href="/">"Continue shopping"</A>
            </p>
            {move || {
                page_error
                    .get()
                    .or_else(|| cart_ctx.error.get())
                    .map(|message| {
                        view! { <p class="shop__error" role="alert">{message}</p> }.into_any()
                    })
            }}
            {move || {
                let nav = navigate.clone();
                cart_main(
                    CartCheckoutUi {
                        busy: cart_ctx.busy.get(),
                        checking_out: checking_out.get(),
                        cart: cart_ctx.cart.get(),
                        email,
                        checking_out_sig: checking_out,
                        page_error,
                        cart_ctx,
                        order_ctx,
                    },
                    nav,
                )
            }}
        </section>
    }
}

fn cart_main(
    ui: CartCheckoutUi,
    navigate: impl Fn(&str, leptos_router::NavigateOptions) + Clone + 'static,
) -> AnyView {
    if ui.busy && ui.cart.is_none() {
        return view! { <p class="shop__tagline">"Loading cart…"</p> }.into_any();
    }
    let Some(cart) = ui.cart else {
        return view! { <p class="shop__tagline">"Your cart is empty."</p> }.into_any();
    };
    if cart.lines.is_empty() {
        return view! { <p class="shop__tagline">"Your cart is empty."</p> }.into_any();
    }
    let total = cart.items_total.display();
    let currency = cart.currency_code().to_owned();
    let disabled = ui.busy || ui.checking_out;
    let email = ui.email;
    let checking_out_sig = ui.checking_out_sig;
    let page_error = ui.page_error;
    let cart_ctx = ui.cart_ctx;
    let order_ctx = ui.order_ctx;
    let checking_out = ui.checking_out;
    let on_checkout = move |_| {
        page_error.set(None);
        let email_raw = email.get();
        let email_ok = match validate_checkout_email(&email_raw) {
            Ok(v) => v,
            Err(msg) => {
                page_error.set(Some(msg.to_owned()));
                return;
            }
        };
        checking_out_sig.set(true);
        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            let result = place_order_flow(cart_ctx, email_ok.as_deref()).await;
            checking_out_sig.set(false);
            match result {
                Ok(order) => {
                    order_ctx.last.set(Some(order.clone()));
                    cart_ctx.clear_session();
                    navigate(
                        &format!("/checkout/{}", order.id),
                        NavigateOptions::default(),
                    );
                }
                Err(err) => page_error.set(Some(err)),
            }
        });
    };
    view! {
        <ul class="shop__cart-lines">
            <For
                each=move || cart.lines.clone()
                key=|line| format!("{}:{}", line.id, line.variant_ref())
                children=move |line| {
                    view! {
                        <li class="shop__cart-line">
                            <strong>{line.product_name}</strong>
                            <span class="shop__tagline">{line.variant_sku}</span>
                            <span>{format!("× {}", line.quantity)}</span>
                            <span>{line.unit_price.display()}</span>
                            <span>{line.line_total.display()}</span>
                        </li>
                    }
                }
            />
        </ul>
        <p class="shop__title">{format!("Total: {total} ({currency})")}</p>
        <label class="shop__tagline">
            "Email (optional)"
            <input
                type="email"
                prop:value=move || email.get()
                on:input=move |ev| email.set(event_target_value(&ev))
                disabled=disabled
            />
        </label>
        <p class="shop__tagline">
            <button type="button" on:click=on_checkout disabled=disabled>
                {if checking_out { "Placing order…" } else { "Checkout" }}
            </button>
        </p>
    }
    .into_any()
}

async fn place_order_flow(
    cart_ctx: CartCtx,
    email: Option<&str>,
) -> Result<crate::api::Order, String> {
    let cart = cart_ctx.ensure_cart().await?;
    if cart.lines.is_empty() {
        return Err("Cart is empty.".into());
    }
    let key = new_idempotency_key();
    api::place_order(&cart.id, email, &key).await
}
