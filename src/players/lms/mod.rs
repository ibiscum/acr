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