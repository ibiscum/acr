pub mod plugin;
pub mod plugin_factory;
pub mod action_plugin;
pub mod action_plugins;

// Re-export commonly used items
pub use plugin::Plugin;
pub use action_plugin::ActionPlugin;
pub use plugin_factory::PluginFactory;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn regression_plugins_module_reexports_plugin_factory() {
		let factory = PluginFactory::new();
		assert!(factory.is_registered("active-monitor"));
		assert!(factory.is_registered("event-logger"));
	}

	#[test]
	fn integration_plugins_module_reexports_plugin_trait() {
		let plugin = plugin::BasePlugin::new("reexport-test");
		let boxed: Box<dyn Plugin> = Box::new(plugin);
		assert_eq!(boxed.name(), "reexport-test");
	}
}
