use log::{debug, info, warn};
use crate::data::artist::Artist;
use crate::helpers::musicbrainz::{search_mbids_for_artist, MusicBrainzSearchResult};
use crate::helpers::ArtistUpdater;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

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
pub fn lookup_artist_mbids(artist_name: &str) -> (Vec<String>, bool) {
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

/// Download and cache artist images using the cover art system
///
/// This function retrieves artist images using the new artist store module.
///
/// # Arguments
/// * `artist` - The artist to update with cover art
///
/// # Returns
/// The updated artist with image URLs in metadata
fn update_artist_with_coverart(artist: Artist) -> Artist {
    debug!("Updating artist {} with cover art system", artist.name);

    // Use the new artist store to handle cover art
    crate::helpers::artist_store::update_artist_with_coverart(artist)
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
pub fn update_data_for_artist(mut artist: Artist) -> Artist {
    debug!("Updating data for artist: {}", artist.name);

    if should_lookup_mbids(&artist) {
        debug!("No MusicBrainz ID set for artist {}, attempting to retrieve it", artist.name);

        // Use the synchronous function to look up MusicBrainz IDs directly
        // No more need for Tokio runtime since our function is now synchronous
        let (mbids, partial_match) = lookup_artist_mbids(&artist.name);
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

        // Record if this is a partial match in the artist metadata.
        // NOTE: clear_metadata() above may have set metadata to None, so ensure it
        // exists before writing the flag.
        if partial_match {
            debug!("Partial match found for multi-artist name: {}", artist.name);
            artist.ensure_metadata();
            if let Some(meta) = &mut artist.metadata {
                meta.is_partial_match = true;
            }
        }
    } else if artist.is_multi {
        debug!("Artist {} is already marked as multi-artist, skipping MBID lookup", artist.name);
    } else {
        debug!("Artist {} already has MusicBrainz ID(s)", artist.name);
    }

    // Always try the coverart system regardless of MBID state.
    artist = update_artist_with_coverart(artist);

    // Update with individual service providers for biography and additional metadata
    // Note: The coverart system handles images, but we need individual services for biography

    // Check if we need biography data or genre data
    let needs_biography = artist.metadata.as_ref().is_none_or(|meta| meta.biography.is_none());
    let needs_genres = artist.metadata.as_ref().is_none_or(|meta| meta.genres.is_empty());

    if needs_biography || needs_genres {
        debug!("Artist {} needs biography or genre data, calling individual service updaters", artist.name);

        // Track what we had before updating
        let had_biography_before = artist.metadata.as_ref().is_some_and(|meta| meta.biography.is_some());
        let genres_count_before = artist.metadata.as_ref().map_or(0, |meta| meta.genres.len());

        // Try LastFM first for biography and genres (usually has good data)
        let lastfm_updater = crate::helpers::lastfm::LastfmUpdater;
        artist = lastfm_updater.update_artist(artist);

        // Check what we got from LastFM
        let has_biography_after_lastfm = artist.metadata.as_ref().is_some_and(|meta| meta.biography.is_some());
        let genres_count_after_lastfm = artist.metadata.as_ref().map_or(0, |meta| meta.genres.len());

        if !had_biography_before && has_biography_after_lastfm {
            info!("Downloaded biography for artist '{}' from LastFM", artist.name);
        }
        if genres_count_after_lastfm > genres_count_before {
            let new_genres = genres_count_after_lastfm - genres_count_before;
            info!("Downloaded {} genre(s) for artist '{}' from LastFM", new_genres, artist.name);
        }

        // Check what we still need after LastFM
        let still_needs_biography = artist.metadata.as_ref().is_none_or(|meta| meta.biography.is_none());
        let still_needs_genres = artist.metadata.as_ref().is_none_or(|meta| meta.genres.is_empty());
        let has_mbid = artist.metadata.as_ref().is_some_and(|meta| !meta.mbid.is_empty());

        // If we still need data and have MusicBrainz ID, try TheAudioDB
        if (still_needs_biography || still_needs_genres) && has_mbid {
            debug!("Artist {} still needs biography or genres and has MBID, trying TheAudioDB", artist.name);

            // Track what we have before TheAudioDB
            let had_biography_before_tadb = artist.metadata.as_ref().is_some_and(|meta| meta.biography.is_some());
            let genres_count_before_tadb = artist.metadata.as_ref().map_or(0, |meta| meta.genres.len());

            let theaudiodb_updater = crate::helpers::theaudiodb::TheAudioDbUpdater;
            artist = theaudiodb_updater.update_artist(artist);

            // Check what we got from TheAudioDB
            let has_biography_after_tadb = artist.metadata.as_ref().is_some_and(|meta| meta.biography.is_some());
            let genres_count_after_tadb = artist.metadata.as_ref().map_or(0, |meta| meta.genres.len());

            if !had_biography_before_tadb && has_biography_after_tadb {
                info!("Downloaded biography for artist '{}' from TheAudioDB", artist.name);
            }
            if genres_count_after_tadb > genres_count_before_tadb {
                let new_genres = genres_count_after_tadb - genres_count_before_tadb;
                info!("Downloaded {} genre(s) for artist '{}' from TheAudioDB", new_genres, artist.name);
            }
        }

        // FanArt.tv updater no longer provides metadata - all image handling is done by CoverartProvider
        if has_mbid {
            debug!("Artist {} has MBID - FanArt.tv images will be handled by CoverartProvider", artist.name);
        }
    } else {
        debug!("Artist {} already has biography and genre data", artist.name);
    }

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
    if let Some(metadata) = &artist.metadata {        // Check if a library scan is running before writing to the database
        wait_for_library_scan_to_complete();
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

fn should_lookup_mbids(artist: &Artist) -> bool {
    let has_mbid = artist
        .metadata
        .as_ref()
        .is_some_and(|meta| !meta.mbid.is_empty());

    !has_mbid && !artist.is_multi
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
        let job_id = "artist_metadata_update".to_string();
        let job_name = "Artist Metadata Update".to_string();

        // Register the background job
        if let Err(e) = crate::helpers::background_jobs::register_job(job_id.clone(), job_name) {
            warn!("Failed to register background job: {}", e);
            return;
        }

        info!("Artist metadata update thread started");

        // Get all artists from the collection
        let artists = {
            let artists_map = artists_collection.read();
            // Clone all artists for processing
            artists_map.values().cloned().collect::<Vec<_>>()
        };

        let total = artists.len();
        info!("Processing metadata for {} artists", total);

        // Update the job with total count
        if let Err(e) = crate::helpers::background_jobs::update_job(
            &job_id,
            Some(format!("Starting metadata update for {} artists", total)),
            Some(0),
            Some(total)
        ) {
            warn!("Failed to update background job: {}", e);
        }

        for (index, artist) in artists.into_iter().enumerate() {
            let artist_name = artist.name.clone();
            debug!("Updating metadata for artist: {}", artist_name);

            // Update progress in background job
            let completed = index;
            let progress_message = format!("Processing artist: {}", artist_name);
            if let Err(e) = crate::helpers::background_jobs::update_job(
                &job_id,
                Some(progress_message),
                Some(completed),
                Some(total)
            ) {
                warn!("Failed to update background job progress: {}", e);
            }

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

                // Update background job with milestone progress
                if let Err(e) = crate::helpers::background_jobs::update_job(
                    &job_id,
                    Some(format!("Processed {}/{} artists", count, total)),
                    Some(count),
                    Some(total)
                ) {
                    warn!("Failed to update background job milestone: {}", e);
                }
            }

            // Sleep between updates to avoid overwhelming external services
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        info!("Artist metadata update process completed");

        // Complete and remove the background job
        if let Err(e) = crate::helpers::background_jobs::complete_job(&job_id) {
            warn!("Failed to complete background job: {}", e);
        }
    });

    info!("Background artist metadata update initiated");
}
/// Wait for any active library scan jobs to complete before writing metadata.
///
/// Library scans (MPD or LMS) lock the database while performing operations.
/// This function checks if either library_scan_mpd or library_scan_lms jobs are running,
/// and if so, waits for them to complete before returning. This prevents database
/// write conflicts between the artist updater and library scanner.
///
/// This ensures that metadata writes don't fail with "readonly database" errors.
fn wait_for_library_scan_to_complete() {
    const CHECK_INTERVAL_MS: u64 = 100; // Check every 100ms (more frequent than updater polling)
    const MAX_WAIT_SECS: u64 = 300; // But still have a timeout

    // Check for both MPD and LMS library scan jobs
    let scan_jobs = ["library_scan_mpd", "library_scan_lms"];

    for scan_job_id in &scan_jobs {
        if let Ok(Some(job)) = crate::helpers::background_jobs::get_job(scan_job_id) {
            if !job.finished {
                debug!("Library scan ({}) is active. Waiting for it to complete before writing metadata...", scan_job_id);

                let start = std::time::Instant::now();
                let timeout = Duration::from_secs(MAX_WAIT_SECS);

                loop {
                    if start.elapsed() > timeout {
                        warn!("Library scan ({}) did not complete within {} seconds. Proceeding with metadata write anyway.", scan_job_id, MAX_WAIT_SECS);
                        break;
                    }

                    if let Ok(Some(updated_job)) = crate::helpers::background_jobs::get_job(scan_job_id) {
                        if updated_job.finished {
                            debug!("Library scan ({}) completed. Proceeding with metadata write.", scan_job_id);
                            break;
                        }
                    } else {
                        // Job no longer exists or error checking
                        debug!("Library scan ({}) is no longer running. Proceeding with metadata write.", scan_job_id);
                        break;
                    }

                    thread::sleep(Duration::from_millis(CHECK_INTERVAL_MS));
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::should_lookup_mbids;
    use crate::data::artist::Artist;
    use crate::data::Identifier;

    fn artist_with_mbid(name: &str, mbid: &str) -> Artist {
        let mut a = Artist {
            id: Identifier::Numeric(1),
            name: name.to_string(),
            is_multi: false,
            metadata: None,
        };
        a.add_mbid(mbid.to_string());
        a
    }

    // --- partial match flag survives clear_metadata ---

    /// Regression test: before the fix, clear_metadata() was called on multi/partial
    /// artists and then the code tried to write is_partial_match to the now-None
    /// metadata, so the flag was always lost.
    #[test]
    fn partial_match_flag_set_after_clear_metadata() {
        let mut artist = Artist {
            id: Identifier::Numeric(42),
            name: "Artist A & Artist B".to_string(),
            is_multi: false,
            metadata: None,
        };

        // Simulate what update_data_for_artist does for a partial match
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
            artist.metadata.as_ref().map_or(false, |m| m.is_partial_match),
            "is_partial_match must survive clear_metadata"
        );
    }

    // --- artists with existing MBIDs skip the lookup branch ---

    #[test]
    fn artist_already_has_mbid_preserves_it() {
        let artist = artist_with_mbid("Known Artist", "abc-123");
        let has_mbid = artist.metadata.as_ref().map_or(false, |m| !m.mbid.is_empty());
        assert!(has_mbid, "MBID should be set by helper");

        // Control flow: has_mbid == true -> skip lookup branch
        let would_skip_lookup = has_mbid;
        assert!(would_skip_lookup);
    }

    #[test]
    fn should_lookup_mbids_for_single_artist_without_mbid() {
        let artist = Artist {
            id: Identifier::Numeric(7),
            name: "Lookup Candidate".to_string(),
            is_multi: false,
            metadata: None,
        };

        assert!(should_lookup_mbids(&artist));
    }

    #[test]
    fn should_not_lookup_mbids_for_multi_artist_without_mbid() {
        let artist = Artist {
            id: Identifier::Numeric(8),
            name: "Artist A & Artist B".to_string(),
            is_multi: true,
            metadata: None,
        };

        assert!(!should_lookup_mbids(&artist));
    }

    #[test]
    fn should_not_lookup_mbids_when_mbid_exists() {
        let artist = artist_with_mbid("Known Artist", "mbid-999");
        assert!(!should_lookup_mbids(&artist));
    }

    // --- multi-artist detection clears metadata ---

    #[test]
    fn multi_artist_marked_and_metadata_cleared() {
        let mut artist = artist_with_mbid("A & B", "id1");
        artist.add_mbid("id2".to_string()); // second MBID makes it multi

        let mbid_count = artist.metadata.as_ref().map_or(0, |m| m.mbid.len());
        assert_eq!(mbid_count, 2);

        if mbid_count > 1 {
            artist.is_multi = true;
            artist.clear_metadata();
        }

        assert!(artist.is_multi);
        // Metadata should be cleared (no leftover MBIDs)
        let leftover_mbids = artist.metadata.as_ref().map_or(0, |m| m.mbid.len());
        assert_eq!(leftover_mbids, 0, "clear_metadata should wipe MBIDs");
    }
}
