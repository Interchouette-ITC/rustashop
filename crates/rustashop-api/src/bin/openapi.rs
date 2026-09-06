//! Writes the utoipa `OpenAPI` document as JSON.

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use utoipa::OpenApi;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string_pretty(&rustashop_api::ApiDoc::openapi())?;
    if let Some(path) = env::args().nth(1) {
        write_spec(Path::new(&path), json.as_bytes())?;
    } else {
        io::stdout().write_all(json.as_bytes())?;
        io::stdout().write_all(b"\n")?;
    }
    Ok(())
}

fn write_spec(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}
