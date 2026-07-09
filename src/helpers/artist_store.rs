use std::sync::Arc;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::io::Read;
use log::{debug, info, warn};
use once_cell::sync::Lazy;
use crate::data::artist::Artist;
use crate::helpers::coverart::get_coverart_manager;
use crate::helpers::musicbrainz::{search_mbids_for_artist, MusicBrainzSearchResult};

/// Result of an artist image operation
#[derive(Debug)]
pub enum ArtistImageResult {
    /// Image found and cached successfully
    Found { cache_path: String },
    /// Image not found
    NotFound,
    /// Error occurred during operation
    Error(String),
}

/// Configuration for the artist store
#[derive(Debug, Clone)]
pub struct ArtistStoreConfig {
    /// Base cache directory for artist images
    pub cache_dir: String,
    /// User directory for custom artist images (takes precedence over cache)
    pub user_dir: String,
    /// Whether to enable custom artist images from settings
    pub enable_custom_images: bool,
    /// Whether to automatically download missing images
    pub auto_download: bool,
}

impl Default for ArtistStoreConfig {
    fn default() -> Self {
        // Read configuration from settings database with fallback defaults
        let cache_dir = crate::helpers::settings_db::get_string_with_default(
            "datastore.artist_store.cache_dir",
            "/var/lib/audiocontrol/cache/artists"
        ).unwrap_or_else(|_| "/var/lib/audiocontrol/cache/artists".to_string());

        let user_dir = crate::helpers::settings_db::get_string_with_default(
            "datastore.user_image_path",
            "/var/lib/audiocontrol/user/images"
        ).unwrap_or_else(|_| "/var/lib/audiocontrol/user/images".to_string());

        let enable_custom_images = crate::helpers::settings_db::get_bool_with_default(
            "datastore.artist_store.enable_custom_images",
            true
        ).unwrap_or(true);

        let auto_download = crate::helpers::settings_db::get_bool_with_default(
            "datastore.artist_store.auto_download",
            true
        ).unwrap_or(true);

        Self {
            cache_dir,
            user_dir,
            enable_custom_images,
            auto_download,
        }
    }
}

/// Artist store for managing artist cover art download and caching
pub struct ArtistStore {
    /// Configuration
    config: ArtistStoreConfig,
    /// Cache of artist image paths
    image_cache: HashMap<String, String>,
    /// Currently downloading artists to prevent duplicate downloads
    downloading: HashMap<String, Arc<std::sync::atomic::AtomicBool>>,
}

impl Default for ArtistStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtistStore {
    /// Create a new artist store with default configuration
    pub fn new() -> Self {
        Self::with_config(ArtistStoreConfig::default())
    }

    /// Create a new artist store with custom configuration
    pub fn with_config(config: ArtistStoreConfig) -> Self {
        Self {
            config,
            image_cache: HashMap::new(),
            downloading: HashMap::new(),
        }
    }

    /// Get the local cache path for an artist's cover art
    ///
    /// # Arguments
    /// * `artist_name` - The name of the artist
    /// * `image_type` - Type of image ("custom", "cover", etc.)
    ///
    /// # Returns
    /// The local cache path for the artist's image
    pub fn get_artist_image_path(&self, artist_name: &str, image_type: &str) -> String {
        let sanitized_name = crate::helpers::sanitize::filename_from_string(artist_name);
        format!("{}/{}/{}.jpg", self.config.cache_dir, sanitized_name, image_type)
    }

    /// Get the user directory path for an artist's custom cover art
    ///
    /// # Arguments
    /// * `artist_name` - The name of the artist
    /// * `image_type` - Type of image ("custom", "cover", etc.)
    ///
    /// # Returns
    /// The user directory path for the artist's image
    pub fn get_artist_user_image_path(&self, artist_name: &str, image_type: &str) -> String {
        let sanitized_name = crate::helpers::sanitize::filename_from_string(artist_name);
        format!("{}/artists/{}/{}.jpg", self.config.user_dir, sanitized_name, image_type)
    }

    /// Check if an artist image exists in cache
    ///
    /// # Arguments
    /// * `artist_name` - The name of the artist
    /// * `image_type` - Type of image ("custom", "cover", etc.)
    ///
    /// # Returns
    /// True if the image exists in cache
    pub fn has_cached_image(&self, artist_name: &str, image_type: &str) -> bool {
        let cache_path = self.get_artist_image_path(artist_name, image_type);
        std::fs::metadata(&cache_path).is_ok()
    }

