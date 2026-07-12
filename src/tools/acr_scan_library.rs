//! audiocontrol_scan_library - Scan and update music library from MPD or LMS
//!
//! ## Overview
//! This tool connects to a Music Player Daemon (MPD) or Lyrion Music Server (LMS)
//! and performs a full library refresh. It is useful for:
//! - Periodically updating music library metadata
//! - Verifying server connectivity
//! - Monitoring library size and statistics
//! - Integration with cron jobs or systemd timers
//!
//! ## Behavior
//!
//! ### Metadata Updater Coordination (Two-Way)
//! The tool implements **bidirectional coordination** with metadata updater background
//! jobs to ensure smooth concurrent operation without database write conflicts:
//!
//! **When scan tool starts:**
//! - Waits for album updater to complete (if running)
//! - Waits for artist updater to complete (if running)
//! - Registers itself as a "library_scan_mpd" or "library_scan_lms" job
//! - Performs the library refresh while holding this job active
//! - Completes the job when finished
//!
//! **When metadata updaters run:**
//! - Both album and artist updaters check if a library scan is active
//! - If a scan is running, they wait up to 5 minutes for it to complete
//! - Only attempt database writes after the scan finishes
//! - This prevents "readonly database" errors from write conflicts
//!
//! **Coordination flow:**
//! ```
//! audiocontrol_scan_library starts
//!   ↓
//! Wait for album_genre_update job (if running)
//!   ↓
//! Wait for artist_metadata_update job (if running)
//!   ↓
//! Register library_scan_mpd/library_scan_lms job
//!   ↓
//! Perform refresh_library() [database locked for read/write]
//!   ↓
//! [album_updater and artist_updater detect scan running and wait]
//!   ↓
//! Complete library_scan job
//!   ↓
//! [album_updater and artist_updater resume and write metadata]
//! ```
//!
//! **Benefits:**
//! - No readonly database errors when updaters run during scans
//! - Metadata updates don't interfere with library scanning
//! - Automatic, transparent coordination without configuration
//! - Graceful timeouts and error handling
//!
//! ### Library Scanning
//! Once all metadata updater checks are complete, the tool:
//! 1. Connects to the specified music server (MPD or LMS)
//! 2. Performs a full refresh_library() call (blocking)
//! 3. Reports completion time and optional statistics
//!
//! ### Statistics Output (with --stats flag)
//! When enabled, displays:
//! - Total number of albums
//! - Total number of artists
//! - Total number of tracks
//! - List of up to 20 first artists
//!
//! ## Exit Codes
//! - 0: Success
//! - 1: Error (connection failure, invalid server type, etc.)
//!
//! ## Logging
//! The tool uses Rust's log/env_logger framework:
//! - Default level: INFO
//! - Override with RUST_LOG environment variable:
//!   - RUST_LOG=debug for detailed output
//!   - RUST_LOG=warn for warnings only
//!   - RUST_LOG=error for errors only
//!
//! ## Typical Output Flow
//! ```
//! [INFO] Connecting to MPD server at localhost:6600
//! [INFO] No album updater job detected. Database writes are available.
//! [INFO] No artist updater job detected. Database writes are available.
//! [INFO] Scanning MPD library...
//! [INFO] Library scan completed in 12.34s
//! [INFO] Library Statistics:
//! [INFO]   Total Albums: 2543
//! [INFO]   Total Artists: 1205
//! [INFO]   Total Tracks: 28567
//! [INFO]   Artists:
//! [INFO]     - Pink Floyd
//! [INFO]     - Led Zeppelin
//! ```
//!
//! If album or artist updaters were running:
//! ```
//! [INFO] Connecting to MPD server at localhost:6600
//! [INFO] Album updater job is running. Allowing database writes to complete...
//! [INFO] Job progress: Starting genre update for 100 albums
//! [INFO] Album updater progress: Processed 25/100 albums, updated 12
//! [INFO] Album updater job completed. Proceeding with library scan.
//! [INFO] Artist updater job is running. Allowing database writes to complete...
//! [INFO] Job progress: Fetching artist metadata from external sources
//! [INFO] Artist updater progress: Processed 150/500 artists
//! [INFO] Artist updater job completed. Proceeding with library scan.
//! [INFO] Scanning MPD library...
//! [INFO] Library scan completed in 12.34s
//! ```

