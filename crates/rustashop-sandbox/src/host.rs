//! Wasmer WASIX host for polyglot `quote(cart) → adjustments` guests.
//!
//! Engine plumbing comes from `serenade-sandbox`; this module owns commerce
//! package pins, fixtures, and quote/migration entrypoints.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
#[cfg(test)]
use serenade_sandbox::load_webc_from;
use serenade_sandbox::{PackageRun, decode_json, run_module, run_package, wasmer_cache_root_from};

use crate::types::{Adjustment, CartSnapshot};

/// Registry URL for the pinned Python Wasmer package (webc download).
pub const PYTHON_PACKAGE_URL: &str = "https://wasmer.io/python/python@0.1.0";

const PYTHON_WEBC_CACHE_NAME: &str = "python-python-0.1.0.webc";

/// Registry URL for the pinned `QuickJS` Wasmer package (JS quote guest).
pub const JS_PACKAGE_URL: &str = "https://wasmer.io/syrusakbary/quickjs";

const JS_WEBC_CACHE_NAME: &str = "syrusakbary-quickjs.webc";

/// Registry URL for the pinned PHP Wasmer package.
pub const PHP_PACKAGE_URL: &str = "https://wasmer.io/php/php-32";

const PHP_WEBC_CACHE_NAME: &str = "php-php-32.webc";

/// Source of the checked-in Python quote fixture.
#[must_use]
pub const fn quote_fixture_source() -> &'static str {
    include_str!("../../../extensions/fixtures/wasmer-quote/quote.py")
}

/// Source of the checked-in `QuickJS` quote fixture.
#[must_use]
pub const fn quote_js_fixture_source() -> &'static str {
    include_str!("../../../extensions/fixtures/wasmer-quote/quote.js")
}

/// PHP `-r` body for the checked-in quote fixture (opening tag stripped).
#[must_use]
pub fn quote_php_fixture_source() -> String {
    strip_php_opening_tag(include_str!(
        "../../../extensions/fixtures/wasmer-quote/quote.php"
    ))
}

/// PHP `-r` body for the cart-update migration fixture (opening tag stripped).
#[must_use]
pub fn php_migration_hook_source() -> String {
    strip_php_opening_tag(include_str!(
        "../../../extensions/fixtures/wasmer-php-migration/action_cart_update_quantity_before.php"
    ))
}

/// Path to the checked-in Rust WASI `quote` guest wasm.
#[must_use]
pub fn rust_quote_wasm_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../extensions/fixtures/wasmer-quote-rust/quote.wasm")
}

/// Runs the Python `quote` guest inside Wasmer and parses JSON adjustments.
///
/// Downloads (and caches) the Wasmer Python package on first use. No Docker.
///
/// # Errors
///
/// Returns an error when the package cannot be loaded, the guest fails, or
/// stdout is not valid adjustment JSON.
pub async fn invoke_python_quote(
    cart: &CartSnapshot,
    python_source: &str,
) -> Result<Vec<Adjustment>> {
    invoke_package_quote(
        cart,
        &wasmer_cache_root(),
        PYTHON_PACKAGE_URL,
        PYTHON_WEBC_CACHE_NAME,
        "python",
        vec!["-c".into(), python_source.to_owned()],
    )
    .await
}

/// Runs the `QuickJS` `quote` guest inside Wasmer and parses JSON adjustments.
///
/// # Errors
///
/// Returns an error when the package cannot be loaded, the guest fails, or
/// stdout is not valid adjustment JSON.
pub async fn invoke_js_quote(cart: &CartSnapshot, js_source: &str) -> Result<Vec<Adjustment>> {
    invoke_package_quote(
        cart,
        &wasmer_cache_root(),
        JS_PACKAGE_URL,
        JS_WEBC_CACHE_NAME,
        "qjs",
        vec!["--std".into(), "-e".into(), js_source.to_owned()],
    )
    .await
}