    /// Get the cached image path for an artist if it exists
    ///
    /// # Arguments
    /// * `artist_name` - The name of the artist
    ///
    /// # Returns
    /// ArtistImageResult with the cache path if found
    pub fn get_cached_image(&mut self, artist_name: &str) -> ArtistImageResult {
        debug!("Checking cached image for artist: {}", artist_name);

        // Check cache first
        if let Some(cached_path) = self.image_cache.get(artist_name) {
            if std::fs::metadata(cached_path).is_ok() {
                debug!("Found cached image path for artist {}: {}", artist_name, cached_path);
                return ArtistImageResult::Found { cache_path: cached_path.clone() };
            } else {
                // Remove stale cache entry
                self.image_cache.remove(artist_name);
            }
        }

        // Check user directory first (takes precedence over cache)
        let user_custom_path = self.get_artist_user_image_path(artist_name, "custom");
        if std::fs::metadata(&user_custom_path).is_ok() {
            debug!("Found user custom image for artist {}: {}", artist_name, user_custom_path);
            self.image_cache.insert(artist_name.to_string(), user_custom_path.clone());
            return ArtistImageResult::Found { cache_path: user_custom_path };
        }

        let user_cover_path = self.get_artist_user_image_path(artist_name, "cover");
        if std::fs::metadata(&user_cover_path).is_ok() {
            debug!("Found user cover image for artist {}: {}", artist_name, user_cover_path);
            self.image_cache.insert(artist_name.to_string(), user_cover_path.clone());
            return ArtistImageResult::Found { cache_path: user_cover_path };
        }

        // Check for custom image in cache directory
        if self.config.enable_custom_images {
            let custom_path = self.get_artist_image_path(artist_name, "custom");
            if std::fs::metadata(&custom_path).is_ok() {
                debug!("Found custom image for artist {}: {}", artist_name, custom_path);
                self.image_cache.insert(artist_name.to_string(), custom_path.clone());
                return ArtistImageResult::Found { cache_path: custom_path };
            }
        }

        // Check for regular cover image in cache directory
        let cover_path = self.get_artist_image_path(artist_name, "cover");
        if std::fs::metadata(&cover_path).is_ok() {
            debug!("Found cover image for artist {}: {}", artist_name, cover_path);
            self.image_cache.insert(artist_name.to_string(), cover_path.clone());
            return ArtistImageResult::Found { cache_path: cover_path };
        }

        debug!("No cached image found for artist: {}", artist_name);
        ArtistImageResult::NotFound
    }

    /// Download and cache an artist image from a URL
    ///
    /// # Arguments
    /// * `artist_name` - The name of the artist
    /// * `url` - The URL to download the image from
    /// * `image_type` - Type of image ("custom", "cover", etc.)
    ///
    /// # Returns
    /// ArtistImageResult with the cache path if successful
    pub fn download_and_cache_image(&mut self, artist_name: &str, url: &str, image_type: &str) -> ArtistImageResult {
        debug!("Downloading image for artist {} from URL: {}", artist_name, url);

        // Check if already downloading
        if let Some(downloading_flag) = self.downloading.get(artist_name) {
            if downloading_flag.load(std::sync::atomic::Ordering::Relaxed) {
                debug!("Image already being downloaded for artist: {}", artist_name);
                return ArtistImageResult::Error("Download already in progress".to_string());
            }
        }

        // Mark as downloading
        let downloading_flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
        self.downloading.insert(artist_name.to_string(), downloading_flag.clone());

        let result = match self.download_image(url) {
            Ok(image_data) => {
                let cache_path = self.get_artist_image_path(artist_name, image_type);

                match self.store_image(&cache_path, &image_data) {
                    Ok(_) => {
                        info!("Downloaded and cached {} image for artist {}", image_type, artist_name);
                        self.image_cache.insert(artist_name.to_string(), cache_path.clone());
                        ArtistImageResult::Found { cache_path }
                    },
                    Err(e) => {
                        warn!("Failed to store {} image for artist {}: {}", image_type, artist_name, e);
                        ArtistImageResult::Error(format!("Failed to store image: {}", e))
                    }
                }
            },
            Err(e) => {
                warn!("Failed to download image for artist {} from URL {}: {}", artist_name, url, e);
                ArtistImageResult::Error(format!("Failed to download image: {}", e))
            }
        };

        // Clear downloading flag
        downloading_flag.store(false, std::sync::atomic::Ordering::Relaxed);
        self.downloading.remove(artist_name);

        result
    }