use clap::Parser;
use log::{info, error, warn};
use std::error::Error;
use std::time::Instant;
use std::thread;
use std::time::Duration;

use audiocontrol::data::LibraryInterface;
use audiocontrol::players::mpd::library::MPDLibrary;
use audiocontrol::players::MPDPlayerController;
use audiocontrol::players::lms::library::LMSLibrary;
use audiocontrol::helpers::background_jobs;

/// Scan and update music library from MPD or LMS
#[derive(Parser)]
#[clap(author, version, about)]
struct Cli {
    /// Music server type: "mpd" or "lms"
    #[clap(short = 't', long, default_value = "mpd")]
    server_type: String,

    /// Server hostname or IP address
    #[clap(short = 'H', long, default_value = "localhost")]
    host: String,

    /// Server port
    #[clap(short, long)]
    port: Option<u16>,

    /// Display statistics after scanning
    #[clap(short = 's', long)]
    stats: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Initialize logging
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    env_logger::Builder::from_default_env()
        .format_timestamp_millis()
        .init();

    let cli = Cli::parse();

    match cli.server_type.to_lowercase().as_str() {
        "mpd" => scan_mpd_library(&cli)?,
        "lms" => scan_lms_library(&cli)?,
        other => {
            error!("Unknown server type: {}. Use 'mpd' or 'lms'", other);
            std::process::exit(1);
        }
    }

    Ok(())
}

fn scan_mpd_library(cli: &Cli) -> Result<(), Box<dyn Error>> {
    let port = cli.port.unwrap_or(6600);
    const LIBRARY_SCAN_JOB_ID: &str = "library_scan_mpd";

    info!("Connecting to MPD server at {}:{}", cli.host, port);

    // Check if metadata updater jobs are running and wait for them
    check_and_allow_album_updater_writes();
    check_and_allow_artist_updater_writes();

    // Register the library scan job to prevent metadata writers from interfering
    if let Err(e) = background_jobs::register_job(LIBRARY_SCAN_JOB_ID.to_string(), "MPD Library Scan".to_string()) {
        warn!("Failed to register library scan job: {}. Proceeding anyway.", e);
    }

    info!("Scanning MPD library...");
    let start = Instant::now();

    // Create MPD controller and library
    let controller = std::sync::Arc::new(
        MPDPlayerController::with_connection(&cli.host, port)
    );
    let library = MPDLibrary::with_connection(&cli.host, port, controller);

    // Refresh the library (blocking call)
    library.refresh_library()?;

    let elapsed = start.elapsed();
    info!("Library scan completed in {:.2}s", elapsed.as_secs_f64());

    if cli.stats {
        display_stats(&library);
    }

    // Complete the library scan job
    if let Err(e) = background_jobs::complete_job(LIBRARY_SCAN_JOB_ID) {
        warn!("Failed to complete library scan job: {}. This may affect coordination.", e);
    }

    Ok(())
}

fn scan_lms_library(cli: &Cli) -> Result<(), Box<dyn Error>> {
    let port = cli.port.unwrap_or(9000);
    const LIBRARY_SCAN_JOB_ID: &str = "library_scan_lms";

    info!("Connecting to LMS server at {}:{}", cli.host, port);

    // Check if metadata updater jobs are running and wait for them
    check_and_allow_album_updater_writes();
    check_and_allow_artist_updater_writes();

    // Register the library scan job to prevent metadata writers from interfering
    if let Err(e) = background_jobs::register_job(LIBRARY_SCAN_JOB_ID.to_string(), "LMS Library Scan".to_string()) {
        warn!("Failed to register library scan job: {}. Proceeding anyway.", e);
    }

    info!("Scanning LMS library...");
    let start = Instant::now();

    // Create LMS library and scan
    let library = LMSLibrary::with_connection(&cli.host, port);

    // Refresh the library (blocking call)
    library.refresh_library()?;

    let elapsed = start.elapsed();
    info!("Library scan completed in {:.2}s", elapsed.as_secs_f64());

    if cli.stats {
        display_stats(&library);
    }

    // Complete the library scan job
    if let Err(e) = background_jobs::complete_job(LIBRARY_SCAN_JOB_ID) {
        warn!("Failed to complete library scan job: {}. This may affect coordination.", e);
    }

    Ok(())
}

