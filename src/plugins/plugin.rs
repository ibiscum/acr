use std::any::Any;

/// Base trait for all plugins
pub trait Plugin {
    /// Get the name of the plugin
    fn name(&self) -> &str;

    /// Get the version of the plugin
    fn version(&self) -> &str;

    /// Initialize the plugin
    ///
    /// # Returns
    ///
    /// `true` if initialization was successful, `false` otherwise
    fn init(&mut self) -> bool;

    /// Shutdown the plugin
    ///
    /// # Returns
    ///
    /// `true` if shutdown was successful, `false` otherwise
    fn shutdown(&mut self) -> bool;

    /// Get the plugin as Any for downcasting
    fn as_any(&self) -> &dyn Any;
}

/// A base implementation of Plugin that can be used by other plugins
pub struct BasePlugin {
    /// Plugin name
    name: String,

    /// Plugin version
    version: String,

    /// Tracks whether init() has been called successfully.
    initialized: bool,
}

impl BasePlugin {
    /// Create a new BasePlugin
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            initialized: false,
        }
    }

    /// Create a new BasePlugin with a specific version
    pub fn with_version(name: &str, version: &str) -> Self {
        Self {
            name: name.to_string(),
            version: version.to_string(),
            initialized: false,
        }
    }
}

impl Plugin for BasePlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn init(&mut self) -> bool {
        if self.initialized {
            log::warn!("Plugin '{}' already initialized", self.name);
            return false;
        }
        log::info!("Plugin '{}' initialized", self.name);
        self.initialized = true;
        true
    }

    fn shutdown(&mut self) -> bool {
        if !self.initialized {
            log::warn!("Plugin '{}' shutdown requested before initialization", self.name);
            return false;
        }
        log::info!("Plugin '{}' shutdown", self.name);
        self.initialized = false;
        true
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_base_plugin_new_uses_package_version() {
        let plugin = BasePlugin::new("test-plugin");
        assert_eq!(plugin.name(), "test-plugin");
        assert_eq!(plugin.version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn regression_base_plugin_with_version_overrides_version() {
        let plugin = BasePlugin::with_version("test-plugin", "1.2.3-test");
        assert_eq!(plugin.name(), "test-plugin");
        assert_eq!(plugin.version(), "1.2.3-test");
    }

    #[test]
    fn regression_base_plugin_lifecycle_validates_state_transitions() {
        let mut plugin = BasePlugin::new("lifecycle-plugin");

        assert!(plugin.init());
        assert!(!plugin.init());

        assert!(plugin.shutdown());
        assert!(!plugin.shutdown());
    }

    #[test]
    fn integration_base_plugin_as_any_supports_downcast() {
        let plugin = BasePlugin::new("downcast-plugin");
        let downcast = plugin.as_any().downcast_ref::<BasePlugin>();
        assert!(downcast.is_some());
    }
}
