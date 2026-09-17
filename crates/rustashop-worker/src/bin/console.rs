//! rustashop Serenade console entry (`bin/console` analogue).

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use rustashop::{APP_ENV, RustashopExtension, boot_kernel, ensure_default_packages, packages_dir};
use rustashop_worker::WorkerExtension;
use serenade_bundle::{CONSOLE_APPLICATION_SERVICE, FrameworkExtension, build_container};
use serenade_console::Application;
use serenade_kernel::Environment;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let root = shop_root();
    let env_name = std::env::var(APP_ENV).unwrap_or_else(|_| "dev".to_owned());
    let environment = Environment::from_name(&env_name)?;
    serenade_config::load_dotenv(&root, environment.as_str())?;
    ensure_default_packages(&root)?;
    ensure_worker_package(&root)?;

    // Marks kernel_status booted for `rustashop:about`.
    boot_kernel(&root)?.shutdown()?;

    let packages = packages_dir(&root);
    let (_config, container) = build_container(
        Some(packages.as_path()),
        environment.as_str(),
        &[&FrameworkExtension, &RustashopExtension, &WorkerExtension],
    )?;
    let container = Arc::new(container);
    let console = container.get_as::<Application>(CONSOLE_APPLICATION_SERVICE)?;
    let argv: Vec<String> = std::env::args().collect();
    console.run_with(argv, Some(Arc::clone(&container)))?;
    Ok(())
}

fn shop_root() -> PathBuf {
    std::env::var_os("RUSTASHOP_ROOT").map_or_else(
        || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        PathBuf::from,
    )
}

fn ensure_worker_package(shop_root: &std::path::Path) -> std::io::Result<()> {
    let path = packages_dir(shop_root).join("rustashop_worker.toml");
    if !path.is_file() {
        std::fs::write(&path, "[rustashop_worker]\n")?;
    }
    Ok(())
}
