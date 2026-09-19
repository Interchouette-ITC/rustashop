//! rustashop native ops / logistics desktop client (GPUI).

#![forbid(unsafe_code)]

mod ui;

use clap::Parser;
use rustashop_ops_gpui::Config;

#[derive(Parser, Debug)]
#[command(
    name = "rustashop-ops-gpui",
    about = "Native ops / logistics client for the rustashop Commerce API"
)]
struct Cli {
    /// Actix Commerce API base URL (no trailing slash).
    #[arg(long, env = "RUSTASHOP_API_BASE", default_value = "http://127.0.0.1:8080")]
    api_base: String,
    /// Admin API path segment (`RUSTASHOP_ADMIN_API_PREFIX`).
    #[arg(long, env = "RUSTASHOP_ADMIN_API_PREFIX", default_value = "admin")]
    admin_prefix: String,
    /// Admin bearer token.
    #[arg(long, env = "RUSTASHOP_ADMIN_API_TOKEN")]
    token: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.token.trim().is_empty() {
        anyhow::bail!("set --token or RUSTASHOP_ADMIN_API_TOKEN");
    }
    ui::run(Config {
        api_base: cli.api_base.trim_end_matches('/').to_owned(),
        admin_prefix: cli.admin_prefix,
        token: cli.token,
    })
}
