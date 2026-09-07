//! Wasmer WASIX host for polyglot `quote(cart) → adjustments` guests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use shared_buffer::OwnedBuffer;
use tokio::runtime::Handle;
use virtual_fs::{AsyncReadExt, AsyncSeekExt, StaticFile};
use wasmer::Module;
use wasmer_package::utils::from_bytes;
use wasmer_types::ModuleHash;
use wasmer_wasix::PluggableRuntime;
use wasmer_wasix::Runtime;
use wasmer_wasix::bin_factory::BinaryPackage;
use wasmer_wasix::runners::wasi::{RuntimeOrEngine, WasiRunner};
use wasmer_wasix::runtime::module_cache::{FileSystemCache, ModuleCache, SharedCache};
use wasmer_wasix::runtime::package_loader::BuiltinPackageLoader;
use wasmer_wasix::runtime::task_manager::tokio::TokioTaskManager;

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
    let (out_buf, err_buf, run_result) =
        run_wasi_module_stdio("quote", &wasm_bytes, &cart_bytes, &wasmer_cache_root()).await?;
    parse_guest_json(
        run_result.map_err(|error| anyhow::anyhow!("{error:#}")),
        &out_buf,
        &err_buf,
    )
}

async fn invoke_package_quote(
    cart: &CartSnapshot,
    cache_root: &std::path::Path,
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
    cache_root: &std::path::Path,
    package_url: &str,
    cache_name: &str,
    command: &str,
    args: Vec<String>,
) -> Result<T> {
    let webc = load_webc_from(cache_root, package_url, cache_name).await?;
    let container =
        from_bytes(webc).with_context(|| format!("decode Wasmer webc {package_url}"))?;
    let (runtime, tasks) = build_runtime(cache_root)?;
    let pkg = BinaryPackage::from_webc(&container, &runtime)
        .await
        .context("load BinaryPackage from webc")?;

    let stdin = StaticFile::new(OwnedBuffer::from_bytes(stdin_bytes.to_vec()));
    let mut stdout = virtual_fs::ArcFile::new(Box::<virtual_fs::BufferFile>::default());
    let mut stderr = virtual_fs::ArcFile::new(Box::<virtual_fs::BufferFile>::default());
    let stdout_guest = stdout.clone();
    let stderr_guest = stderr.clone();
    let runtime = Arc::new(runtime);
    let command = command.to_owned();

    let join = tokio::task::spawn_blocking(move || {
        let _guard = tasks.runtime_handle().enter();
        WasiRunner::new()
            .with_args(args)
            .with_stdin(Box::new(stdin) as Box<_>)
            .with_stdout(Box::new(stdout_guest) as Box<_>)
            .with_stderr(Box::new(stderr_guest) as Box<_>)
            .run_command(&command, &pkg, RuntimeOrEngine::Runtime(runtime))
    });

    let run_result = join.await.context("join wasmer package task")?;
    stdout.rewind().await.context("rewind stdout")?;
    stderr.rewind().await.context("rewind stderr")?;
    let mut out_buf = Vec::new();
    let mut err_buf = Vec::new();
    stdout
        .read_to_end(&mut out_buf)
        .await
        .context("read stdout")?;
    stderr
        .read_to_end(&mut err_buf)
        .await
        .context("read stderr")?;
    parse_guest_json(
        run_result.map_err(|error| anyhow::anyhow!("{error:#}")),
        &out_buf,
        &err_buf,
    )
}

async fn run_wasi_module_stdio(
    program_name: &str,
    wasm_bytes: &[u8],
    stdin_bytes: &[u8],
    cache_root: &Path,
) -> Result<(Vec<u8>, Vec<u8>, Result<(), anyhow::Error>)> {
    let (runtime, tasks) = build_runtime(cache_root)?;
    let engine = runtime.engine();
    let module = Module::new(&engine, wasm_bytes).context("compile WASI quote module")?;
    let module_hash = ModuleHash::new(wasm_bytes);

    let stdin = StaticFile::new(OwnedBuffer::from_bytes(stdin_bytes.to_vec()));
    let mut stdout = virtual_fs::ArcFile::new(Box::<virtual_fs::BufferFile>::default());
    let mut stderr = virtual_fs::ArcFile::new(Box::<virtual_fs::BufferFile>::default());
    let stdout_guest = stdout.clone();
    let stderr_guest = stderr.clone();
    let program = program_name.to_owned();

    let join = tokio::task::spawn_blocking(move || {
        let _guard = tasks.runtime_handle().enter();
        WasiRunner::new()
            .with_stdin(Box::new(stdin) as Box<_>)
            .with_stdout(Box::new(stdout_guest) as Box<_>)
            .with_stderr(Box::new(stderr_guest) as Box<_>)
            .run_wasm(
                RuntimeOrEngine::Engine(engine),
                &program,
                module,
                module_hash,
            )
    });

    let run_result = join
        .await
        .context("join wasmer wasi task")?
        .map_err(|error| anyhow::anyhow!("{error:#}"));
    stdout.rewind().await.context("rewind stdout")?;
    stderr.rewind().await.context("rewind stderr")?;
    let mut out_buf = Vec::new();
    let mut err_buf = Vec::new();
    stdout
        .read_to_end(&mut out_buf)
        .await
        .context("read stdout")?;
    stderr
        .read_to_end(&mut err_buf)
        .await
        .context("read stderr")?;
    Ok((out_buf, err_buf, run_result))
}

