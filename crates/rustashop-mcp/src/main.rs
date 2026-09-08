//! `rustashop-mcp` - commerce MCP server (stdio by default, optional Streamable HTTP).

use anyhow::Result;
use clap::Parser;
use rustashop_mcp::{ALLOW_COMMIT_ENV, DEFAULT_HTTP_LISTEN, run_http, run_stdio};

#[derive(Debug, Parser)]
#[command(
    name = "rustashop-mcp",
    about = "rustashop commerce MCP server (stdio or Streamable HTTP on /mcp)",
    version
)]
struct Cli {
    /// Serve Streamable HTTP instead of stdio.
    #[arg(
        long,
        env = "RUSTASHOP_MCP_HTTP",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    http: bool,

    /// HTTP bind address when `--http` is set (also: `RUSTASHOP_MCP_BIND`).
    #[arg(long, env = "RUSTASHOP_MCP_BIND", default_value = DEFAULT_HTTP_LISTEN)]
    listen: String,
}

fn init_logging() {
    // Keep stdio MCP quiet: hosts often surface any stderr line as an error.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(std::io::stderr)
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();

    if cli.http {
        tracing::info!(
            addr = %cli.listen,
            allow_commit_env = ALLOW_COMMIT_ENV,
            "rustashop-mcp starting (HTTP)"
        );
        run_http(&cli.listen)
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
    } else {
        tracing::info!("rustashop-mcp starting (stdio)");
        run_stdio().await.map_err(|err| anyhow::anyhow!("{err}"))?;
    }
    Ok(())
}
