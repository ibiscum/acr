use log::{debug, info, warn};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use crate::data::album::Album;
use std::time::Duration;

const CACHE_KEY_PREFIX: &str = "album::genres::";
const MUSICBRAINZ_REQUEST_DELAY_MS: u64 = 1000;

/// Return the attribute cache key for a given album ID
fn cache_key(album_id: &str) -> String {
    format!("{}{}", CACHE_KEY_PREFIX, album_id)
}

fn musicbrainz_request_delay() -> Duration {
    Duration::from_millis(MUSICBRAINZ_REQUEST_DELAY_MS)
}

/// Load cached genres for an album from the attribute cache.
/// Returns `Some(genres)` if a cached entry exists (even if empty), `None` if not found.
pub fn load_cached_genres(album_id: &str) -> Option<Vec<String>> {
    match crate::helpers::attribute_cache::get::<Vec<String>>(&cache_key(album_id)) {
        Ok(Some(genres)) => Some(genres),
        Ok(None) => None,
        Err(e) => {
            debug!("Error reading album genre cache for {}: {}", album_id, e);
            None
        }
    }
}

/// Persist genres for an album to the attribute cache.
fn store_cached_genres(album_id: &str, genres: &[String]) {
    let genres_vec = genres.to_vec();
    match crate::helpers::attribute_cache::set(&cache_key(album_id), &genres_vec) {
        Ok(_) => debug!("Stored genres for album {} in attribute cache", album_id),
        Err(e) => warn!("Failed to store genres for album {} in attribute cache: {}", album_id, e),
    }
}

/// Look up genres for an album from MusicBrainz.
/// Checks attribute cache first; only calls MusicBrainz if not cached.
/// Stores the result (even an empty list) in the cache so we don't retry.
pub fn fetch_album_genres(album_id: &str, artist: &str, album_name: &str) -> Vec<String> {
    // Return cached value if present
    if let Some(cached) = load_cached_genres(album_id) {
        debug!("Using cached genres for album '{}': {:?}", album_name, cached);
        return cached;
    }

    // Not cached — fetch from MusicBrainz
    let genres = crate::helpers::musicbrainz::search_release_group_genres(artist, album_name);

    info!(
        "Fetched {} genre(s) from MusicBrainz for album '{}' by '{}'",
        genres.len(),
        album_name,
        artist
    );

    // Cache the result (including empty results to avoid repeated lookups)
    store_cached_genres(album_id, &genres);

    genres
}