    /// Download and store image directly to the user directory
    ///
    /// # Arguments
    /// * `artist_name` - The name of the artist
    /// * `url` - URL of the image to download
    /// * `image_type` - Type of image ("custom", "cover", etc.)
    ///
    /// # Returns
    /// ArtistImageResult with the user path if successfully downloaded and stored
    pub fn download_and_store_user_image(&mut self, artist_name: &str, url: &str, image_type: &str) -> ArtistImageResult {
        debug!("Downloading image for artist {} from URL to user directory: {}", artist_name, url);

        // Check if already downloading
        if let Some(downloading_flag) = self.downloading.get(artist_name) {
            if downloading_flag.load(std::sync::atomic::Ordering::Relaxed) {
                debug!("Image already being downloaded for artist: {}", artist_name);
                return ArtistImageResult::Error("Download already in progress".to_string());
            }
        }

        // Mark as downloading
        let downloading_flag = Arc::new(std::sync::atomic::AtomicBool::new(true));
        self.downloading.insert(artist_name.to_string(), downloading_flag.clone());

        let result = match self.download_image(url) {
            Ok(image_data) => {
                let user_path = self.get_artist_user_image_path(artist_name, image_type);

                match self.store_image(&user_path, &image_data) {
                    Ok(_) => {
                        info!("Downloaded and stored {} image for artist {} in user directory", image_type, artist_name);
                        // Also cache the path for quick access
                        self.image_cache.insert(artist_name.to_string(), user_path.clone());
                        ArtistImageResult::Found { cache_path: user_path }
                    },
                    Err(e) => {
                        warn!("Failed to store {} image for artist {} in user directory: {}", image_type, artist_name, e);
                        ArtistImageResult::Error(format!("Failed to store image: {}", e))
                    }
                }
            },
            Err(e) => {
                warn!("Failed to download image for artist {} from URL {}: {}", artist_name, url, e);
                ArtistImageResult::Error(format!("Failed to download image: {}", e))
            }
        };

        // Clear downloading flag
        downloading_flag.store(false, std::sync::atomic::Ordering::Relaxed);
        self.downloading.remove(artist_name);

        result
    }

    /// Get or download artist cover art
    ///
    /// # Arguments
    /// * `artist_name` - The name of the artist
    ///
    /// # Returns
    /// ArtistImageResult with the cache path if found or downloaded
    pub fn get_or_download_artist_image(&mut self, artist_name: &str) -> ArtistImageResult {
        debug!("Getting or downloading image for artist: {}", artist_name);

        // First check if we already have a cached image
        if let ArtistImageResult::Found { cache_path } = self.get_cached_image(artist_name) {
            return ArtistImageResult::Found { cache_path };
        }

        // If auto-download is disabled, return not found
        if !self.config.auto_download {
            return ArtistImageResult::NotFound;
        }

        // Check for custom image URL in settings first
        if self.config.enable_custom_images {
            let custom_url_key = format!("artist.image.{}", artist_name);
            if let Ok(Some(custom_url)) = crate::helpers::settings_db::get_string(&custom_url_key) {
                if !custom_url.is_empty() {
                    debug!("Found custom image URL for artist {}: {}", artist_name, custom_url);
                    return self.download_and_cache_image(artist_name, &custom_url, "custom");
                }
            }
        }

        // Use the cover art system to find images
        let manager = get_coverart_manager();
        let manager_guard = manager.lock();
        let results = manager_guard.get_artist_coverart(artist_name);
        drop(manager_guard);

        if results.is_empty() {
            debug!("No cover art found for artist {}", artist_name);
            return ArtistImageResult::NotFound;
        }

        // Find the highest-rated image across all providers
        let mut best_image: Option<&crate::helpers::coverart::ImageInfo> = None;
        let mut best_grade = -10; // Start lower to allow grade -1 images

        for result in &results {
            for image in &result.images {
                let grade = image.grade.unwrap_or(0);
                if grade > best_grade {
                    best_grade = grade;
                    best_image = Some(image);
                }
            }
        }

        if let Some(best_image) = best_image {
            debug!("Found best image for artist {} with grade {}: {}", artist_name, best_grade, best_image.url);
            self.download_and_cache_image(artist_name, &best_image.url, "cover")
        } else {
            debug!("No images with valid grades found for artist {}", artist_name);
            ArtistImageResult::NotFound
        }
    }

    /// Update an artist with cover art information
    ///
    /// # Arguments
    /// * `artist` - The artist to update
    ///
    /// # Returns
    /// The updated artist with image URLs in metadata
    pub fn update_artist_with_coverart(&mut self, mut artist: Artist) -> Artist {
        debug!("Updating artist {} with cover art", artist.name);

        match self.get_or_download_artist_image(&artist.name) {
            ArtistImageResult::Found { cache_path: _ } => {
                // Initialize metadata if needed
                if artist.metadata.is_none() {
                    artist.metadata = Some(crate::data::ArtistMeta::new());
                }

                // Add the cached image to the artist metadata
                if let Some(ref mut metadata) = artist.metadata {
                    // Generate proper API URL for artist image
                    let encoded_name = crate::helpers::url_encoding::encode_url_safe(&artist.name);
                    let api_url = format!("{}/coverart/artist/{}/image", crate::constants::API_PREFIX, encoded_name);
                    metadata.thumb_url = vec![api_url];
                    debug!("Updated artist {} with coverart API image URL: /api/coverart/artist/{}/image", artist.name, encoded_name);
                }
            },
            ArtistImageResult::NotFound => {
                debug!("No image available for artist {}", artist.name);
            },
            ArtistImageResult::Error(e) => {
                warn!("Error getting image for artist {}: {}", artist.name, e);
            }
        }

        artist
    }

