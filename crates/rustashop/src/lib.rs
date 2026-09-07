//! Application kernel package for rustashop.
//!
//! Boots a Serenade [`App`] with [`FrameworkBundle`] and
//! [`RustashopBundle`], then builds the DI container from `config/packages`.

mod bundle;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serenade_bundle::{BundleError, FrameworkBundle, FrameworkExtension, build_container};
use serenade_config::Config;
use serenade_di::Container;
use serenade_kernel::{App, Application, Environment, KernelPhase};

pub use bundle::{RUSTASHOP_BUNDLE, RustashopBundle, RustashopExtension};

/// Diagnostics marker before [`boot_kernel`] succeeds in this process.
pub const SERENADE_KERNEL_PENDING: &str = "serenade-pending";

/// Diagnostics marker after a successful [`boot_kernel`] in this process.
pub const SERENADE_KERNEL_BOOTED: &str = "serenade";

static KERNEL_STATUS: OnceLock<&'static str> = OnceLock::new();

/// Relative packages directory under the shop root.
pub const PACKAGES_DIR: &str = "config/packages";

/// Environment variable for `APP_ENV` (Serenade / Symfony habit).
pub const APP_ENV: &str = "APP_ENV";

/// Booted Serenade application plus compiled DI container.
pub struct RustashopKernel {
    app: App,
    config: Config,
    container: Container,
}

impl RustashopKernel {
    /// Active Serenade environment.
    #[must_use]
    pub fn environment(&self) -> &Environment {
        self.app.kernel().environment()
    }

    /// Registered bundle names in dependency order.
    #[must_use]
    pub fn bundle_names(&self) -> Vec<&'static str> {
        self.app.kernel().bundle_names()
    }

    /// Kernel lifecycle phase.
    #[must_use]
    pub fn phase(&self) -> KernelPhase {
        self.app.kernel().phase()
    }

    /// Merged root config snapshot.
    #[must_use]
    pub const fn config(&self) -> &Config {
        &self.config
    }

    /// Compiled DI container.
    #[must_use]
    pub const fn container(&self) -> &Container {
        &self.container
    }

    /// Shuts the Serenade application down.
    ///
    /// # Errors
    ///
    /// Propagates Serenade kernel shutdown failures as [`BundleError`].
    pub fn shutdown(mut self) -> Result<(), BundleError> {
        self.app.shutdown()?;
        Ok(())
    }
}

/// Returns the kernel integration status for health and diagnostics surfaces.
///
/// Starts as [`SERENADE_KERNEL_PENDING`] until [`boot_kernel`] succeeds once in
/// this process, then [`SERENADE_KERNEL_BOOTED`].
#[must_use]
pub fn kernel_status() -> &'static str {
    KERNEL_STATUS
        .get()
        .copied()
        .unwrap_or(SERENADE_KERNEL_PENDING)
}

/// Boots Serenade (`FrameworkBundle` + [`RustashopBundle`]) and builds the DI container.
///
/// Loads dotenv files under `shop_root`, reads `config/packages` (plus env overlay),
/// and marks [`kernel_status`] as booted on success.
///
/// # Errors
///
/// Returns [`BundleError`] when environment parsing, dotenv, bundle registration,
/// boot, or container compile fails.
pub fn boot_kernel(shop_root: &Path) -> Result<RustashopKernel, BundleError> {
    let env_name = std::env::var(APP_ENV).unwrap_or_else(|_| "dev".to_owned());
    let environment =
        Environment::from_name(&env_name).map_err(|error| BundleError::Extension {
            alias: RUSTASHOP_BUNDLE,
            message: error.to_string(),
        })?;

    serenade_config::load_dotenv(shop_root, environment.as_str()).map_err(|error| {
        BundleError::Extension {
            alias: RUSTASHOP_BUNDLE,
            message: error.to_string(),
        }
    })?;

    let mut app = App::new(environment.clone());
    app.register_bundle(RustashopBundle)?;
    app.register_bundle(FrameworkBundle)?;
    app.boot()?;

    let packages = packages_dir(shop_root);
    let (config, container) = build_container(
        Some(packages.as_path()),
        environment.as_str(),
        &[&FrameworkExtension, &RustashopExtension],
    )?;

    let _ = KERNEL_STATUS.set(SERENADE_KERNEL_BOOTED);

    Ok(RustashopKernel {
        app,
        config,
        container,
    })
}