/// Check if the album_updater background job is running and wait for it to complete.
///
/// ## Purpose
/// This function ensures that genre metadata database writes from the album updater
/// background job are not blocked or interfered with by the library scan operation.
/// It implements a polling-based coordination mechanism.
///
/// ## Behavior
///
/// ### When album_updater job is RUNNING (job.finished == false):
/// 1. Logs that updater is running and scan will wait
/// 2. Logs initial progress message from the job (if available)
/// 3. Enters polling loop:
///    - Checks job status every 500ms (CHECK_INTERVAL_MS)
///    - Logs any progress updates to help user understand what updater is doing
///    - Exits loop when:
///      - Job finishes (job.finished == true)
///      - Job is removed from registry (Ok(None))
///      - Timeout is exceeded (300 seconds / MAX_WAIT_SECS)
///      - Error occurs while checking status
/// 4. Logs appropriate message before returning
///
/// ### When album_updater job is NOT RUNNING or doesn't exist:
/// - Logs "No album updater job detected"
/// - Returns immediately (no waiting)
///
/// ### On error checking job status:
/// - Logs warning with error details
/// - Returns immediately without blocking
///
/// ## Timeouts and Limits
/// - Maximum wait: 300 seconds (5 minutes)
/// - If exceeded: Warns user and proceeds anyway to avoid blocking indefinitely
/// - Polling interval: 500ms (100 checks per 50 seconds)
///
/// ## Logging
/// All status changes and progress updates are logged with INFO level for normal
/// operation, WARN level for timeouts and errors. This allows operators to see
/// coordination in action and diagnose any issues.
///
/// ## Notes
/// - This is a blocking operation; the calling thread will sleep during polling
/// - No exceptions or early returns; always allows scan to proceed eventually
/// - Safe to call multiple times (each call is independent)
fn check_and_allow_album_updater_writes() {
    const ALBUM_UPDATER_JOB_ID: &str = "album_genre_update";
    const MAX_WAIT_SECS: u64 = 300; // Wait up to 5 minutes for the updater to complete
    const CHECK_INTERVAL_MS: u64 = 500; // Check every 500ms

    // Check if album_updater job is currently running
    match background_jobs::get_job(ALBUM_UPDATER_JOB_ID) {
        Ok(Some(job)) => {
            if !job.finished {
                info!("Album updater job is running. Allowing database writes to complete...");
                if let Some(progress) = &job.progress {
                    info!("Job progress: {}", progress);
                }

                // Wait for the job to complete, with timeout
                let start = Instant::now();
                let timeout = Duration::from_secs(MAX_WAIT_SECS);

                loop {
                    if start.elapsed() > timeout {
                        warn!(
                            "Album updater job did not complete within {} seconds. Proceeding with library scan.",
                            MAX_WAIT_SECS
                        );
                        break;
                    }

                    match background_jobs::get_job(ALBUM_UPDATER_JOB_ID) {
                        Ok(Some(updated_job)) => {
                            if updated_job.finished {
                                info!("Album updater job completed. Proceeding with library scan.");
                                break;
                            }
                            // Job still running, log progress and continue waiting
                            if let Some(msg) = &updated_job.progress {
                                info!("Album updater progress: {}", msg);
                            }
                        }
                        Ok(None) => {
                            // Job no longer exists (completed and removed)
                            info!("Album updater job completed and removed. Proceeding with library scan.");
                            break;
                        }
                        Err(e) => {
                            warn!("Error checking album updater job status: {}. Proceeding with library scan.", e);
                            break;
                        }
                    }

                    thread::sleep(Duration::from_millis(CHECK_INTERVAL_MS));
                }
            }
        }
        Ok(None) => {
            // No album updater job currently running
            info!("No album updater job detected. Database writes are available.");
        }
        Err(e) => {
            warn!("Error checking for album updater job: {}. Proceeding with library scan.", e);
        }
    }
}

