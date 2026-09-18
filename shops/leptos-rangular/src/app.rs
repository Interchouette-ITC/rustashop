use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use crate::cart::CartCtx;
use crate::order::OrderCtx;
use crate::pages::{CartPage, CatalogPage, CheckoutPage, ProductDetailPage};
use crate::shell::ShopShell;

/// Host root: router + shell (Angular analog: `rs-root` → shell → outlet).
#[component]
pub fn App() -> impl IntoView {
    CartCtx::provide();
    OrderCtx::provide();

    view! {
        <Router>
            <ShopShell>
                <Routes fallback=|| {
                    view! { <p class="shop__tagline">"Not found"</p> }
                }>
                    <Route path=path!("/") view=CatalogPage />
                    <Route path=path!("/products/:id") view=ProductDetailPage />
                    <Route path=path!("/cart") view=CartPage />
                    <Route path=path!("/checkout/:orderId") view=CheckoutPage />
                </Routes>
            </ShopShell>
        </Router>
    }
}