/// Interprets guest exit + stdout/stderr into adjustment JSON.
#[cfg(test)]
fn parse_guest_adjustments(
    run_result: Result<(), anyhow::Error>,
    out_buf: &[u8],
    err_buf: &[u8],
) -> Result<Vec<Adjustment>> {
    parse_guest_json(run_result, out_buf, err_buf)
}

fn parse_guest_json<T: serde::de::DeserializeOwned>(
    run_result: Result<(), anyhow::Error>,
    out_buf: &[u8],
    err_buf: &[u8],
) -> Result<T> {
    if let Err(error) = run_result {
        bail!(
            "Wasmer guest failed: {error:#}; stderr: {}; stdout: {}",
            String::from_utf8_lossy(err_buf),
            String::from_utf8_lossy(out_buf)
        );
    }

    if out_buf.is_empty() && !err_buf.is_empty() {
        bail!(
            "guest produced no stdout; stderr: {}",
            String::from_utf8_lossy(err_buf)
        );
    }

    serde_json::from_slice(out_buf).with_context(|| {
        format!(
            "parse guest JSON from stdout `{}` (stderr: {})",
            String::from_utf8_lossy(out_buf),
            String::from_utf8_lossy(err_buf)
        )
    })
}

fn build_runtime(
    cache_root: &std::path::Path,
) -> Result<(PluggableRuntime, Arc<TokioTaskManager>)> {
    let tasks = Arc::new(TokioTaskManager::new(Handle::current()));
    let mut runtime = PluggableRuntime::new(Arc::clone(&tasks) as Arc<_>);
    let compiled = cache_root.join("compiled");
    let packages = cache_root.join("packages");
    std::fs::create_dir_all(&compiled).with_context(|| format!("create {}", compiled.display()))?;
    std::fs::create_dir_all(&packages).with_context(|| format!("create {}", packages.display()))?;
    let module_cache =
        SharedCache::default().with_fallback(FileSystemCache::new(compiled, Arc::clone(&tasks)));
    runtime
        .set_module_cache(module_cache)
        .set_package_loader(BuiltinPackageLoader::new().with_cache_dir(packages));
    Ok((runtime, tasks))
}

fn wasmer_cache_root() -> PathBuf {
    wasmer_cache_root_from(std::env::var_os("RUSTASHOP_WASMER_CACHE"))
}

fn wasmer_cache_root_from(override_path: Option<std::ffi::OsString>) -> PathBuf {
    if let Some(path) = override_path {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.wasmer")
}

/// Rejects obviously truncated package downloads.
fn ensure_webc_payload(body: &[u8]) -> Result<()> {
    if body.len() < 1024 {
        bail!(
            "Wasmer Python download looks too small ({} bytes); check Accept header",
            body.len()
        );
    }
    Ok(())
}

#[cfg(test)]
async fn load_python_webc(cache_root: &std::path::Path) -> Result<bytes::Bytes> {
    load_webc_from(cache_root, PYTHON_PACKAGE_URL, PYTHON_WEBC_CACHE_NAME).await
}

async fn load_webc_from(
    cache_root: &std::path::Path,
    package_url: &str,
    cache_name: &str,
) -> Result<bytes::Bytes> {
    let downloads = cache_root.join("downloads");
    let cache_path = downloads.join(cache_name);
    if cache_path.is_file() {
        return Ok(std::fs::read(&cache_path)
            .with_context(|| format!("read cached webc {}", cache_path.display()))?
            .into());
    }
    std::fs::create_dir_all(&downloads)
        .with_context(|| format!("create {}", downloads.display()))?;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(300))
        .build()
        .context("build HTTP client for Wasmer package download")?;
    let response = client
        .get(package_url)
        .header("Accept", "application/webc")
        .send()
        .await
        .with_context(|| format!("GET {package_url}"))?;
    if !response.status().is_success() {
        bail!(
            "download {package_url} failed with HTTP {}",
            response.status()
        );
    }
    let body = response
        .bytes()
        .await
        .context("read Wasmer Python webc body")?;
    ensure_webc_payload(&body)?;
    std::fs::write(&cache_path, &body)
        .with_context(|| format!("cache webc at {}", cache_path.display()))?;
    Ok(body)
}

#[cfg(test)]
mod host_tests {
    use super::*;
    use crate::types::{CartLine, Money};

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
    fn ensure_webc_payload_rejects_tiny_bodies() {
        assert!(ensure_webc_payload(&[0_u8; 10]).is_err());
        assert!(ensure_webc_payload(&[0_u8; 2048]).is_ok());
    }

    #[test]
    fn parse_guest_adjustments_maps_failure_and_bad_json() {
        let err = parse_guest_adjustments(Err(anyhow::anyhow!("boom")), b"", b"trace").unwrap_err();
        assert!(err.to_string().contains("guest failed"));

        let empty = parse_guest_adjustments(Ok(()), b"", b"oops").unwrap_err();
        assert!(empty.to_string().contains("no stdout"));

        let bad = parse_guest_adjustments(Ok(()), b"not-json", b"").unwrap_err();
        assert!(bad.to_string().contains("parse guest JSON"));
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
            wasmer_cache_root_from(Some(std::ffi::OsString::from(
                "/tmp/rustashop-wasmer-test-cache"
            ))),
            PathBuf::from("/tmp/rustashop-wasmer-test-cache")
        );
        assert!(wasmer_cache_root_from(None).ends_with(std::path::Path::new(".wasmer")));
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
        ensure_webc_payload(&body).expect("payload size");
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
