//! Axum MCP and agent HTTP name marker.
//!
//! Workspace member that reserves the crate name for future MCP tool routes.
//! Today it only re-exports the application kernel status for shared diagnostics.

/// Crate name marker for workspace and diagnostics checks.
pub const MCP_CRATE: &str = "rustashop-mcp";

/// Kernel integration status from the `rustashop` application package.
#[must_use]
pub fn kernel_status() -> &'static str {
    rustashop::kernel_status()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_marker_and_kernel_status() {
        assert_eq!(MCP_CRATE, "rustashop-mcp");
        assert_eq!(kernel_status(), rustashop::kernel_status());
    }
}
