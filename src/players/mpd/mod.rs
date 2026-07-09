// Re-export the MPD player controller
mod mpd;
pub use mpd::MPDPlayerController;

// Export the MPD library interface
pub mod library;

// Export the MPD library loader
mod library_loader;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn regression_exports_mpd_controller_at_module_root() {
		let type_name = std::any::type_name::<MPDPlayerController>();
		assert!(type_name.ends_with("MPDPlayerController"));
	}

	#[test]
	fn regression_exports_library_module() {
		let type_name = std::any::type_name::<library::MPDLibrary>();
		assert!(type_name.ends_with("MPDLibrary"));
	}
}