/// Runs the PHP `quote` guest inside Wasmer and parses JSON adjustments.
///
/// `php_source` is the body passed to `php -r` (no opening tag).
///
/// # Errors
///
/// Returns an error when the package cannot be loaded, the guest fails, or
/// stdout is not valid adjustment JSON.
pub async fn invoke_php_quote(cart: &CartSnapshot, php_source: &str) -> Result<Vec<Adjustment>> {
    invoke_package_quote(
        cart,
        &wasmer_cache_root(),
        PHP_PACKAGE_URL,
        PHP_WEBC_CACHE_NAME,
        "php",
        vec!["-r".into(), php_source.to_owned()],
    )
    .await
}

/// Runs the PHP migration guest for one legacy hook → domain event draft.
///
/// # Errors
///
/// Returns an error when the package cannot be loaded, the guest fails, or
/// stdout is not a valid [`crate::types::DomainEventDraft`].
pub async fn invoke_php_migration_hook(
    input: &crate::types::LegacyHookInput,
    php_source: &str,
) -> Result<crate::types::DomainEventDraft> {
    let stdin = serde_json::to_vec(input).context("serialize legacy hook input")?;
    invoke_package_json(
        &stdin,
        &wasmer_cache_root(),
        PHP_PACKAGE_URL,
        PHP_WEBC_CACHE_NAME,
        "php",
        vec!["-r".into(), php_source.to_owned()],
    )
    .await
}

fn strip_php_opening_tag(source: &str) -> String {
    let trimmed = source.trim_start();
    trimmed
        .strip_prefix("<?php")
        .map_or(trimmed, str::trim_start)
        .to_owned()
}

/// Runs the checked-in Rust WASI `quote` guest and parses JSON adjustments.
///
/// # Errors
///
/// Returns an error when the wasm cannot be loaded, the guest fails, or stdout
/// is not valid adjustment JSON.
pub async fn invoke_rust_wasi_quote(cart: &CartSnapshot) -> Result<Vec<Adjustment>> {
    invoke_rust_wasi_quote_at(cart, &rust_quote_wasm_path()).await
}

async fn invoke_rust_wasi_quote_at(
    cart: &CartSnapshot,
    wasm_path: &Path,
) -> Result<Vec<Adjustment>> {
    let wasm_bytes = std::fs::read(wasm_path)
        .with_context(|| format!("read Rust quote wasm {}", wasm_path.display()))?;
    let cart_bytes = serde_json::to_vec(cart).context("serialize cart")?;
    let output = run_module("quote", &wasm_bytes, &cart_bytes, &wasmer_cache_root()).await?;
    decode_json(&output)
}

async fn invoke_package_quote(
    cart: &CartSnapshot,
    cache_root: &Path,
    package_url: &str,
    cache_name: &str,
    command: &str,
    args: Vec<String>,
) -> Result<Vec<Adjustment>> {
    let cart_bytes = serde_json::to_vec(cart).context("serialize cart")?;
    invoke_package_json(
        &cart_bytes,
        cache_root,
        package_url,
        cache_name,
        command,
        args,
    )
    .await
}

async fn invoke_package_json<T: serde::de::DeserializeOwned>(
    stdin_bytes: &[u8],
    cache_root: &Path,
    package_url: &str,
    cache_name: &str,
    command: &str,
    args: Vec<String>,
) -> Result<T> {
    let output = run_package(PackageRun {
        stdin_bytes,
        cache_root,
        package_url,
        cache_name,
        command,
        args,
    })
    .await?;
    decode_json(&output)
}

fn wasmer_cache_root() -> PathBuf {
    wasmer_cache_root_with(
        std::env::var_os("RUSTASHOP_WASMER_CACHE")
            .or_else(|| std::env::var_os("SERENADE_WASMER_CACHE")),
    )
}