    /// Looks up MusicBrainz IDs for an artist and returns them if found
    ///
    /// This function searches for MusicBrainz IDs associated with the given artist name.
    ///
    /// # Arguments
    /// * `artist_name` - The name of the artist to look up
    ///
    /// # Returns
    /// A tuple containing:
    /// * `Vec<String>` - Vector of MusicBrainz IDs if found, empty vector otherwise
    /// * `bool` - true if this is a partial match (only some artists in a multi-artist name found)
    pub fn lookup_artist_mbids(&self, artist_name: &str) -> (Vec<String>, bool) {
        debug!("Looking up MusicBrainz IDs for artist: {}", artist_name);

        // Try to retrieve MusicBrainz ID using search_mbids_for_artist function
        // This is now a fully synchronous call since we replaced musicbrainz_rs with direct HTTP
        let search_result = search_mbids_for_artist(artist_name, true, false, true);

        match search_result {
            MusicBrainzSearchResult::Found(mbids, _) => {
                debug!("Found {} MusicBrainz ID(s) for artist {}: {:?}",
                      mbids.len(), artist_name, mbids);
                (mbids, false) // Complete match
            },
            MusicBrainzSearchResult::FoundPartial(mbids, _) => {
                info!("Found {} partial MusicBrainz ID(s) for multi-artist {}: {:?}",
                      mbids.len(), artist_name, mbids);
                (mbids, true) // Partial match
            },
            MusicBrainzSearchResult::NotFound => {
                info!("No MusicBrainz ID found for artist: {}", artist_name);
                (Vec::new(), false)
            },
            MusicBrainzSearchResult::Error(error) => {
                warn!("Error retrieving MusicBrainz ID for artist {}: {}", artist_name, error);
                (Vec::new(), false)
            }
        }
    }

    /// Updates artist data by fetching additional information like MusicBrainz IDs
    ///
    /// This function takes an artist and attempts to retrieve and set any missing data
    /// such as MusicBrainz IDs.
    ///
    /// # Arguments
    /// * `artist` - The artist to update
    ///
    /// # Returns
    /// The updated artist
    pub fn update_data_for_artist(&mut self, mut artist: Artist) -> Artist {
        debug!("Updating data for artist: {}", artist.name);

        // Check if the artist already has MusicBrainz IDs set
        let has_mbid = match &artist.metadata {
            Some(meta) => !meta.mbid.is_empty(),
            None => false,
        };

        if !has_mbid {
            debug!("No MusicBrainz ID set for artist {}, attempting to retrieve it", artist.name);

            // Use the synchronous function to look up MusicBrainz IDs directly
            let (mbids, partial_match) = self.lookup_artist_mbids(&artist.name);
            let mbid_count = mbids.len();

            // Add each MusicBrainz ID to the artist if any were found
            for mbid in mbids {
                artist.add_mbid(mbid);
            }

            // if there is more than one mbid or it was a partial match, it's a multi-artist entry
            if mbid_count > 1 || partial_match {
                artist.is_multi = true; // Mark as multi-artist entry
                artist.clear_metadata(); // Clear metadata for multi-artist entries
                debug!("Cleared metadata for multi-artist entry: {}", artist.name);
            } else if mbid_count > 0 {
                info!("Updated artist '{}' with MusicBrainz data: {} ID(s)", artist.name, mbid_count);
                debug!("Added MusicBrainz ID(s) to artist {}", artist.name);
            }

            // Record if this is a partial match in the artist metadata
            if partial_match {
                debug!("Partial match found for multi-artist name: {}", artist.name);
                artist.ensure_metadata();
                if let Some(meta) = &mut artist.metadata {
                    meta.is_partial_match = true;
                }
            }
        } else {
            debug!("Artist {} already has MusicBrainz ID(s)", artist.name);
        }

        // If the artist has MusicBrainz IDs, update from the coverart system
        if artist.metadata.as_ref().is_some_and(|meta| !meta.mbid.is_empty()) {
            debug!("Artist {} has MusicBrainz ID(s), updating with cover art system", artist.name);
            artist = self.update_artist_with_coverart(artist);
        } else {
            // For artists without MusicBrainz IDs, still try coverart system with artist name only
            debug!("Artist {} has no MusicBrainz ID, trying cover art by name only", artist.name);
            artist = self.update_artist_with_coverart(artist);
        }

        // Note: LastFM metadata is now handled by the unified coverart system
        // No need for separate LastFM calls as the coverart system includes LastFM provider

        // Handle artists without MusicBrainz IDs but with existing thumbnails
        if artist.metadata.as_ref().is_some_and(|meta| meta.mbid.is_empty()) {
            // Check if the artist has thumbnail images
            let has_thumbnails = match &artist.metadata {
                Some(meta) => !meta.thumb_url.is_empty(),
                None => false,
            };

            if has_thumbnails {
                debug!("Artist {} has thumbnail image(s) but no MusicBrainz ID, skipping updates", artist.name);
            }
        }

        // Store the updated metadata in cache
        if let Some(metadata) = &artist.metadata {
            // Create a cache key using the artist's name
            let cache_key = format!("artist::metadata::{}", artist.name);

            // Store the metadata in the attribute cache
            match crate::helpers::attribute_cache::set(&cache_key, metadata) {
                Ok(_) => debug!("Stored metadata for artist {} in attribute cache", artist.name),
                Err(e) => warn!("Failed to store metadata for artist {} in attribute cache: {}", artist.name, e),
            }

            // If the artist has MusicBrainz IDs, store them separately for faster lookup
            if !metadata.mbid.is_empty() {
                let mbid_key = format!("artist::mbid::{}", artist.name);
                if let Err(e) = crate::helpers::attribute_cache::set(&mbid_key, &metadata.mbid) {
                    warn!("Failed to store MusicBrainz IDs for artist {} in attribute cache: {}", artist.name, e);
                }
            }
        }

        // Return the potentially updated artist
        artist
    }

