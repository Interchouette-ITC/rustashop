use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::pages::{OrdersPage, ProductsPage};
use crate::shell::AdminShell;
use crate::token::TokenCtx;

/// Host root: router + admin shell.
#[component]
pub fn App() -> impl IntoView {
    TokenCtx::provide();

    view! {
        <Router>
            <AdminShell>
                <Routes fallback=|| {
                    view! { <p class="admin__tagline">"Not found"</p> }
                }>
                    <Route path=path!("/") view=OrdersPage />
                    <Route path=path!("/products") view=ProductsPage />
                </Routes>
            </AdminShell>
        </Router>
    }
}
