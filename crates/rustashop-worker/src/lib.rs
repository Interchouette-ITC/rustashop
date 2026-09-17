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
