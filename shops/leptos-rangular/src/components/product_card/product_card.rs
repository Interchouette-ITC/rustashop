use leptos::prelude::*;
use rangular_aot::HostCell;
use rangular_host::{Host, HostError, Value};

include!(concat!(env!("OUT_DIR"), "/rangular/product_card_view.rs"));

/// One catalog tile from the shared template (`templates/<id>/product_card`).
#[component]
pub fn ProductCardPanel(
    name: String,
    slug: String,
    description: Option<String>,
    href: String,
) -> impl IntoView {
    product_card_view(HostCell::new(ProductCardHost {
        name,
        slug,
        description,
        detail_href: href,
    }))
}

struct ProductCardHost {
    name: String,
    slug: String,
    description: Option<String>,
    detail_href: String,
}

impl Host for ProductCardHost {
    fn get(&self, name: &str) -> Option<Value> {
        match name {
            "name" => Some(Value::Str(self.name.clone())),
            "slug" => Some(Value::Str(self.slug.clone())),
            "description" => Some(Value::Str(self.description.clone().unwrap_or_default())),
            "detailHref" => Some(Value::Str(self.detail_href.clone())),
            _ => None,
        }
    }

    fn call(&mut self, _: &str, _: &[Value]) -> Result<Value, HostError> {
        Ok(Value::Unit)
    }
}
