use leptos::prelude::*;

use crate::api::{self, Product};
use crate::token::use_token;

/// Products list (Angular `products_page` analog).
#[component]
pub fn ProductsPage() -> impl IntoView {
    let tokens = use_token();
    let products = RwSignal::new(Vec::<Product>::new());
    let busy = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);

    let reload = move || {
        let token = tokens.token.get();
        if token.is_empty() {
            products.set(Vec::new());
            error.set(None);
            return;
        }
        busy.set(true);
        error.set(None);
        leptos::task::spawn_local(async move {
            match api::list_products(&token).await {
                Ok(items) => products.set(items),
                Err(msg) => {
                    error.set(Some(msg));
                    products.set(Vec::new());
                }
            }
            busy.set(false);
        });
    };

    Effect::new(move |_| {
        let _ = tokens.token.get();
        reload();
    });

    view! {
        <section aria-labelledby="products-heading">
            <h1 id="products-heading" class="admin__title">"Products"</h1>
            <p class="admin__tagline">"Product list from the operator API (includes disabled)."</p>
            {move || error.get().map(|msg| view! {
                <p class="admin__error" role="alert">{msg}</p>
            })}
            {move || render_products_body(tokens, products, busy, reload)}
        </section>
    }
}

fn render_products_body(
    tokens: crate::token::TokenCtx,
    products: RwSignal<Vec<Product>>,
    busy: RwSignal<bool>,
    reload: impl Fn() + Copy + 'static,
) -> AnyView {
    let has_token = tokens.has_token();
    let list = products.get();
    let is_busy = busy.get();
    if is_busy && list.is_empty() {
        return view! { <p class="admin__tagline">"Loading products…"</p> }.into_any();
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
            <p class="admin__empty">"No products."</p>
            <button type="button" class="admin__btn admin__btn--muted" on:click=move |_| reload()>
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
                        <th scope="col">"Name"</th>
                        <th scope="col">"Slug"</th>
                        <th scope="col">"Enabled"</th>
                        <th scope="col">"Id"</th>
                    </tr>
                </thead>
                <tbody>
                    {list
                        .into_iter()
                        .map(|product| {
                            let (enabled_label, enabled_class) = if product.enabled {
                                ("yes", "admin__badge admin__badge--on")
                            } else {
                                ("no", "admin__badge admin__badge--off")
                            };
                            view! {
                                <tr>
                                    <td>{product.name}</td>
                                    <td>{product.slug}</td>
                                    <td><span class=enabled_class>{enabled_label}</span></td>
                                    <td><code>{product.id}</code></td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </div>
        <p class="admin__toolbar">
            <button
                type="button"
                class="admin__btn admin__btn--muted"
                disabled=move || busy.get()
                on:click=move |_| reload()
            >
                "Refresh"
            </button>
        </p>
    }
    .into_any()
}
