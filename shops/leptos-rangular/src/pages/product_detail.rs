use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::api::{self, ProductDetail, ProductVariant};
use crate::cart::use_cart;

struct DetailViewModel {
    error: Option<String>,
    loading: bool,
    product: Option<ProductDetail>,
    notice: Option<String>,
    selected_variant: RwSignal<String>,
    quantity: RwSignal<i32>,
    busy: RwSignal<bool>,
    on_add: Callback<()>,
}

struct AddFormModel {
    variants: Vec<ProductVariant>,
    selected_variant: RwSignal<String>,
    quantity: RwSignal<i32>,
    busy: RwSignal<bool>,
    on_add: Callback<()>,
}

/// Product detail + add-to-cart (same cart API as Angular).
#[component]
pub fn ProductDetailPage() -> impl IntoView {
    let params = use_params_map();
    let cart = use_cart();
    let product = RwSignal::new(None::<ProductDetail>);
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    let notice = RwSignal::new(None::<String>);
    let selected_variant = RwSignal::new(String::new());
    let quantity = RwSignal::new(1_i32);

    Effect::new(move |_| {
        let id = params.read().get("id").unwrap_or_default();
        if id.is_empty() {
            return;
        }
        leptos::task::spawn_local(async move {
            loading.set(true);
            error.set(None);
            notice.set(None);
            match api::get_product(&id).await {
                Ok(detail) => {
                    selected_variant.set(first_variant_id(&detail));
                    product.set(Some(detail));
                }
                Err(err) => {
                    product.set(None);
                    error.set(Some(err));
                }
            }
            loading.set(false);
        });
    });

    let on_add = Callback::new(move |()| {
        let variant_id = selected_variant.get();
        let qty = quantity.get().max(1);
        if variant_id.is_empty() {
            return;
        }
        leptos::task::spawn_local(async move {
            match cart.add_line(&variant_id, qty).await {
                Ok(_) => notice.set(Some("Added to cart.".into())),
                Err(err) => error.set(Some(err)),
            }
        });
    });

    view! {
        <section class="shop">
            <p class="shop__tagline">
                <A href="/">"Catalog"</A>
            </p>
            {move || {
                detail_body(DetailViewModel {
                    error: error.get(),
                    loading: loading.get(),
                    product: product.get(),
                    notice: notice.get(),
                    selected_variant,
                    quantity,
                    busy: cart.busy,
                    on_add,
                })
            }}
        </section>
    }
}

fn first_variant_id(detail: &ProductDetail) -> String {
    detail
        .variants
        .first()
        .map_or_else(String::new, |variant| variant.id.clone())
}

fn variant_label(variant: &ProductVariant) -> String {
    let name = variant.name.clone().unwrap_or_else(|| variant.sku.clone());
    format!(
        "{name} - {} (stock {}) [{}]",
        variant.price.display(),
        variant.stock_quantity,
        variant.parent_product_id()
    )
}

fn detail_body(model: DetailViewModel) -> AnyView {
    if let Some(message) = model.error {
        return view! { <p class="shop__error" role="alert">{message}</p> }.into_any();
    }
    if model.loading {
        return view! { <p class="shop__tagline">"Loading product…"</p> }.into_any();
    }
    let Some(detail) = model.product else {
        return view! { <p class="shop__tagline">"Product not found."</p> }.into_any();
    };
    let listed = detail.is_listed();
    let variants = detail.variants.clone();
    view! {
        <article>
            <header class="shop__hero">
                <h1 class="shop__title">{detail.name.clone()}</h1>
                <p class="shop__tagline">{format!("{} · {}", detail.slug, detail.id)}</p>
                {(!listed).then(|| view! {
                    <p class="shop__tagline" role="status">"This product is not listed in the catalog."</p>
                })}
            </header>
            {detail.description.map_or_else(
                || ().into_any(),
                |text| view! { <p class="shop__tagline">{text}</p> }.into_any(),
            )}
            {model.notice.map_or_else(
                || ().into_any(),
                |ok| {
                    view! {
                        <p class="shop__notice" role="status">
                            {ok}
                            " "
                            <A href="/cart">"View cart"</A>
                        </p>
                    }
                    .into_any()
                },
            )}
            {if variants.is_empty() {
                view! { <p class="shop__tagline">"No variants available."</p> }.into_any()
            } else {
                add_form(AddFormModel {
                    variants,
                    selected_variant: model.selected_variant,
                    quantity: model.quantity,
                    busy: model.busy,
                    on_add: model.on_add,
                })
            }}
        </article>
    }
    .into_any()
}

fn add_form(model: AddFormModel) -> AnyView {
    let selected_variant = model.selected_variant;
    let quantity = model.quantity;
    let busy = model.busy;
    let on_add = model.on_add;
    let variants = model.variants;
    view! {
        <div class="shop__form">
            <label>
                "Variant"
                <select
                    prop:value=move || selected_variant.get()
                    on:change=move |ev| selected_variant.set(event_target_value(&ev))
                >
                    <For
                        each=move || variants.clone()
                        key=|variant| variant.id.clone()
                        children=move |variant| {
                            let label = variant_label(&variant);
                            let id = variant.id;
                            view! { <option value=id>{label}</option> }
                        }
                    />
                </select>
            </label>
            <label>
                "Quantity"
                <input
                    type="number"
                    min="1"
                    prop:value=move || quantity.get().to_string()
                    on:input=move |ev| {
                        if let Ok(value) = event_target_value(&ev).parse::<i32>() {
                            quantity.set(value.max(1));
                        }
                    }
                />
            </label>
            <button
                type="button"
                on:click=move |_| on_add.run(())
                disabled=move || busy.get()
            >
                {move || if busy.get() { "Adding…" } else { "Add to cart" }}
            </button>
        </div>
    }
    .into_any()
}
