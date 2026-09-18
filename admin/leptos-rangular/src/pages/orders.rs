use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::{self, ORDER_STATUSES, Order};
use crate::token::use_token;

/// Orders list + status PATCH (Angular `orders_page` analog).
#[component]
pub fn OrdersPage() -> impl IntoView {
    let tokens = use_token();
    let orders = RwSignal::new(Vec::<Order>::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);

    let load = move || {
        let token = tokens.token.get();
        if token.is_empty() {
            orders.set(Vec::new());
            error.set(None);
            return;
        }
        busy.set(true);
        error.set(None);
        leptos::task::spawn_local(async move {
            match api::list_orders(&token).await {
                Ok(items) => orders.set(items),
                Err(msg) => {
                    error.set(Some(msg));
                    orders.set(Vec::new());
                }
            }
            busy.set(false);
        });
    };

    Effect::new(move |_| {
        let _ = tokens.token.get();
        load();
    });

    view! {
        <section aria-labelledby="orders-heading">
            <h1 id="orders-heading" class="admin__title">"Orders"</h1>
            <p class="admin__tagline">
                "Order list from the operator API. Change status with PATCH."
            </p>
            {move || error.get().map(|msg| view! {
                <p class="admin__error" role="alert">{msg}</p>
            })}
            {move || render_orders_body(tokens, orders, busy, error, load)}
        </section>
    }
}

fn render_orders_body(
    tokens: crate::token::TokenCtx,
    orders: RwSignal<Vec<Order>>,
    busy: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    load: impl Fn() + Copy + 'static,
) -> AnyView {
    let has_token = tokens.has_token();
    let list = orders.get();
    let is_busy = busy.get();
    if is_busy && list.is_empty() {
        return view! { <p class="admin__tagline">"Loading orders…"</p> }.into_any();
    }
    if !has_token {
        return view! {
            <p class="admin__tagline">
                "Set an admin bearer token in the bar (RUSTASHOP_ADMIN_API_TOKEN value)."
            </p>
        }
        .into_any();
    }
    if list.is_empty() {
        return view! {
            <p class="admin__empty">
                "No orders yet. Place one from the shop checkout, then refresh."
            </p>
            <button type="button" class="admin__btn admin__btn--muted" on:click=move |_| load()>
                "Refresh"
            </button>
        }
        .into_any();
    }
    view! {
        <div class="admin__panel">
            <table class="admin__table">
                <thead>
                    <tr>
                        <th scope="col">"Number"</th>
                        <th scope="col">"State"</th>
                        <th scope="col">"Total"</th>
                        <th scope="col">"Id"</th>
                        <th scope="col">"Action"</th>
                    </tr>
                </thead>
                <tbody>
                    {list
                        .iter()
                        .map(|order| order_row(order, tokens, orders, busy, error, load))
                        .collect_view()}
                </tbody>
            </table>
        </div>
        <p class="admin__toolbar">
            <button
                type="button"
                class="admin__btn admin__btn--muted"
                disabled=move || busy.get()
                on:click=move |_| load()
            >
                "Refresh"
            </button>
        </p>
    }
    .into_any()
}

fn order_row(
    order: &Order,
    tokens: crate::token::TokenCtx,
    orders: RwSignal<Vec<Order>>,
    busy: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    load: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let order_id = order.id.clone();
    let current_state = order.state.clone();
    let number = order.number.clone();
    let total = order.total.display();
    let id_display = order.id.clone();
    let state_label = order.state.clone();
    let state_for_options = order.state.as_str();
    let select_value = current_state.clone();

    let on_change = move |ev: web_sys::Event| {
        let select: web_sys::HtmlSelectElement = ev.target().unwrap().unchecked_into();
        let status = select.value();
        if status == current_state {
            return;
        }
        let token = tokens.token.get();
        if token.is_empty() {
            return;
        }
        let oid = order_id.clone();
        busy.set(true);
        error.set(None);
        leptos::task::spawn_local(async move {
            match api::patch_order_status(&token, &oid, &status).await {
                Ok(updated) => {
                    orders.update(|rows| {
                        for row in rows.iter_mut() {
                            if row.id == updated.id {
                                *row = updated.clone();
                            }
                        }
                    });
                }
                Err(msg) => {
                    error.set(Some(msg));
                    load();
                }
            }
            busy.set(false);
        });
    };

    let badge_class = match state_for_options {
        "placed" => "admin__badge admin__badge--placed",
        "paid" => "admin__badge admin__badge--paid",
        "shipped" => "admin__badge admin__badge--shipped",
        "cancelled" => "admin__badge admin__badge--cancelled",
        _ => "admin__badge",
    };

    view! {
        <tr>
            <td>{number}</td>
            <td><span class=badge_class>{state_label}</span></td>
            <td>{total}</td>
            <td><code>{id_display}</code></td>
            <td>
                <select
                    class="admin__select"
                    prop:value=select_value
                    disabled=move || busy.get()
                    on:change=on_change
                >
                    {ORDER_STATUSES
                        .iter()
                        .map(|status| {
                            let selected = *status == state_for_options;
                            view! {
                                <option value=*status selected=selected>{*status}</option>
                            }
                        })
                        .collect_view()}
                </select>
            </td>
        </tr>
    }
}
