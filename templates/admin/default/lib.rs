//! Default **admin** template package: on-disk root for operator BO hosts.
//! Markup lives beside this crate (`orders_page/`, …). Kind is always admin.

use std::path::{Path, PathBuf};

/// Template surface: operator back-office (not shop).
pub const KIND: &str = "admin";

/// Absolute directory of this template (`templates/admin/default`).
#[must_use]
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Template id (folder name under `templates/admin/`).
#[must_use]
pub fn id() -> &'static str {
    "default"
}

/// Path to a component file: `{root}/{component}/{component}.{ext}`.
#[must_use]
pub fn component_file(component: &str, ext: &str) -> PathBuf {
    root().join(component).join(format!("{component}.{ext}"))
}

/// Same as [`root`], as a borrowed path (for APIs that take `&Path`).
#[must_use]
pub fn root_path() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