    /// Clear cached image for an artist
    ///
    /// # Arguments
    /// * `artist_name` - The name of the artist
    pub fn clear_cached_image(&mut self, artist_name: &str) {
        self.image_cache.remove(artist_name);

        // Remove user directory images
        let user_custom_path = self.get_artist_user_image_path(artist_name, "custom");
        let _ = std::fs::remove_file(&user_custom_path);

        let user_cover_path = self.get_artist_user_image_path(artist_name, "cover");
        let _ = std::fs::remove_file(&user_cover_path);

        // Remove cache directory images
        let custom_path = self.get_artist_image_path(artist_name, "custom");
        let _ = std::fs::remove_file(&custom_path);

        let cover_path = self.get_artist_image_path(artist_name, "cover");
        let _ = std::fs::remove_file(&cover_path);

        debug!("Cleared cached images for artist: {}", artist_name);
    }

    /// Download an image from a URL
    ///
    /// # Arguments
    /// * `url` - The URL to download the image from
    ///
    /// # Returns
    /// Result with the image data or an error message
    fn download_image(&self, url: &str) -> Result<Vec<u8>, String> {
        debug!("Downloading image from URL: {}", url);

        // Use ureq to download the image
        match ureq::get(url).call() {
            Ok(response) => {
                let mut bytes = Vec::new();
                if let Err(e) = response.into_reader().read_to_end(&mut bytes) {
                    return Err(format!("Failed to read image data: {}", e));
                }

                if bytes.is_empty() {
                    return Err("Downloaded image is empty".to_string());
                }

                debug!("Successfully downloaded image: {} bytes", bytes.len());
                Ok(bytes)
            },
            Err(e) => {
                Err(format!("HTTP request failed: {}", e))
            }
        }
    }

    /// Store image data to a file
    ///
    /// # Arguments
    /// * `cache_path` - The path to store the image
    /// * `image_data` - The image data to store
    ///
    /// # Returns
    /// Result indicating success or failure
    fn store_image(&self, cache_path: &str, image_data: &[u8]) -> Result<(), String> {
        // Use the existing image cache functionality
        crate::helpers::image_cache::store_image(cache_path, image_data)
            .map_err(|e| e.to_string())
    }
}

/// Global singleton instance of the artist store
static ARTIST_STORE: Lazy<Arc<Mutex<ArtistStore>>> = Lazy::new(|| {
    Arc::new(Mutex::new(ArtistStore::new()))
});

/// Get the global artist store instance
pub fn get_artist_store() -> Arc<Mutex<ArtistStore>> {
    ARTIST_STORE.clone()
}

/// Convenience function to get cached image for an artist
///
/// # Arguments
/// * `artist_name` - The name of the artist
///
/// # Returns
/// Option with the cache path if found
pub fn get_artist_cached_image(artist_name: &str) -> Option<String> {
    let store_arc = get_artist_store();
    let mut store = store_arc.lock();
    match store.get_cached_image(artist_name) {
        ArtistImageResult::Found { cache_path } => Some(cache_path),
        _ => None,
    }
}

