//! rustashop Leptos CSR shop (track B).

// Leptos `#[component]` TypedBuilder emits empty marker enums; nursery lint.
#![allow(clippy::empty_enums)]
// gloo-net / browser futures are not Send; CSR only.
#![allow(clippy::future_not_send)]

use leptos::mount::mount_to_body;

mod api;
mod app;
mod cart;
mod cart_ws;
mod checkout;
mod components;
mod order;
mod pages;
mod shell;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(app::App);
}