fn wasmer_cache_root_with(override_path: Option<std::ffi::OsString>) -> PathBuf {
    if let Some(path) = override_path {
        return wasmer_cache_root_from(Some(path));
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.wasmer")
}

#[cfg(test)]
async fn load_python_webc(cache_root: &Path) -> Result<bytes::Bytes> {
    load_webc_from(cache_root, PYTHON_PACKAGE_URL, PYTHON_WEBC_CACHE_NAME).await
}

#[cfg(test)]
mod host_tests {
    use super::*;
    use crate::types::{CartLine, Money};
    use serenade_sandbox::{GuestOutput, decode_json as sandbox_decode_json};

    fn sample_cart() -> CartSnapshot {
        CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "TEE".into(),
                quantity: 1,
                unit_price: Money {
                    amount_minor: 100,
                    currency: "EUR".into(),
                },
            }],
        }
    }

    #[test]
    fn fixture_source_includes_quote_entrypoint() {
        let source = quote_fixture_source();
        assert!(source.contains("def quote("));
        assert!(source.contains("json.dump"));
        assert!(quote_js_fixture_source().contains("function quote("));
        let php = quote_php_fixture_source();
        assert!(php.contains("function quote("));
        assert!(!php.trim_start().starts_with("<?php"));
    }

    #[test]
    fn decode_json_maps_failure_and_bad_json() {
        let failed = GuestOutput {
            stdout: Vec::new(),
            stderr: b"trace".to_vec(),
            success: false,
        };
        let err = sandbox_decode_json::<Vec<Adjustment>>(&failed).unwrap_err();
        assert!(err.to_string().contains("guest failed"));

        let empty = GuestOutput {
            stdout: Vec::new(),
            stderr: b"oops".to_vec(),
            success: true,
        };
        let empty_err = sandbox_decode_json::<Vec<Adjustment>>(&empty).unwrap_err();
        assert!(empty_err.to_string().contains("no stdout"));

        let bad = GuestOutput {
            stdout: b"not-json".to_vec(),
            stderr: Vec::new(),
            success: true,
        };
        let bad_err = sandbox_decode_json::<Vec<Adjustment>>(&bad).unwrap_err();
        assert!(bad_err.to_string().contains("parse guest JSON"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn python_guest_exit_failure_is_mapped() {
        let err = invoke_python_quote(
            &sample_cart(),
            "import sys\nsys.stderr.write('nope')\nsys.exit(1)\n",
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("guest failed"));
    }

    #[test]
    fn wasmer_cache_root_reads_override() {
        assert_eq!(
            wasmer_cache_root_with(Some(std::ffi::OsString::from(
                "/tmp/rustashop-wasmer-test-cache"
            ))),
            PathBuf::from("/tmp/rustashop-wasmer-test-cache")
        );
        assert!(wasmer_cache_root_with(None).ends_with(Path::new(".wasmer")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn load_python_webc_maps_http_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let err = load_webc_from(
            tmp.path(),
            "https://example.com/no-such-webc",
            "missing.webc",
        )
        .await
        .unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("failed with HTTP") || message.contains("too small"),
            "unexpected error: {message}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn load_python_webc_downloads_when_cache_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let body = load_python_webc(tmp.path()).await.expect("download webc");
        assert!(body.len() >= 1024, "payload size");
        let cached = tmp.path().join("downloads").join(PYTHON_WEBC_CACHE_NAME);
        assert!(cached.is_file());
        let again = load_python_webc(tmp.path()).await.expect("read cache");
        assert_eq!(body.len(), again.len());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rust_wasi_quote_volume_discount() {
        let cart = CartSnapshot {
            currency: "EUR".into(),
            lines: vec![CartLine {
                sku: "HOODIE-M".into(),
                quantity: 2,
                unit_price: Money {
                    amount_minor: 5000,
                    currency: "EUR".into(),
                },
            }],
        };
        let raw = invoke_rust_wasi_quote(&cart)
            .await
            .expect("rust wasi quote");
        let applied = crate::apply_validated_adjustments(&cart, raw).expect("validate");
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].label, "volume-discount");
        assert_eq!(applied[0].amount_minor, -1000);
    }
}
