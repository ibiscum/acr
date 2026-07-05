// Re-export the MPD player controller
mod mpd;
pub use mpd::MPDPlayerController;

// Export the MPD library interface
pub mod library;

// Export the MPD library loader
#[path = "library_loader.rs"]
mod library_loader;
