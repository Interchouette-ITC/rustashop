//! `rustashop-mcp` - commerce MCP server (stdio or Streamable HTTP).
//!
//! ```bash
//! rustashop-mcp
//! rustashop-mcp --http
//! rustashop-mcp --http --listen 127.0.0.1:8090
//! MCP_HTTP=true rustashop-mcp
//! ```

use anyhow::Result;
use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};
use rustashop_mcp::{DEFAULT_HTTP_LISTEN, RustashopMcp, run_http};

#[derive(Debug, Parser)]
#[command(
    name = "rustashop-mcp",
    about = "rustashop commerce MCP server (stdio or Streamable HTTP): catalog, cart, checkout, admin tools",
    version
)]
struct Cli {
    /// Serve Streamable HTTP instead of stdio.
    #[arg(
        long,
        env = "MCP_HTTP",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    http: bool,

    /// HTTP bind address when `--http` is set (also: `RUSTASHOP_MCP_ADDR`).
    #[arg(long, env = "RUSTASHOP_MCP_ADDR", default_value = DEFAULT_HTTP_LISTEN)]
    listen: String,
}

fn init_logging() {
    // Keep stdio MCP quiet: many hosts treat any stderr line as an error.
    // Default warn; override with RUST_LOG when debugging.
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
        tracing::info!(addr = %cli.listen, "rustashop-mcp starting (HTTP)");
        run_http(&cli.listen)
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
    } else {
        tracing::info!("rustashop-mcp starting (stdio)");
        let service = RustashopMcp::from_env()
            .serve(stdio())
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        service
            .waiting()
            .await
            .map_err(|err| anyhow::anyhow!("{err}"))?;
    }
    Ok(())
}
