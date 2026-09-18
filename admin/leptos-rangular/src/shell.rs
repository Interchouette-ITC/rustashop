use leptos::prelude::*;
use leptos_router::components::A;
use wasm_bindgen::JsCast;

use crate::auth::validate_admin_token;
use crate::token::use_token;

/// Admin chrome: brand, nav, bearer token bar (Angular `admin_shell` analog).
#[component]
pub fn AdminShell(children: Children) -> impl IntoView {
    let tokens = use_token();
    let draft = RwSignal::new(tokens.token.get_untracked());
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
                            <A href="/" attr:class="">"Orders"</A>
                            <A href="/products" attr:class="">"Products"</A>
                        </nav>
                    </div>
                    <div class="admin__token">
                        <label class="visually-hidden" for="admin-token">"Admin API token"</label>
                        <input
                            id="admin-token"
                            type="password"
                            class="form-control form-control-sm"
                            placeholder="Bearer token"
                            prop:value=move || draft.get()
                            on:input=move |ev| {
                                let input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
                                draft.set(input.value());
                            }
                            autocomplete="off"
                        />
                        <button type="button" class="btn btn-sm btn-primary" on:click=on_save>
                            "Save token"
                        </button>
                        <button type="button" class="btn btn-sm btn-outline-light" on:click=on_clear>
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
