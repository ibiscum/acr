// Module declaration for librespot player implementation
mod librespot;

// Re-export for easier access from parent module
pub use librespot::LibrespotPlayerController;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn regression_exports_librespot_controller_at_module_root() {
		let ctor: fn(&str, Option<&str>) -> LibrespotPlayerController =
			LibrespotPlayerController::with_config_and_systemd;
		let _ = ctor;
	}
}
