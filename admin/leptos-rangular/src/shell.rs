use leptos::prelude::*;
use leptos_router::components::A;
use wasm_bindgen::JsCast;

use crate::api::{API_BASE_STORAGE_KEY, api_base, save_api_base};
use crate::auth::validate_admin_token;
use crate::token::use_token;

/// Admin chrome: brand, nav, bearer token bar (Angular `admin_shell` analog).
#[component]
pub fn AdminShell(children: Children) -> impl IntoView {
    let tokens = use_token();
    let draft = RwSignal::new(tokens.token.get_untracked());
    let api_draft = RwSignal::new(api_base());
    let error = RwSignal::new(Option::<String>::None);

    let on_save = move |_| match validate_admin_token(&draft.get()) {
        Ok(value) => {
            tokens.save(&value);
            error.set(None);
        }
        Err(msg) => error.set(Some(msg.to_owned())),
    };
    let on_clear = move |_| {
        draft.set(String::new());
        tokens.clear();
        error.set(None);
    };
    let on_save_api = move |_| {
        save_api_base(&api_draft.get());
        if let Some(window) = web_sys::window() {
            let _ = window.location().reload();
        }
    };

    view! {
        <div class="admin">
            <header class="admin__bar">
                <div class="admin__bar-inner">
                    <div class="admin__brand">
                        <span class="admin__name">
                            "rusta"
                            <span class="admin__name-accent">"shop"</span>
                            " admin"
                        </span>
                        <nav class="admin__nav" aria-label="Admin">
                            <A href="/" exact=true>"Orders"</A>
                            <A href="/products">"Products"</A>
                        </nav>
                    </div>
                    <div class="admin__token">
                        <label class="visually-hidden" for="admin-api-base">"API base URL"</label>
                        <input
                            id="admin-api-base"
                            type="text"
                            class="admin__input admin__input--base"
                            placeholder="API base (/api or http://…)"
                            title=format!("sessionStorage {API_BASE_STORAGE_KEY}; empty restores default")
                            prop:value=move || api_draft.get()
                            on:input=move |ev| {
                                let input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
                                api_draft.set(input.value());
                            }
                            autocomplete="off"
                        />
                        <button type="button" class="admin__btn admin__btn--ghost" on:click=on_save_api>
                            "Save API"
                        </button>
                        <label class="visually-hidden" for="admin-token">"Admin API token"</label>
                        <input
                            id="admin-token"
                            type="password"
                            class="admin__input"
                            placeholder="Bearer token"
                            prop:value=move || draft.get()
                            on:input=move |ev| {
                                let input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
                                draft.set(input.value());
                            }
                            autocomplete="off"
                        />
                        <button type="button" class="admin__btn admin__btn--primary" on:click=on_save>
                            "Save token"
                        </button>
                        <button type="button" class="admin__btn admin__btn--ghost" on:click=on_clear>
                            "Clear"
                        </button>
                    </div>
                </div>
            </header>
            {move || error.get().map(|msg| view! {
                <p class="admin__error" role="alert">{msg}</p>
            })}
            <main class="admin__main">
                {children()}
            </main>
        </div>
    }
}
