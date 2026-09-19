//! rustashop native POS / TPV / caisse desktop client (GPUI).

#![forbid(unsafe_code)]

mod ui;

use std::path::PathBuf;

use clap::Parser;
use rustashop_pos_gpui::Config;

#[derive(Parser, Debug)]
#[command(
    name = "rustashop-pos-gpui",
    about = "Native POS / TPV / caisse client for the rustashop Commerce API"
)]
struct Cli {
    /// Actix Commerce API base URL (no trailing slash).
    #[arg(
        long,
        env = "RUSTASHOP_API_BASE",
        default_value = "http://127.0.0.1:8080"
    )]
    api_base: String,
    /// Append-only journal path (JSONL hash-chain stub).
    #[arg(
        long,
        env = "RUSTASHOP_POS_JOURNAL",
        default_value = "pos/gpui/data/journal.jsonl"
    )]
    journal: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    ui::run(
        Config {
            api_base: cli.api_base.trim_end_matches('/').to_owned(),
        },
        cli.journal,
    )
}