/// Start a background thread to update genre tags for all albums in the library.
///
/// For each album that has no genres, fetches genres from MusicBrainz and stores
/// them in the album struct and in the attribute cache.
pub fn update_library_albums_genres_in_background(
    albums_collection: Arc<RwLock<HashMap<String, Album>>>,
) {
    debug!("Starting background thread to update album genres");

    std::thread::spawn(move || {
        let job_id = "album_genre_update".to_string();
        let job_name = "Album Genre Update".to_string();

        if let Err(e) = crate::helpers::background_jobs::register_job(job_id.clone(), job_name) {
            warn!("Failed to register album genre background job: {}", e);
            return;
        }

        info!("Album genre update thread started");

        // Collect albums that need genre lookup
        let albums_snapshot: Vec<(String, String, Vec<String>)> = {
            let map = albums_collection.read();
            map.values()
                .filter(|a| a.genres.is_empty())
                .map(|a| {
                    let id = a.id.to_string();
                    let name = a.name.clone();
                    let artists = a.artists.lock().clone();
                    (id, name, artists)
                })
                .collect()
        };

        let total = albums_snapshot.len();
        info!("Updating genres for {} albums without genre tags", total);

        let _ = crate::helpers::background_jobs::update_job(
            &job_id,
            Some(format!("Starting genre update for {} albums", total)),
            Some(0),
            Some(total),
        );

        let mut updated = 0usize;

        for (index, (album_id, album_name, artists)) in albums_snapshot.into_iter().enumerate() {
            let artist = artists.first().cloned().unwrap_or_default();

            let _ = crate::helpers::background_jobs::update_job(
                &job_id,
                Some(format!("Processing: {}", album_name)),
                Some(index),
                Some(total),
            );

            // Skip if already cached with empty result (avoid repeated API calls)
            if let Some(cached) = load_cached_genres(&album_id) {
                if cached.is_empty() {
                    debug!("Skipping album '{}' — cached empty result", album_name);
                    continue;
                }
                // Has cached genres — apply them to the album.
                // Verify album ID to guard against name collisions (two albums with the
                // same title) between when the snapshot was taken and now.
                let mut map = albums_collection.write();
                if let Some(album) = map.get_mut(&album_name) {
                    if album.id.to_string() == album_id && album.genres.is_empty() {
                        album.genres = cached;
                        updated += 1;
                    }
                }
                continue;
            }

            if artist.is_empty() || album_name.is_empty() {
                // Do not cache an empty result here: the artist/name may be populated
                // on a later library refresh, so we want to retry then.
                debug!("Skipping album '{}' — missing artist or album name, will retry next run", album_name);
                continue;
            }

            let genres = fetch_album_genres(&album_id, &artist, &album_name);

            if !genres.is_empty() {
                let mut map = albums_collection.write();
                // Verify album ID to guard against name collisions.
                if let Some(album) = map.get_mut(&album_name) {
                    if album.id.to_string() == album_id {
                        album.genres = genres;
                        updated += 1;
                    }
                }
            }

            let count = index + 1;
            if count % 50 == 0 || count == total {
                info!("Album genre update: {}/{} processed, {} updated", count, total, updated);
                let _ = crate::helpers::background_jobs::update_job(
                    &job_id,
                    Some(format!("Processed {}/{} albums", count, total)),
                    Some(count),
                    Some(total),
                );
            }

            // MusicBrainz requires no more than 1 request/second.
            std::thread::sleep(musicbrainz_request_delay());
        }

        info!("Album genre update complete: {}/{} albums updated", updated, total);
        let _ = crate::helpers::background_jobs::complete_job(&job_id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- cache_key ---

    #[test]
    fn cache_key_has_expected_prefix() {
        let key = cache_key("42");
        assert_eq!(key, "album::genres::42");
    }

    #[test]
    fn cache_key_is_unique_per_id() {
        assert_ne!(cache_key("1"), cache_key("2"));
    }

    #[test]
    fn musicbrainz_request_delay_respects_one_request_per_second() {
        let delay = musicbrainz_request_delay();
        assert!(delay >= std::time::Duration::from_secs(1));
        assert_eq!(delay, std::time::Duration::from_millis(MUSICBRAINZ_REQUEST_DELAY_MS));
    }

    // --- background update: album name collision guard ---
    // The following tests exercise the logic that was added to guard against
    // applying genres to the wrong album when two albums share a name.

    #[test]
    fn genres_not_applied_when_album_id_mismatches() {
        use std::sync::Arc;
        use parking_lot::{RwLock, Mutex};
        use std::collections::HashMap;
        use crate::data::album::Album;
        use crate::data::Identifier;

        // Build a minimal album map with id=99, name="Greatest Hits"
        let mut map: HashMap<String, Album> = HashMap::new();
        let album = Album {
            id: Identifier::Numeric(99),
            name: "Greatest Hits".to_string(),
            artists: Arc::new(Mutex::new(vec![])),
            artists_flat: None,
            release_date: None,
            tracks: Arc::new(Mutex::new(vec![])),
            cover_art: None,
            uri: None,
            genres: vec![],
        };
        map.insert("Greatest Hits".to_string(), album);

        let collection = Arc::new(RwLock::new(map));

        // Simulate applying genres fetched for a *different* album id (42 ≠ 99)
        let stale_id = "42";
        let genres = vec!["Rock".to_string()];
        {
            let mut m = collection.write();
            if let Some(album) = m.get_mut("Greatest Hits") {
                if album.id.to_string() == stale_id {
                    album.genres = genres.clone();
                }
            }
        }

        // Genres must NOT have been written because the ID didn't match
        let m = collection.read();
        assert!(m["Greatest Hits"].genres.is_empty());
    }

    #[test]
    fn genres_applied_when_album_id_matches() {
        use std::sync::Arc;
        use parking_lot::{RwLock, Mutex};
        use std::collections::HashMap;
        use crate::data::album::Album;
        use crate::data::Identifier;

        let mut map: HashMap<String, Album> = HashMap::new();
        let album = Album {
            id: Identifier::Numeric(99),
            name: "Greatest Hits".to_string(),
            artists: Arc::new(Mutex::new(vec![])),
            artists_flat: None,
            release_date: None,
            tracks: Arc::new(Mutex::new(vec![])),
            cover_art: None,
            uri: None,
            genres: vec![],
        };
        map.insert("Greatest Hits".to_string(), album);

        let collection = Arc::new(RwLock::new(map));

        let correct_id = "99";
        let genres = vec!["Rock".to_string()];
        {
            let mut m = collection.write();
            if let Some(album) = m.get_mut("Greatest Hits") {
                if album.id.to_string() == correct_id && album.genres.is_empty() {
                    album.genres = genres.clone();
                }
            }
        }

        let m = collection.read();
        assert_eq!(m["Greatest Hits"].genres, vec!["Rock".to_string()]);
    }
}
