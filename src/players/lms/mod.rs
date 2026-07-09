/// LMS (Lyrion Music Server) client module
#[path = "json_rps.rs"]
pub mod json_rps;
#[path = "lms_server.rs"]
pub mod lms_server;
#[path = "lms_audio.rs"]
pub mod lms_audio;
#[path = "lms_player.rs"]
pub mod lms_player;
pub mod player_finder;
pub mod cli_listener;
pub mod mapping;
pub mod library;
#[path = "library_loader.rs"]
pub mod library_loader;

// Re-export main components for easier access
pub use json_rps::{LmsRpcClient, LmsRpcError, Player, PlayerStatus, Track, Album, Artist, Playlist, SearchResults};
pub use lms_server::{LmsServer, find_local_servers};
pub use lms_audio::{LMSAudioController, LMSAudioConfig};
pub use lms_player::LMSPlayer;
pub use library::LMSLibrary;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn regression_exports_lms_audio_controller_at_module_root() {
		let ctor: fn(serde_json::Value) -> LMSAudioController = LMSAudioController::new;
		let _ = ctor;
	}

	#[test]
	fn regression_exports_lms_rpc_client_at_module_root() {
		let ctor: fn(&str, u16) -> LmsRpcClient = LmsRpcClient::new;
		let _ = ctor;
	}

	#[test]
	fn regression_exports_lms_library_at_module_root() {
		let ctor: fn(&str, u16) -> LMSLibrary = LMSLibrary::with_connection;
		let _ = ctor;
	}

	#[test]
	fn regression_exports_lms_player_at_module_root() {
		let ctor: fn(LmsRpcClient, &str) -> LMSPlayer = LMSPlayer::new;
		let _ = ctor;
	}

	#[test]
	fn regression_exports_lms_discovery_function_at_module_root() {
		let discover: fn(Option<u64>) -> std::io::Result<Vec<LmsServer>> = find_local_servers;
		let _ = discover;
	}
}
