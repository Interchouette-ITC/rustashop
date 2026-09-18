use leptos::prelude::*;

use crate::api::{self, Product};
use crate::components::ProductCardPanel;

/// Catalog from `GET /v1/products` (shared API with Angular).
#[component]
pub fn CatalogPage() -> impl IntoView {
    let products = RwSignal::new(Vec::<Product>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        leptos::task::spawn_local(async move {
            loading.set(true);
            error.set(None);
            match api::list_products().await {
                Ok(body) => {
                    products.set(body.items.into_iter().filter(Product::is_listed).collect());
                }
                Err(err) => {
                    products.set(Vec::new());
                    error.set(Some(err));
                }
            }
            loading.set(false);
        });
    });

    view! {
        <section class="shop" aria-labelledby="catalog-heading">
            <header class="shop__hero">
                <h1 id="catalog-heading" class="shop__title">"Catalog"</h1>
                <p class="shop__tagline">"Seeded demo products from the Commerce API."</p>
            </header>
            {move || catalog_body(error.get(), loading.get(), products.get())}
        </section>
    }
}

fn catalog_body(error: Option<String>, loading: bool, products: Vec<Product>) -> AnyView {
    if let Some(message) = error {
        return view! { <p class="shop__error" role="alert">{message}</p> }.into_any();
    }
    if loading {
        return view! { <p class="shop__tagline">"Loading products…"</p> }.into_any();
    }
    if products.is_empty() {
        return view! {
            <p class="shop__tagline">
                "No products yet. Migrate and seed the database, then refresh."
            </p>
        }
        .into_any();
    }
    view! {
        <div class="shop__catalog">
            <For
                each=move || products.clone()
                key=|product| product.id.clone()
                children=move |product| {
                    let href = format!("/products/{}", product.id);
                    view! {
                        <ProductCardPanel
                            name=product.name
                            slug=product.slug
                            description=product.description
                            detail_href=href
                        />
                    }
                }
            />
        </div>
    }
    .into_any()
}
