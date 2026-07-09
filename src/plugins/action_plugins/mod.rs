pub mod active_monitor;
pub mod event_logger;
pub mod lastfm; // Renamed from lastfm_plugin

// Re-export commonly used items
pub use active_monitor::ActiveMonitor;
pub use event_logger::{EventLogger, LogLevel};
pub use lastfm::{Lastfm, LastfmConfig}; // Renamed from lastfm_plugin and updated structs

#[cfg(test)]
mod tests {
	use super::*;
	use crate::plugins::plugin::Plugin;

	#[test]
	fn regression_action_plugins_module_reexports_core_types() {
		let monitor = ActiveMonitor::new();
		let logger = EventLogger::new(false);
		let lastfm = Lastfm::new(LastfmConfig {
			enabled: false,
			api_key: "key".to_string(),
			api_secret: "secret".to_string(),
			scrobble: true,
		});

		assert_eq!(monitor.name(), "ActiveMonitor");
		assert_eq!(logger.name(), "EventLogger");
		assert_eq!(lastfm.name(), "Lastfm");
	}

	#[test]
	fn regression_action_plugins_module_reexports_log_level() {
		assert_eq!(LogLevel::from("warn"), LogLevel::Warning);
		assert_eq!(LogLevel::from("error"), LogLevel::Error);
		assert_eq!(LogLevel::from("DEBUG"), LogLevel::Debug);
	}
}
