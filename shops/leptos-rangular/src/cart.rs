//! Browser cart session (same `rs.cartId` key as the Angular shop).

use std::cell::RefCell;

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{MessageEvent, WebSocket};

use crate::api::{self, Cart};
use crate::cart_ws::{cart_ws_url, parse_cart_updated_message};

const CART_ID_KEY: &str = "rs.cartId";

thread_local! {
    static CART_WS: RefCell<WsState> = RefCell::new(WsState::default());
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn read_cart_id() -> Option<String> {
    local_storage()?.get_item(CART_ID_KEY).ok().flatten()
}

fn write_cart_id(id: &str) {
    if let Some(store) = local_storage() {
        let _ = store.set_item(CART_ID_KEY, id);
    }
}

fn clear_cart_id() {
    if let Some(store) = local_storage() {
        let _ = store.remove_item(CART_ID_KEY);
    }
}

#[derive(Default)]
struct WsState {
    socket: Option<WebSocket>,
    cart_id: Option<String>,
}

/// Browser cart session context (shared via Leptos context).
#[derive(Clone, Copy)]
pub struct CartCtx {
    /// Live cart snapshot (HTTP + WebSocket).
    pub cart: RwSignal<Option<Cart>>,
    /// True while an HTTP cart mutation is in flight.
    pub busy: RwSignal<bool>,
    /// Last cart HTTP error message.
    pub error: RwSignal<Option<String>>,
}

impl CartCtx {
    /// Creates and provides `CartCtx` for the shop tree.
    pub fn provide() -> Self {
        let ctx = Self {
            cart: RwSignal::new(None),
            busy: RwSignal::new(false),
            error: RwSignal::new(None),
        };
        provide_context(ctx);
        ctx
    }

    /// Sum of line quantities for the nav badge.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.cart.get().map_or(0, |cart| {
            cart.lines
                .iter()
                .map(|line| usize::try_from(line.quantity.max(0)).unwrap_or(0))
                .sum()
        })
    }

    /// Loads the stored cart, or creates one when missing.
    pub async fn ensure_cart(&self) -> Result<Cart, String> {
        if let Some(id) = read_cart_id() {
            match api::get_cart(&id).await {
                Ok(cart) if cart.status == "open" => {
                    self.apply_cart(Some(&cart));
                    self.error.set(None);
                    return Ok(cart);
                }
                _ => clear_cart_id(),
            }
        }
        let cart = api::create_cart().await?;
        self.apply_cart(Some(&cart));
        self.error.set(None);
        Ok(cart)
    }

    /// Refreshes the current cart from the API when an id is known.
    pub async fn refresh(&self) -> Result<(), String> {
        let id = self
            .cart
            .get_untracked()
            .map(|cart| cart.id)
            .or_else(read_cart_id);
        let Some(id) = id else {
            self.apply_cart(None);
            return Ok(());
        };
        self.busy.set(true);
        let result = api::get_cart(&id).await;
        self.busy.set(false);
        match result {
            Ok(cart) => {
                self.apply_cart(Some(&cart));
                self.error.set(None);
                Ok(())
            }
            Err(err) => {
                self.error.set(Some(err.clone()));
                Err(err)
            }
        }
    }

    /// Adds a line via HTTP; WebSocket keeps peers in sync.
    pub async fn add_line(&self, variant_id: &str, quantity: i32) -> Result<Cart, String> {
        self.busy.set(true);
        self.error.set(None);
        let result: Result<Cart, String> = async {
            let cart = self.ensure_cart().await?;
            let updated = api::add_cart_line(&cart.id, variant_id, quantity).await?;
            self.apply_cart(Some(&updated));
            Ok(updated)
        }
        .await;
        self.busy.set(false);
        if let Err(err) = &result {
            self.error.set(Some(err.clone()));
        }
        result
    }

    /// Clears the local cart session after a successful checkout.
    pub fn clear_session(&self) {
        self.apply_cart(None);
        self.error.set(None);
        self.busy.set(false);
    }

    fn apply_cart(&self, cart: Option<&Cart>) {
        if let Some(cart) = cart {
            write_cart_id(&cart.id);
            self.cart.set(Some(cart.clone()));
            sync_socket(Some(cart), self.cart);
        } else {
            clear_cart_id();
            self.cart.set(None);
            sync_socket(None, self.cart);
        }
    }
}

fn sync_socket(cart: Option<&Cart>, cart_signal: RwSignal<Option<Cart>>) {
    let Some(cart) = cart else {
        close_socket();
        return;
    };
    if cart.status != "open" || cart.token.is_empty() {
        close_socket();
        return;
    }
    let already = CART_WS.with(|cell| {
        let state = cell.borrow();
        state.cart_id.as_deref() == Some(cart.id.as_str()) && state.socket.is_some()
    });
    if already {
        return;
    }
    close_socket();
    open_socket(cart, cart_signal);
}

fn open_socket(cart: &Cart, cart_signal: RwSignal<Option<Cart>>) {
    let Ok(url) = cart_ws_url(&crate::api::api_base(), &cart.id, &cart.token) else {
        return;
    };
    let Ok(socket) = WebSocket::new(&url) else {
        return;
    };
    let onmessage = Closure::wrap(Box::new(move |event: MessageEvent| {
        let Some(text) = event.data().as_string() else {
            return;
        };
        let Some(next) = parse_cart_updated_message(&text) else {
            return;
        };
        write_cart_id(&next.id);
        cart_signal.set(Some(next));
    }) as Box<dyn FnMut(MessageEvent)>);
    socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    // Leak the closure for the socket lifetime (CSR; reconnect replaces the socket).
    onmessage.forget();
    CART_WS.with(|cell| {
        let mut state = cell.borrow_mut();
        state.cart_id = Some(cart.id.clone());
        state.socket = Some(socket);
    });
}

fn close_socket() {
    CART_WS.with(|cell| {
        let mut state = cell.borrow_mut();
        if let Some(socket) = state.socket.take() {
            socket.set_onmessage(None);
            let _ = socket.close();
        }
        state.cart_id = None;
    });
}

/// Returns the provided [`CartCtx`].
///
/// # Panics
///
/// Panics when `CartCtx` was not provided in the component tree.
#[must_use]
pub fn use_cart() -> CartCtx {
    use_context::<CartCtx>().expect("CartCtx provided")
}