/// Check if the artist_updater background job is running and wait for it to complete.
///
/// ## Purpose
/// This function ensures that artist metadata database writes from the artist updater
/// background job are not blocked or interfered with by the library scan operation.
/// It implements a polling-based coordination mechanism.
///
/// ## Behavior
///
/// ### When artist_updater job is RUNNING (job.finished == false):
/// 1. Logs that updater is running and scan will wait
/// 2. Logs initial progress message from the job (if available)
/// 3. Enters polling loop:
///    - Checks job status every 500ms (CHECK_INTERVAL_MS)
///    - Logs any progress updates to help user understand what updater is doing
///    - Exits loop when:
///      - Job finishes (job.finished == true)
///      - Job is removed from registry (Ok(None))
///      - Timeout is exceeded (300 seconds / MAX_WAIT_SECS)
///      - Error occurs while checking status
/// 4. Logs appropriate message before returning
///
/// ### When artist_updater job is NOT RUNNING or doesn't exist:
/// - Logs "No artist updater job detected"
/// - Returns immediately (no waiting)
///
/// ### On error checking job status:
/// - Logs warning with error details
/// - Returns immediately without blocking
///
/// ## Timeouts and Limits
/// - Maximum wait: 300 seconds (5 minutes)
/// - If exceeded: Warns user and proceeds anyway to avoid blocking indefinitely
/// - Polling interval: 500ms (100 checks per 50 seconds)
///
/// ## Logging
/// All status changes and progress updates are logged with INFO level for normal
/// operation, WARN level for timeouts and errors. This allows operators to see
/// coordination in action and diagnose any issues.
///
/// ## Notes
/// - This is a blocking operation; the calling thread will sleep during polling
/// - No exceptions or early returns; always allows scan to proceed eventually
/// - Safe to call multiple times (each call is independent)
fn check_and_allow_artist_updater_writes() {
    const ARTIST_UPDATER_JOB_ID: &str = "artist_metadata_update";
    const MAX_WAIT_SECS: u64 = 300; // Wait up to 5 minutes for the updater to complete
    const CHECK_INTERVAL_MS: u64 = 500; // Check every 500ms

    // Check if artist_updater job is currently running
    match background_jobs::get_job(ARTIST_UPDATER_JOB_ID) {
        Ok(Some(job)) => {
            if !job.finished {
                info!("Artist updater job is running. Allowing database writes to complete...");
                if let Some(progress) = &job.progress {
                    info!("Job progress: {}", progress);
                }

                // Wait for the job to complete, with timeout
                let start = Instant::now();
                let timeout = Duration::from_secs(MAX_WAIT_SECS);

                loop {
                    if start.elapsed() > timeout {
                        warn!(
                            "Artist updater job did not complete within {} seconds. Proceeding with library scan.",
                            MAX_WAIT_SECS
                        );
                        break;
                    }

                    match background_jobs::get_job(ARTIST_UPDATER_JOB_ID) {
                        Ok(Some(updated_job)) => {
                            if updated_job.finished {
                                info!("Artist updater job completed. Proceeding with library scan.");
                                break;
                            }
                            // Job still running, log progress and continue waiting
                            if let Some(msg) = &updated_job.progress {
                                info!("Artist updater progress: {}", msg);
                            }
                        }
                        Ok(None) => {
                            // Job no longer exists (completed and removed)
                            info!("Artist updater job completed and removed. Proceeding with library scan.");
                            break;
                        }
                        Err(e) => {
                            warn!("Error checking artist updater job status: {}. Proceeding with library scan.", e);
                            break;
                        }
                    }

                    thread::sleep(Duration::from_millis(CHECK_INTERVAL_MS));
                }
            }
        }
        Ok(None) => {
            // No artist updater job currently running
            info!("No artist updater job detected. Database writes are available.");
        }
        Err(e) => {
            warn!("Error checking for artist updater job: {}. Proceeding with library scan.", e);
        }
    }
}

fn display_stats<T: LibraryInterface>(library: &T) {
    info!("Library Statistics:");

    let albums = library.get_albums();
    let artists = library.get_artists();

    info!("  Total Albums: {}", albums.len());
    info!("  Total Artists: {}", artists.len());

    let total_tracks: usize = albums.iter()
        .map(|album| {
            let tracks = album.tracks.lock();
            tracks.len()
        })
        .sum();

    info!("  Total Tracks: {}", total_tracks);

    if !artists.is_empty() && artists.len() <= 20 {
        info!("  Artists:");
        for artist in artists.iter().take(20) {
            info!("    - {}", artist.name);
        }
    }
}