/// `shop_root/config/packages`.
#[must_use]
pub fn packages_dir(shop_root: &Path) -> PathBuf {
    shop_root.join(PACKAGES_DIR)
}

/// Ensures framework and rustashop packages exist under `shop_root` (tests / empty trees).
///
/// # Errors
///
/// Returns [`std::io::Error`] when directories or files cannot be created.
pub fn ensure_default_packages(shop_root: &Path) -> std::io::Result<()> {
    let packages = packages_dir(shop_root);
    std::fs::create_dir_all(&packages)?;
    let framework = packages.join("framework.toml");
    if !framework.is_file() {
        std::fs::write(&framework, "[framework]\nsecret = \"change-me\"\n")?;
    }
    let rustashop = packages.join("rustashop.toml");
    if !rustashop.is_file() {
        std::fs::write(&rustashop, "[rustashop]\nname = \"rustashop\"\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenade_bundle::FRAMEWORK_BUNDLE;
    use std::fs;
    use std::sync::Mutex;

    static APP_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn boot_kernel_registers_framework_and_rustashop() {
        let _guard = APP_ENV_LOCK.lock().expect("lock");
        // SAFETY: clear leftover APP_ENV from other tests in this process.
        unsafe {
            std::env::remove_var(APP_ENV);
        }
        let dir = tempfile_dir("boot");
        ensure_default_packages(&dir).expect("packages");
        let kernel = boot_kernel(&dir).expect("boot");
        assert_eq!(kernel.phase(), KernelPhase::Booted);
        assert_eq!(kernel.environment().as_str(), "dev");
        let names = kernel.bundle_names();
        assert!(names.contains(&FRAMEWORK_BUNDLE));
        assert!(names.contains(&RUSTASHOP_BUNDLE));
        assert!(kernel.config().parameters().contains_key("rustashop.name"));
        assert!(
            kernel
                .container()
                .parameters()
                .get("rustashop.name")
                .is_ok()
        );
        assert_eq!(packages_dir(&dir), dir.join(PACKAGES_DIR));
        assert_eq!(kernel_status(), SERENADE_KERNEL_BOOTED);
        kernel.shutdown().expect("shutdown");
    }

    #[test]
    fn ensure_default_packages_is_idempotent() {
        let dir = tempfile_dir("packages");
        ensure_default_packages(&dir).expect("first");
        ensure_default_packages(&dir).expect("second");
        assert!(packages_dir(&dir).join("framework.toml").is_file());
        assert!(packages_dir(&dir).join("rustashop.toml").is_file());
    }

    #[test]
    fn boot_kernel_rejects_empty_app_env() {
        let _guard = APP_ENV_LOCK.lock().expect("lock");
        let dir = tempfile_dir("bad-env");
        ensure_default_packages(&dir).expect("packages");
        // SAFETY: test-local APP_ENV override.
        unsafe {
            std::env::set_var(APP_ENV, "   ");
        }
        assert!(boot_kernel(&dir).is_err(), "empty APP_ENV must fail boot");
        unsafe {
            std::env::remove_var(APP_ENV);
        }
    }

    #[test]
    fn boot_kernel_rejects_missing_packages() {
        let _guard = APP_ENV_LOCK.lock().expect("lock");
        unsafe {
            std::env::remove_var(APP_ENV);
        }
        let dir = tempfile_dir("no-packages");
        assert!(boot_kernel(&dir).is_err(), "missing packages must fail");
    }

    #[test]
    fn boot_kernel_rejects_bad_dotenv() {
        let _guard = APP_ENV_LOCK.lock().expect("lock");
        unsafe {
            std::env::remove_var(APP_ENV);
        }
        let dir = tempfile_dir("bad-dotenv");
        ensure_default_packages(&dir).expect("packages");
        fs::write(
            dir.join(".env"),
            "BAD_LINE_WITHOUT_EQUALS\nFOO=\"unterminated\n",
        )
        .expect("dotenv");
        assert!(boot_kernel(&dir).is_err(), "malformed dotenv must fail");
    }

    fn tempfile_dir(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rustashop-kernel-{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("tmpdir");
        dir
    }
}
