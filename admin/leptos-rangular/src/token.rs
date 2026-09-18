//! Session-scoped admin bearer (`sessionStorage`; never committed).

use leptos::prelude::*;

const STORAGE_KEY: &str = "rs.adminApiToken";

/// Provides the admin bearer token signal for the admin shell.
#[derive(Clone, Copy)]
pub struct TokenCtx {
    /// Current bearer (empty when unset).
    pub token: RwSignal<String>,
}

impl TokenCtx {
    /// Installs context and hydrates from sessionStorage.
    pub fn provide() {
        let token = RwSignal::new(read_stored_token());
        provide_context(Self { token });
    }

    /// Saves a validated token to signal + sessionStorage.
    pub fn save(self, value: &str) {
        self.token.set(value.to_owned());
        if let Some(storage) = window_storage() {
            let _ = storage.set_item(STORAGE_KEY, value);
        }
    }

    /// Clears the token from signal + sessionStorage.
    pub fn clear(self) {
        self.token.set(String::new());
        if let Some(storage) = window_storage() {
            let _ = storage.remove_item(STORAGE_KEY);
        }
    }

    /// Whether a non-empty token is present.
    #[must_use]
    pub fn has_token(self) -> bool {
        !self.token.get().is_empty()
    }
}

/// Reads [`TokenCtx`] from context.
///
/// # Panics
///
/// Panics when called outside [`TokenCtx::provide`].
#[must_use]
pub fn use_token() -> TokenCtx {
    expect_context::<TokenCtx>()
}

fn read_stored_token() -> String {
    window_storage()
        .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

fn window_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok().flatten()
}
