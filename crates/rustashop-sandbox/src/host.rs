//! Wasmer WASIX host that runs the fixed Python `quote` fixture.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use shared_buffer::OwnedBuffer;
use tokio::runtime::Handle;
use virtual_fs::{AsyncReadExt, AsyncSeekExt, StaticFile};
use wasmer_package::utils::from_bytes;
use wasmer_wasix::PluggableRuntime;
use wasmer_wasix::bin_factory::BinaryPackage;
use wasmer_wasix::runners::wasi::{RuntimeOrEngine, WasiRunner};
use wasmer_wasix::runtime::module_cache::{FileSystemCache, ModuleCache, SharedCache};
use wasmer_wasix::runtime::package_loader::BuiltinPackageLoader;
use wasmer_wasix::runtime::task_manager::tokio::TokioTaskManager;

use crate::types::{Adjustment, CartSnapshot};

/// Registry URL for the pinned Python Wasmer package (webc download).
pub const PYTHON_PACKAGE_URL: &str = "https://wasmer.io/python/python@0.1.0";

const PYTHON_WEBC_CACHE_NAME: &str = "python-python-0.1.0.webc";

/// Source of the checked-in Python quote fixture.
#[must_use]
pub const fn quote_fixture_source() -> &'static str {
    include_str!("../../../extensions/fixtures/wasmer-quote/quote.py")
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
    invoke_python_quote_with_cache(cart, python_source, &wasmer_cache_root()).await
}

async fn invoke_python_quote_with_cache(
    cart: &CartSnapshot,
    python_source: &str,
    cache_root: &std::path::Path,
) -> Result<Vec<Adjustment>> {
    let webc = load_python_webc(cache_root).await?;
    let container = from_bytes(webc).context("decode Wasmer Python webc")?;
    let (runtime, tasks) = build_runtime(cache_root)?;
    let pkg = BinaryPackage::from_webc(&container, &runtime)
        .await
        .context("load BinaryPackage from webc")?;

    let cart_bytes = serde_json::to_vec(cart).context("serialize cart")?;
    let stdin = StaticFile::new(OwnedBuffer::from_bytes(cart_bytes));
    let mut stdout = virtual_fs::ArcFile::new(Box::<virtual_fs::BufferFile>::default());
    let mut stderr = virtual_fs::ArcFile::new(Box::<virtual_fs::BufferFile>::default());
    let stdout_guest = stdout.clone();
    let stderr_guest = stderr.clone();
    let runtime = Arc::new(runtime);
    let source = python_source.to_owned();

    let join = tokio::task::spawn_blocking(move || {
        let _guard = tasks.runtime_handle().enter();
        WasiRunner::new()
            .with_args(["-c", &source])
            .with_stdin(Box::new(stdin) as Box<_>)
            .with_stdout(Box::new(stdout_guest) as Box<_>)
            .with_stderr(Box::new(stderr_guest) as Box<_>)
            .run_command("python", &pkg, RuntimeOrEngine::Runtime(runtime))
    });

    let run_result = join.await.context("join wasmer python task")?;

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

    parse_guest_adjustments(
        run_result.map_err(|error| anyhow::anyhow!("{error:#}")),
        &out_buf,
        &err_buf,
    )
}

/// Interprets guest exit + stdout/stderr into adjustment JSON.
fn parse_guest_adjustments(
    run_result: Result<(), anyhow::Error>,
    out_buf: &[u8],
    err_buf: &[u8],
) -> Result<Vec<Adjustment>> {
    if let Err(error) = run_result {
        bail!(
            "Wasmer Python guest failed: {error:#}; stderr: {}; stdout: {}",
            String::from_utf8_lossy(err_buf),
            String::from_utf8_lossy(out_buf)
        );
    }

    if out_buf.is_empty() && !err_buf.is_empty() {
        bail!(
            "Python guest produced no stdout; stderr: {}",
            String::from_utf8_lossy(err_buf)
        );
    }

    serde_json::from_slice(out_buf).with_context(|| {
        format!(
            "parse adjustments JSON from guest stdout `{}` (stderr: {})",
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

async fn load_python_webc(cache_root: &std::path::Path) -> Result<bytes::Bytes> {
    load_python_webc_from(cache_root, PYTHON_PACKAGE_URL).await
}

async fn load_python_webc_from(
    cache_root: &std::path::Path,
    package_url: &str,
) -> Result<bytes::Bytes> {
    let cache_path = cache_root.join("downloads").join(PYTHON_WEBC_CACHE_NAME);
    if cache_path.is_file() {
        return Ok(std::fs::read(&cache_path)
            .with_context(|| format!("read cached webc {}", cache_path.display()))?
            .into());
    }
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

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
        assert!(bad.to_string().contains("parse adjustments"));
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
            wasmer_cache_root_from(Some(std::ffi::OsString::from("/tmp/rustashop-wasmer-test-cache"))),
            PathBuf::from("/tmp/rustashop-wasmer-test-cache")
        );
        assert!(wasmer_cache_root_from(None)
            .ends_with(std::path::Path::new(".wasmer")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn load_python_webc_maps_http_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let err = load_python_webc_from(tmp.path(), "https://httpbingo.org/status/404")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("failed with HTTP"));
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
}
