use leptos::prelude::*;
use leptos_router::components::A;
use wasm_bindgen::JsCast;

use crate::api::{API_BASE_STORAGE_KEY, api_base, save_api_base};
use crate::cart::use_cart;

/// Leptos shop chrome (nav + cart badge). Markup/SCSS for Angular `shop_shell` live in `templates/shop/default`.
#[component]
pub fn ShopShell(children: Children) -> impl IntoView {
    let cart = use_cart();
    let api_draft = RwSignal::new(api_base());

    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            let _ = cart.ensure_cart().await;
        });
    });

    let on_save_api = move |_| {
        save_api_base(&api_draft.get());
        if let Some(window) = web_sys::window() {
            let _ = window.location().reload();
        }
    };

    view! {
        <div class="shell">
            <header class="shell__bar">
                <div class="shell__inner">
                    <A href="/" attr:class="shell__brand" exact=true>
                        <span class="shell__name">"rustashop"</span>
                    </A>
                    <nav class="shell__nav" aria-label="Shop">
                        <A href="/" exact=true attr:class="">
                            "Catalog"
                        </A>
                        <A href="/cart" attr:class="">
                            "Cart"
                            {move || {
                                let count = cart.line_count();
                                (count > 0).then(|| view! {
                                    <span class="shell__badge">" " {count}</span>
                                })
                            }}
                        </A>
                    </nav>
                    <div class="shell__api">
                        <label class="visually-hidden" for="shop-api-base">"API base URL"</label>
                        <input
                            id="shop-api-base"
                            type="text"
                            class="shell__api-input"
                            placeholder="API base"
                            title=format!("sessionStorage {API_BASE_STORAGE_KEY}; empty restores default")
                            prop:value=move || api_draft.get()
                            on:input=move |ev| {
                                let input: web_sys::HtmlInputElement =
                                    ev.target().unwrap().unchecked_into();
                                api_draft.set(input.value());
                            }
                            autocomplete="off"
                        />
                        <button type="button" class="shell__api-btn" on:click=on_save_api>
                            "Save API"
                        </button>
                    </div>
                </div>
            </header>
            <main class="shell__main">
                {children()}
            </main>
        </div>
    }
}
