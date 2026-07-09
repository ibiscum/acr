// Re-export the MetadataPipeReader and RAATPlayerController
mod metadata_pipe_reader;
mod raat;

pub use metadata_pipe_reader::MetadataPipeReader;
pub use raat::RAATPlayerController;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn regression_exports_metadata_pipe_reader_at_module_root() {
		let type_name = std::any::type_name::<MetadataPipeReader>();
		assert!(type_name.ends_with("MetadataPipeReader"));
	}

	#[test]
	fn regression_exports_raat_controller_at_module_root() {
		let type_name = std::any::type_name::<RAATPlayerController>();
		assert!(type_name.ends_with("RAATPlayerController"));
	}
}
