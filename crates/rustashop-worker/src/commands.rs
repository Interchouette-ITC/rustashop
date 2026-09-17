//! Console commands for rustashop ops.

use serenade_cache::{ArrayAdapter, CacheItemPool};
use serenade_console::{Command, ConsoleError, Input};
use tracing::info;

use crate::CATALOG_CACHE_POOL_SERVICE;

/// Prints kernel readiness and environment for ops.
#[derive(Clone, Copy, Debug, Default)]
pub struct AboutCommand;

impl Command for AboutCommand {
    fn name(&self) -> &'static str {
        "rustashop:about"
    }

    fn description(&self) -> &'static str {
        "Print rustashop kernel status and environment"
    }

    fn execute(&self, input: &Input) -> Result<(), ConsoleError> {
        println!("kernel_status={}", rustashop::kernel_status());
        println!("environment={}", input.environment().as_str());
        println!("debug={}", input.debug());
        if let Some(container) = input.container()
            && let Ok(name) = container.parameters().get("rustashop.name")
        {
            println!("rustashop.name={name}");
        }
        Ok(())
    }
}

/// Clears the DI catalog cache pool (console process local).
#[derive(Clone, Copy, Debug, Default)]
pub struct CacheClearCommand;

impl Command for CacheClearCommand {
    fn name(&self) -> &'static str {
        "cache:clear"
    }

    fn description(&self) -> &'static str {
        "Clear the console DI catalog cache pool"
    }

    fn execute(&self, input: &Input) -> Result<(), ConsoleError> {
        let Some(container) = input.container() else {
            return Err(ConsoleError::Failed(
                "cache:clear needs a container (Application::run_with)".to_owned(),
            ));
        };
        let pool = container
            .get_as::<ArrayAdapter>(CATALOG_CACHE_POOL_SERVICE)
            .map_err(|error| ConsoleError::Failed(error.to_string()))?;
        pool.clear()
            .map_err(|error| ConsoleError::Failed(error.to_string()))?;
        println!("cache:clear: catalog pool emptied");
        Ok(())
    }
}

/// Consumes sandbox messenger jobs (`--once` / `--limit=N`).
#[derive(Clone, Copy, Debug, Default)]
pub struct MessengerConsumeCommand;

impl Command for MessengerConsumeCommand {
    fn name(&self) -> &'static str {
        "messenger:consume"
    }

    fn description(&self) -> &'static str {
        "Consume sandbox messenger jobs (Redis multi-process, or in-memory --once)"
    }

    fn execute(&self, input: &Input) -> Result<(), ConsoleError> {
        let once = input.args().iter().any(|arg| arg == "--once");
        let limit = parse_limit(input.args());
        let options = rustashop_jobs::ConsumeOptions { once, limit };
        let messenger = rustashop_jobs::SandboxJobMessenger::new();
        let registry = rustashop_jobs::SandboxJobRegistry::new();
        let hub = rustashop_jobs::SandboxJobHub::new();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| ConsoleError::Failed(error.to_string()))?;
        let processed = runtime
            .block_on(rustashop_jobs::run_consume_loop(
                &messenger, registry, hub, options,
            ))
            .map_err(ConsoleError::Failed)?;
        info!(processed, once, ?limit, "messenger:consume finished");
        println!("messenger:consume: processed {processed} job(s)");
        Ok(())
    }
}

fn parse_limit(args: &[String]) -> Option<usize> {
    for arg in args {
        if let Some(raw) = arg.strip_prefix("--limit=") {
            return raw.parse().ok();
        }
    }
    args.windows(2).find_map(|pair| {
        (pair[0] == "--limit")
            .then(|| pair[1].parse().ok())
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenade_kernel::Environment;

    #[test]
    fn parse_limit_forms() {
        assert_eq!(parse_limit(&["--limit=3".into()]), Some(3));
        assert_eq!(parse_limit(&["--limit".into(), "2".into()]), Some(2));
        assert_eq!(parse_limit(&["--once".into()]), None);
    }

    #[test]
    fn about_without_container() {
        let input = Input::new(Environment::Dev, true, Vec::new(), None);
        AboutCommand.execute(&input).expect("about");
    }

    #[test]
    fn cache_clear_requires_container() {
        let input = Input::new(Environment::Dev, true, Vec::new(), None);
        assert!(CacheClearCommand.execute(&input).is_err());
    }

    #[test]
    fn messenger_consume_once_empty() {
        let input = Input::new(Environment::Dev, true, vec!["--once".into()], None);
        MessengerConsumeCommand.execute(&input).expect("consume");
    }
}
