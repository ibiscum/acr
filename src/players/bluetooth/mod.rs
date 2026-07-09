mod bluetooth;

pub use bluetooth::BluetoothPlayerController;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn regression_exports_bluetooth_controller_at_module_root() {
		let type_name = std::any::type_name::<BluetoothPlayerController>();
		assert!(type_name.ends_with("BluetoothPlayerController"));
	}
}
