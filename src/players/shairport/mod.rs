mod shairport;

pub use shairport::ShairportController;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn regression_exports_shairport_controller_at_module_root() {
		let type_name = std::any::type_name::<ShairportController>();
		assert!(type_name.ends_with("ShairportController"));
	}
}
