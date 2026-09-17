//! Serenade console ops + sandbox messenger worker commands.

mod commands;

use std::sync::Arc;

use serenade_bundle::{BundleError, COMMAND_TAG, Extension};
use serenade_cache::ArrayAdapter;
use serenade_config::Config;
use serenade_console::CommandService;
use serenade_di::{ContainerBuilder, ServiceDefinition};

use commands::{AboutCommand, CacheClearCommand, MessengerConsumeCommand};

/// Service id for the ops catalog cache pool (`cache:clear`).
pub const CATALOG_CACHE_POOL_SERVICE: &str = "rustashop.cache.catalog";

/// DI extension registering rustashop console commands.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorkerExtension;

impl Extension for WorkerExtension {
    fn alias(&self) -> &'static str {
        "rustashop_worker"
    }

    fn load(&self, config: &Config, builder: &mut ContainerBuilder) -> Result<(), BundleError> {
        config.apply_to(builder.parameters_mut());
        builder.register(ServiceDefinition::new(CATALOG_CACHE_POOL_SERVICE), |_| {
            Ok(Box::new(ArrayAdapter::new()))
        })?;
        builder
            .register(
                ServiceDefinition::new("console.command.rustashop_about").with_tag(COMMAND_TAG),
                |_| Ok(Box::new(CommandService(Arc::new(AboutCommand)))),
            )
            .expect("rustashop about command id is unique");
        builder
            .register(
                ServiceDefinition::new("console.command.cache_clear").with_tag(COMMAND_TAG),
                |_| Ok(Box::new(CommandService(Arc::new(CacheClearCommand)))),
            )
            .expect("cache clear command id is unique");
        builder
            .register(
                ServiceDefinition::new("console.command.messenger_consume").with_tag(COMMAND_TAG),
                |_| Ok(Box::new(CommandService(Arc::new(MessengerConsumeCommand)))),
            )
            .expect("messenger consume command id is unique");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenade_console::{Command, Input};
    use serenade_kernel::Environment;

    #[test]
    fn worker_extension_registers_commands_and_cache_pool() {
        assert_eq!(WorkerExtension.alias(), "rustashop_worker");
        let config = Config::empty();
        let mut builder = ContainerBuilder::new();
        builder
            .parameters_mut()
            .set("rustashop.name", "coverage-shop");
        WorkerExtension
            .load(&config, &mut builder)
            .expect("load worker extension");
        let container = std::sync::Arc::new(builder.compile().expect("compile"));

        let about = container
            .get_as::<CommandService>("console.command.rustashop_about")
            .expect("about");
        assert_eq!(about.0.name(), "rustashop:about");
        assert_ne!(about.0.description(), "");

        let cache = container
            .get_as::<CommandService>("console.command.cache_clear")
            .expect("cache");
        assert_eq!(cache.0.name(), "cache:clear");
        assert_ne!(cache.0.description(), "");

        let consume = container
            .get_as::<CommandService>("console.command.messenger_consume")
            .expect("consume");
        assert_eq!(consume.0.name(), "messenger:consume");
        assert_ne!(consume.0.description(), "");

        let input = Input::new(
            Environment::Dev,
            true,
            Vec::new(),
            Some(std::sync::Arc::clone(&container)),
        );
        AboutCommand.execute(&input).expect("about with container");
        CacheClearCommand.execute(&input).expect("cache clear");

        let limit_input = Input::new(
            Environment::Dev,
            false,
            vec!["--limit".into(), "1".into()],
            Some(container),
        );
        MessengerConsumeCommand
            .execute(&limit_input)
            .expect("consume limit");
    }
}