/// Convenience function to get or download artist image
///
/// # Arguments
/// * `artist_name` - The name of the artist
///
/// # Returns
/// Option with the cache path if found or downloaded
pub fn get_or_download_artist_image(artist_name: &str) -> Option<String> {
    let store_arc = get_artist_store();
    let mut store = store_arc.lock();
    match store.get_or_download_artist_image(artist_name) {
        ArtistImageResult::Found { cache_path } => Some(cache_path),
        _ => None,
    }
}

/// Convenience function to update an artist with cover art
///
/// # Arguments
/// * `artist` - The artist to update
///
/// # Returns
/// The updated artist with cover art information
pub fn update_artist_with_coverart(artist: Artist) -> Artist {
    let store_arc = get_artist_store();
    let mut store = store_arc.lock();
    store.update_artist_with_coverart(artist)
}

/// Convenience function to lookup MusicBrainz IDs for an artist
///
/// # Arguments
/// * `artist_name` - The name of the artist
///
/// # Returns
/// A tuple containing:
/// * `Vec<String>` - Vector of MusicBrainz IDs if found, empty vector otherwise
/// * `bool` - true if this is a partial match (only some artists in a multi-artist name found)
pub fn lookup_artist_mbids(artist_name: &str) -> (Vec<String>, bool) {
    let store_arc = get_artist_store();
    let store = store_arc.lock();
    store.lookup_artist_mbids(artist_name)
}

/// Convenience function to update artist data including metadata and cover art
///
/// # Arguments
/// * `artist` - The artist to update
///
/// # Returns
/// The updated artist with metadata and cover art information
pub fn update_data_for_artist(artist: Artist) -> Artist {
    let store_arc = get_artist_store();
    let mut store = store_arc.lock();
    store.update_data_for_artist(artist)
}

