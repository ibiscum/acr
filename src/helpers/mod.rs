#[path = "attribute_cache.rs"]
pub mod attribute_cache;
#[path = "image_cache.rs"]
pub mod image_cache;
pub mod image_meta;
pub mod image_grader;
#[path = "artist_updater.rs"]
pub mod artist_updater;
#[path = "album_updater.rs"]
pub mod album_updater;
pub mod artist_store;
#[path = "artist_splitter.rs"]
pub mod artist_splitter;
#[path = "background_jobs.rs"]
pub mod background_jobs;
pub mod coverart;
pub mod coverart_providers;
pub mod local_coverart;
pub mod fanarttv;
pub mod memory_report;
pub mod stream_helper;
pub mod musicbrainz;
pub mod theaudiodb;
pub mod sanitize;
#[path = "mac_address.rs"]
pub mod mac_address;
pub mod http_client;
#[path = "rate_limit.rs"]
pub mod rate_limit;
pub mod lastfm;
pub mod security_store;
#[path = "settings_db.rs"]
pub mod settings_db;
pub mod spotify;
pub mod retry;
pub mod systemd;
pub mod playback_progress;
pub mod process_helper;
pub mod favourites;
pub mod genre_cleanup;
pub mod volume;
pub mod global_volume;
pub mod url_encoding;
pub mod configurator;
pub mod lyrics;
#[path = "song_title_splitter.rs"]
pub mod song_title_splitter;
#[path = "song_split_manager.rs"]
pub mod song_split_manager;
pub mod m3u;
pub mod bluez;
#[cfg(unix)]
pub mod mpris;
#[cfg(unix)]
pub mod shairportsync_messages;

use crate::data::artist::Artist;

pub use playback_progress::PlayerProgress;

/// Trait for services that can update artist metadata
pub trait ArtistUpdater {
    /// Update an artist with additional metadata from a service
    /// 
    /// # Arguments
    /// * `artist` - The artist to update
    /// 
    /// # Returns
    /// The updated artist with additional metadata
    fn update_artist(&self, artist: Artist) -> Artist;
}