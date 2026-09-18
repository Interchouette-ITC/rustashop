//! rustashop Leptos CSR admin (track B).

#![allow(clippy::empty_enums)]
#![allow(clippy::future_not_send)]

use leptos::mount::mount_to_body;

mod api;
mod app;
mod auth;
mod money;
mod pages;
mod shell;
mod token;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(app::App);
}
