mod generic_controller;

#[cfg(test)]
mod tests;

pub use generic_controller::GenericPlayerController;

#[cfg(test)]
mod export_tests {
	use super::*;

	#[test]
	fn regression_exports_generic_controller_at_module_root() {
		let type_name = std::any::type_name::<GenericPlayerController>();
		assert!(type_name.ends_with("GenericPlayerController"));
	}
}