/// Start a background thread to update metadata for all artists in the library sequentially
///
/// This function updates artist metadata using the update_data_for_artist method in a background process.
/// It takes an Arc to the artists collection for direct updating and reading.
///
/// # Arguments
/// * `artists_collection` - Arc to the artists collection for updating
pub fn update_library_artists_metadata_in_background(
    artists_collection: Arc<RwLock<HashMap<String, Artist>>>
) {
    debug!("Starting background thread to update artist metadata");

    // Spawn a new thread to handle the metadata updates
    use std::thread;
    thread::spawn(move || {
        info!("Artist metadata update thread started");

        // Get all artists from the collection
        let artists = {
            let artists_map = artists_collection.read();
            // Clone all artists for processing
            artists_map.values().cloned().collect::<Vec<_>>()
        };

        let total = artists.len();
        info!("Processing metadata for {} artists", total);

        for (index, artist) in artists.into_iter().enumerate() {
            let artist_name = artist.name.clone();
            debug!("Updating metadata for artist: {}", artist_name);

            // Use the synchronous version of update_data_for_artist
            let updated_artist = update_data_for_artist(artist);

            // Check if we found new metadata to log appropriately
            let has_new_metadata = {
                let original_metadata = {
                    let artists_map = artists_collection.read();
                    artists_map.get(&artist_name).and_then(|a| a.metadata.clone())
                };

                if let Some(new_metadata) = &updated_artist.metadata {
                    if !new_metadata.mbid.is_empty() {
                        match original_metadata {
                            Some(old_meta) if !old_meta.mbid.is_empty() => false,
                            _ => {
                                info!("Adding MusicBrainz ID(s) to artist {}", artist_name);
                                true
                            }
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            };

            // Update the artist in the collection
            {
                let mut artists_map = artists_collection.write();
                artists_map.insert(artist_name.clone(), updated_artist);

                if has_new_metadata {
                    debug!("Successfully updated artist {} in library collection", artist_name);
                }
            }

            // Log progress periodically
            let count = index + 1;
            if count % 10 == 0 || count == total {
                info!("Processed {}/{} artists for metadata", count, total);
            }

            // Sleep between updates to avoid overwhelming external services
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        info!("Artist metadata update process completed");
    });

    info!("Background artist metadata update initiated");
}

/// Convenience function to clear cached image for an artist
///
/// # Arguments
/// * `artist_name` - The name of the artist
pub fn clear_artist_cached_image(artist_name: &str) {
    let store_arc = get_artist_store();
    let mut store = store_arc.lock();
    store.clear_cached_image(artist_name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Create a test artist store with temporary directories
    fn create_test_store() -> (ArtistStore, TempDir, TempDir) {
        let cache_temp_dir = TempDir::new().expect("Failed to create temp cache dir");
        let user_temp_dir = TempDir::new().expect("Failed to create temp user dir");

        let config = ArtistStoreConfig {
            cache_dir: cache_temp_dir.path().to_string_lossy().to_string(),
            user_dir: user_temp_dir.path().to_string_lossy().to_string(),
            enable_custom_images: true,
            auto_download: true,
        };

        let store = ArtistStore::with_config(config);
        (store, cache_temp_dir, user_temp_dir)
    }

    #[test]
    fn test_user_directory_precedence() {
        let (mut store, _cache_temp, _user_temp) = create_test_store();
        let artist_name = "Test Artist";

        // Use the sanitized name format
        let sanitized_name = crate::helpers::sanitize::filename_from_string(artist_name);

        // Create user directory structure
        let user_artist_dir = Path::new(&store.config.user_dir).join("artists").join(&sanitized_name);
        fs::create_dir_all(&user_artist_dir).expect("Failed to create user artist dir");

        // Create cache directory structure (cache_dir already includes 'artists')
        let cache_artist_dir = Path::new(&store.config.cache_dir).join(&sanitized_name);
        fs::create_dir_all(&cache_artist_dir).expect("Failed to create cache artist dir");

        // Create a dummy image in cache
        let cache_image_path = cache_artist_dir.join("cover.jpg");
        fs::write(&cache_image_path, b"cache image data").expect("Failed to write cache image");

        // Create a dummy image in user directory
        let user_image_path = user_artist_dir.join("cover.jpg");
        fs::write(&user_image_path, b"user image data").expect("Failed to write user image");

        // Test that user directory takes precedence
        match store.get_cached_image(artist_name) {
            ArtistImageResult::Found { cache_path } => {
                assert!(cache_path.contains(&store.config.user_dir),
                    "User directory should take precedence over cache directory. Got: {}", cache_path);

                // Verify the content is from user directory
                let content = fs::read(&cache_path).expect("Failed to read image");
                assert_eq!(content, b"user image data");
            },
            _ => panic!("Should have found image in user directory"),
        }
    }

    #[test]
    fn test_get_artist_image_paths() {
        let (store, _cache_temp, _user_temp) = create_test_store();

        let cache_path = store.get_artist_image_path("Metallica", "cover");
        // Use the sanitized filename format (filename_from_string converts to lowercase)
        assert!(cache_path.contains("/metallica/cover.jpg"));
        assert!(cache_path.starts_with(&store.config.cache_dir));

        let user_path = store.get_artist_user_image_path("Metallica", "custom");
        assert!(user_path.contains("/artists/metallica/custom.jpg"));
        assert!(user_path.starts_with(&store.config.user_dir));
    }

    /// Real network download — skipped in normal CI; run with `cargo test -- --ignored`
    #[test]
    #[ignore]
    fn test_metallica_cover_download() {
        let (mut store, _cache_temp, _user_temp) = create_test_store();
        let artist_name = "Metallica";

        match store.get_or_download_artist_image(artist_name) {
            ArtistImageResult::Found { cache_path } => {
                assert!(Path::new(&cache_path).exists(), "Downloaded image file should exist");
                let metadata = fs::metadata(&cache_path).expect("Failed to get file metadata");
                assert!(metadata.len() > 1024, "Image should be larger than 1KB");
                assert!(metadata.len() < 10_000_000, "Image should be smaller than 10MB");
            },
            // NotFound or Error are acceptable in environments without providers/connectivity
            _ => {}
        }
    }

    #[test]
    fn test_cache_invalidation() {
        let (mut store, _cache_temp, _user_temp) = create_test_store();
        let artist_name = "Cache Test Artist";

        // Use the sanitized name format
        let sanitized_name = crate::helpers::sanitize::filename_from_string(artist_name);

        // Create cache directory structure (cache_dir already includes 'artists')
        let cache_artist_dir = Path::new(&store.config.cache_dir).join(&sanitized_name);
        fs::create_dir_all(&cache_artist_dir).expect("Failed to create cache artist dir");

        // Create a dummy image
        let image_path = cache_artist_dir.join("cover.jpg");
        fs::write(&image_path, b"test image data").expect("Failed to write test image");

        // First call should find the image and cache the path
        match store.get_cached_image(artist_name) {
            ArtistImageResult::Found { cache_path } => {
                assert_eq!(cache_path, image_path.to_string_lossy());
                assert!(store.image_cache.contains_key(artist_name));
            },
            _ => panic!("Should have found the test image"),
        }

        // Remove the file
        fs::remove_file(&image_path).expect("Failed to remove test image");

        // Second call should detect the missing file and remove from cache
        match store.get_cached_image(artist_name) {
            ArtistImageResult::NotFound => {
                assert!(!store.image_cache.contains_key(artist_name));
            },
            _ => panic!("Should not have found the removed image"),
        }
    }

    #[test]
    fn partial_match_flag_set_after_clear_metadata() {
        let mut artist = Artist {
            id: crate::data::Identifier::Numeric(42),
            name: "Artist A & Artist B".to_string(),
            is_multi: false,
            metadata: None,
        };

        let partial_match = true;
        let mbid_count = 1usize;

        if mbid_count > 1 || partial_match {
            artist.is_multi = true;
            artist.clear_metadata();
        }

        if partial_match {
            artist.ensure_metadata();
            if let Some(meta) = &mut artist.metadata {
                meta.is_partial_match = true;
            }
        }

        assert!(artist.is_multi);
        assert!(
            artist.metadata.as_ref().is_some_and(|meta| meta.is_partial_match),
            "is_partial_match should be set even after clear_metadata"
        );
    }

    #[test]
    fn test_download_prevention() {
        let (mut store, _cache_temp, _user_temp) = create_test_store();
        store.config.auto_download = false;

        match store.get_or_download_artist_image("NonExistent Artist") {
            ArtistImageResult::NotFound => {}
            other => panic!("Expected NotFound when auto-download is disabled, got {:?}", other),
        }
    }

    // --- path construction ---

    #[test]
    fn image_path_embeds_sanitized_name_and_type() {
        let (store, _c, _u) = create_test_store();
        let path = store.get_artist_image_path("AC/DC", "cover");
        assert!(path.ends_with("/cover.jpg"), "{}", path);
        assert!(path.starts_with(&store.config.cache_dir), "{}", path);
        // Sanitizer should have replaced the slash in "AC/DC" — the literal string must not appear
        assert!(!path.contains("AC/DC"), "raw artist name slash should have been sanitized: {}", path);
    }

    #[test]
    fn user_image_path_has_artists_subdirectory() {
        let (store, _c, _u) = create_test_store();
        let path = store.get_artist_user_image_path("Test Artist", "custom");
        assert!(path.contains("/artists/"), "{}", path);
        assert!(path.ends_with("/custom.jpg"), "{}", path);
        assert!(path.starts_with(&store.config.user_dir), "{}", path);
    }

    // --- has_cached_image ---

    #[test]
    fn has_cached_image_false_when_file_absent() {
        let (store, _c, _u) = create_test_store();
        assert!(!store.has_cached_image("Nobody", "cover"));
    }

    #[test]
    fn has_cached_image_true_when_file_present() {
        let (store, _c, _u) = create_test_store();
        let artist = "Has Cover";
        let sanitized = crate::helpers::sanitize::filename_from_string(artist);
        let dir = Path::new(&store.config.cache_dir).join(&sanitized);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("cover.jpg"), b"data").unwrap();
        assert!(store.has_cached_image(artist, "cover"));
    }

    // --- custom image priority over cover ---

    #[test]
    fn custom_image_takes_priority_over_cover_in_cache_dir() {
        let (mut store, _c, _u) = create_test_store();
        let artist = "Priority Test";
        let sanitized = crate::helpers::sanitize::filename_from_string(artist);
        let dir = Path::new(&store.config.cache_dir).join(&sanitized);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("cover.jpg"), b"cover").unwrap();
        fs::write(dir.join("custom.jpg"), b"custom").unwrap();

        match store.get_cached_image(artist) {
            ArtistImageResult::Found { cache_path } => {
                assert!(cache_path.ends_with("custom.jpg"), "custom should beat cover, got {}", cache_path);
            }
            other => panic!("Expected Found, got {:?}", other),
        }
    }

    // --- stale in-memory cache eviction ---

    #[test]
    fn stale_in_memory_entry_is_evicted_when_file_deleted() {
        let (mut store, _c, _u) = create_test_store();
        let artist = "Stale Cache";
        let sanitized = crate::helpers::sanitize::filename_from_string(artist);
        let dir = Path::new(&store.config.cache_dir).join(&sanitized);
        fs::create_dir_all(&dir).unwrap();
        let img = dir.join("cover.jpg");
        fs::write(&img, b"img").unwrap();

        // Populate in-memory cache
        store.get_cached_image(artist);
        assert!(store.image_cache.contains_key(artist));

        // Delete the file and call again
        fs::remove_file(&img).unwrap();
        match store.get_cached_image(artist) {
            ArtistImageResult::NotFound => {}
            other => panic!("Expected NotFound after file deletion, got {:?}", other),
        }
        assert!(!store.image_cache.contains_key(artist), "stale entry should have been evicted");
    }
}
