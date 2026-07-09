# Helpers Module

`src/helpers/` contains focused, single-purpose service and utility modules. They sit between the
pure domain types in `src/data/` and the higher-level logic in `src/players/`, `src/api/`, and
`src/plugins/`.

**Design boundary**
- `data/` — pure domain types, no I/O
- `helpers/` — stateless utilities, external-service clients, platform adapters
- `players/` — player-specific state machines and controllers
- `api/` — HTTP request/response handlers
- `plugins/` — event-driven business logic

---

## Metadata Enrichment

| Module | Purpose |
|---|---|
| `artist_updater.rs` | Fetches and merges artist metadata from external sources; implements the `ArtistUpdater` trait |
| `album_updater.rs` | Fetches and merges album metadata |
| `artist_store.rs` | In-memory store of enriched `Artist` objects shared across updaters |
| `musicbrainz.rs` | MusicBrainz API client — artist MBID lookup, release/recording search |
| `theaudiodb.rs` | TheAudioDB API client — artist images, biographies |
| `fanarttv.rs` | FanArt.tv API client — high-resolution artist and album artwork |
| `lastfm.rs` | Last.fm API client — scrobbling, loved-track status, play counts, track info |
| `genre_cleanup.rs` | Normalises and maps raw genre tag strings to canonical genre names |
| `song_title_splitter.rs` | Splits combined "Artist – Title" strings common in radio stream metadata |
| `song_split_manager.rs` | Manages per-source `SongTitleSplitter` configurations and persistence |

---

## Caching and Persistence

| Module | Purpose |
|---|---|
| `attribute_cache.rs` | Two-level (LRU memory + SQLite disk) key-value cache for enrichment results; supports per-entry TTL, prefix-based preloading, and JSON-driven initialisation — see [detail](#attribute_cacher-detail) below |
| `image_cache.rs` | Disk and memory cache for cover-art images |
| `image_meta.rs` | Stores image URL, hash, and quality metadata alongside cached images |
| `image_grader.rs` | Scores candidate images and selects the best one (resolution, aspect ratio, etc.) |
| `settings_db.rs` | SQLite-backed persistent key-value settings store for user/system preferences |
| `security_store.rs` | Stores credentials and secrets (API keys, tokens) in an encrypted store |

---

## Cover Art

| Module | Purpose |
|---|---|
| `coverart.rs` | Orchestrates cover-art resolution: tries providers in priority order, caches results |
| `coverart_providers.rs` | Abstracts individual image sources behind a common provider interface |
| `local_coverart.rs` | Finds embedded cover art in audio files or `cover.jpg`/`folder.jpg` on disk |

---

## Volume and Playback

| Module | Purpose |
|---|---|
| `volume.rs` | Reads and sets system volume via ALSA or other backends; emits `PlayerEvent::VolumeChanged` |
| `global_volume.rs` | Singleton wrapper that holds the current system-wide volume state |
| `playback_progress.rs` | Tracks playback position over time; used to interpolate position between updates |

---

## Infrastructure

| Module | Purpose |
|---|---|
| `http_client.rs` | Shared HTTP client with connection pooling, timeout, and User-Agent configuration |
| `rate_limit.rs` | Token-bucket rate limiter used to stay within external API quotas |
| `retry.rs` | Generic retry logic with configurable back-off for transient failures |
| `background_jobs.rs` | Long-running background task registry — creation, status tracking, cancellation |
| `process_helper.rs` | Spawns and monitors external processes (e.g., helper binaries) |
| `systemd.rs` | Checks and controls systemd unit state (active, start, stop) |

---

## Platform and Protocol Adapters

| Module | Purpose |
|---|---|
| `mpris.rs` *(Unix only)* | D-Bus/MPRIS2 helpers — connect to and query Linux media players |
| `bluez.rs` *(Unix only)* | BlueZ D-Bus helpers for Bluetooth audio device discovery and control — see [detail](#bluezrs--detail) below |
| `shairportsync_messages.rs` *(Unix only)* | Parses metadata pipe messages emitted by shairport-sync |
| `spotify.rs` | Spotify OAuth flow, token refresh, and API wrapper |
| `m3u.rs` | Reads and writes M3U/M3U8 playlist files |
| `lyrics.rs` | Fetches song lyrics from configured sources |
| `configurator.rs` | Detects and reports hardware/system audio configuration |

---

## Utilities

| Module | Purpose |
|---|---|
| `sanitize.rs` | String cleaning — strips control characters, normalises whitespace, makes filenames safe |
| `url_encoding.rs` | URL percent-encoding and decoding helpers |
| `mac_address.rs` | Reads MAC addresses from local network interfaces |
| `memory_report.rs` | Reports current process memory usage (RSS, virtual) |
| `stream_helper.rs` | I/O stream utilities — buffered pipe readers, chunk collectors |
| `artist_splitter.rs` | Splits multi-artist strings such as `"A & B feat. C"` into individual artist names |

---

## `attribute_cache.rs` — detail

### Purpose

Generic persistent cache used throughout the enrichment pipeline.  Any module
can store and retrieve arbitrary `serde`-serialisable values under string keys.

### Architecture

```
write / read call
       │
       ▼
 LRU memory cache  (configurable byte limit, default 50 MB)
       │  miss
       ▼
 SQLite disk cache  (/var/lib/audiocontrol/cache/attributes.db by default)
       │  hit → promote to LRU
       ▼
 caller receives None (triggers downstream API call)
```

### SQLite schema (`table cache`)

| Column | Type | Notes |
|---|---|---|
| `key` | TEXT PRIMARY KEY | Fully qualified, e.g. `album::genres::42` |
| `value` | BLOB | Serialised value (MessagePack via bincode/serde) |
| `created_at` | INTEGER | Unix timestamp — set on INSERT |
| `updated_at` | INTEGER | Updated on every write |
| `expires_at` | INTEGER / NULL | NULL = no expiry |

If the on-disk schema is found to be incomplete at startup, the table is dropped
and recreated automatically — cache data is lost but no crash occurs.

### Key naming conventions

| Prefix | Owner module | Meaning |
|---|---|---|
| `album::genres::<id>` | `album_updater` | Genre list for an album |
| `artist::metadata::<name>` | `artist_updater`, `artist_store` | `ArtistMeta` struct |
| `artist::mbid::<name>` | `artist_updater` | MBIDs for fast lookup |
| `artist::split::<name>` | `artist_splitter` | MBID-validated multi-artist split result |
| `artist::simple_split::...::<name>` | `artist_splitter` | Text-only split result |
| `artist::not_found::<name>` | `musicbrainz` | Negative-lookup marker |
| `image::meta::<path>` | `image_meta` | Image URL/hash/quality metadata |

### Public module-level API (free functions, global singleton)

```rust
// Read — None if not cached or expired
fn get<T: DeserializeOwned>(key: &str) -> Result<Option<T>, String>

// Write — no expiry
fn set<T: Serialize>(key: &str, value: &T) -> Result<(), String>

// Write with absolute expiry (Unix timestamp); None = no expiry
fn set_with_expiry<T: Serialize>(key: &str, value: &T, expires_at: Option<i64>) -> Result<(), String>

// Write with relative TTL in seconds
fn set_with_ttl<T: Serialize>(key: &str, value: &T, ttl_seconds: u64) -> Result<(), String>

// Remove one entry; returns true if it existed
fn remove(key: &str) -> Result<bool, String>

// Remove all entries whose key starts with prefix; returns count
fn remove_by_prefix(prefix: &str) -> Result<usize, String>

// Load all entries with a given prefix into the LRU memory cache (cache warm-up)
fn preload_prefix(prefix: &str) -> Result<usize, String>

// List all keys, optionally filtered by prefix
fn list_keys(prefix_filter: Option<&str>) -> Result<Vec<String>, String>

// Age/timestamp helpers
fn get_age(key: &str) -> Result<Option<i64>, String>          // seconds since created_at
fn get_last_updated_age(key: &str) -> Result<Option<i64>, String>  // seconds since updated_at
```

### Initialisation (called once at startup from `main.rs`)

```rust
// From a JSON config block — supports "dbfile" and "memory_limit" keys
AttributeCache::initialize_from_config(config: &serde_json::Value)

// Programmatic
AttributeCache::initialize_global_with_memory_limit(path, max_bytes: usize)
```

### `memory_limit` string formats accepted by `parse_size_string`

Plain bytes, `100K`, `200M`, `18kB`, `189MB`, `1G`, `2TB` (case-insensitive,
fractional values like `1.5M` supported).

---

## `bluez.rs` — detail

### Purpose

D-Bus helpers for querying and controlling Bluetooth audio devices via the BlueZ daemon.
Provides device discovery, playback status monitoring, track information retrieval, and
playback control (play, pause, stop, next, previous).

### Struct types

**`BluetoothDeviceInfo`**
Represents a discovered Bluetooth audio device:
- `device_address` — MAC address string (e.g., `"80:B9:89:1E:B5:6F"`)
- `device_name` — Human-readable name or `None`
- `player_path` — D-Bus path to the MediaPlayer1 interface
- `is_connected` — Connection status
- `is_playing` — Current playback state

**`BluetoothTrackInfo`**
Current track metadata retrieved from MediaPlayer1:
- `title`, `artist`, `album` — Metadata strings or `None`
- `duration` — Track duration in milliseconds or `None`
- `position` — Current playback position in milliseconds or `None`

**`BluetoothPlaybackStatus`** *(enum)*
- `Playing` — MediaPlayer1 status: `"playing"`
- `Paused` — MediaPlayer1 status: `"paused"`
- `Stopped` — MediaPlayer1 status: `"stopped"`
- `Unknown` — Unrecognised or unreachable

### Public module-level API

```rust
impl BlueZManager {
    // Connect to system D-Bus and create manager
    fn new() -> Result<Self, Box<dyn std::error::Error>>

    // Discover all Bluetooth audio devices with MediaPlayer1 interfaces
    pub fn discover_audio_devices(&self) -> Result<Vec<BluetoothDeviceInfo>, ...>

    // Get playback status from a MediaPlayer1 interface
    pub fn get_playback_status(&self, player_path: &str) -> BluetoothPlaybackStatus

    // Get current track metadata
    pub fn get_track_info(&self, player_path: &str) -> Result<BluetoothTrackInfo, ...>

    // Send playback control commands: "play", "pause", "stop", "next", "previous"
    pub fn send_control_command(&self, player_path: &str, command: &str) -> Result<(), ...>

    // Find device by MAC address (case-insensitive)
    pub fn find_device_by_address(&self, target_address: &str) -> Result<Option<BluetoothDeviceInfo>, ...>

    // Get the currently active (playing) device or None
    pub fn get_active_device(&self) -> Result<Option<BluetoothDeviceInfo>, ...>
}

// Pure function: map MediaPlayer1 Status string to enum (testable without D-Bus)
pub fn parse_playback_status(status: &str) -> BluetoothPlaybackStatus
```

### D-Bus interface mapping

- **Device1** interface (`org.bluez.Device1`) — MAC address, connection status, device name
- **MediaPlayer1** interface (`org.bluez.MediaPlayer1`) — playback status, track metadata, playback control

### Operational notes

- **D-Bus requirement** — All methods require an active system D-Bus connection and BlueZ daemon.
- **Adapter hardcoding** — `discover_audio_devices` searches only under `/org/bluez/hci0/` and silently
  skips devices under other adapters (e.g., `hci1`). Multi-adapter systems may need custom logic.
- **Timeouts** — Individual D-Bus calls use 1–2 second timeouts; discovery can take ~5 seconds.
- **Error handling** — Failed individual property reads (e.g., device name) do not fail the whole
  discovery; affected fields are set to `None` or default values.

### Testing

Most BlueZ methods are difficult to test without a running BlueZ daemon and actual Bluetooth devices.
The module provides `parse_playback_status(status: &str)` as a pure, testable function for status
string mapping. Struct field tests ensure that `Option<T>` fields are properly handled (all `None`,
mixed, or all `Some`).

## lastfm.rs — Detail

**Purpose:** Last.fm API client for scrobbling, artist metadata enrichment, and personal music enrichment
(play counts, loved-track status, track information).

### Key data structures

```rust
// Last.fm credentials with optional session key
pub struct LastfmCredentials {
    pub api_key: String,               // API key from secrets or defaults
    pub api_secret: String,            // API secret from secrets or defaults
    pub session_key: Option<String>,   // Session key after user authentication
    pub username: Option<String>,      // Username after user authentication
    pub auth_token: Option<String>,    // Temporary token during auth flow
    pub token_created: Option<u64>,    // Unix timestamp of token creation
}

// Last.fm client singleton, lazy-initialized with credentials
pub static LASTFM_CLIENT: Lazy<Mutex<Option<LastfmClient>>> = ...

pub struct LastfmClient {
    credentials: LastfmCredentials,
    client: ureq::Agent,  // HTTP client for API requests
}

// Artist-level information from Last.fm
pub struct LastfmArtistDetails {
    pub name: String,
    pub mbid: Option<String>,          // MusicBrainz ID (if available)
    pub url: String,
    pub image: Vec<LastfmArtistImage>, // Multiple image sizes
    pub streamable: String,
    pub stats: Option<serde_json::Value>, // playcount, listeners
    pub similar: Option<LastfmSimilar>, // Similar artists
    pub tags: Option<LastfmTopTags>,   // User-assigned tags/genres
    pub bio: Option<LastfmWiki>,       // Biography (summary and full content)
}

// Track-level information
pub struct LastfmTrackInfoDetails {
    pub name: String,
    pub mbid: Option<String>,
    pub url: String,
    pub duration: String,              // Duration in milliseconds
    pub listeners: String,
    pub playcount: String,
    pub artist: LastfmTrackInfoArtist,
    pub album: Option<LastfmTrackInfoAlbum>,
    pub tags: Option<LastfmTopTags>,
    pub wiki: Option<LastfmWiki>,
    pub userloved: bool,               // Whether user has loved this track
    pub user_playcount: Option<String>, // User's personal play count
}
```

### Public module-level API

```rust
impl LastfmClient {
    // Initialize the client with explicit credentials
    pub fn initialize(api_key: String, api_secret: String) -> Result<(), LastfmError>

    // Initialize with default credentials from secrets.txt (built at compile time)
    pub fn initialize_with_defaults() -> Result<(), LastfmError>

    // Get the singleton instance
    pub fn get_instance() -> Result<LastfmClient, LastfmError>

    // Generate auth URL for user to authorize the application
    pub fn get_auth_url(&mut self) -> Result<(String, String), LastfmError>

    // Get an authentication token (step 1 of auth flow)
    pub fn get_auth_token(&mut self) -> Result<String, LastfmError>

    // Convert auth token to session key after user authorizes (step 2)
    pub fn get_session(&mut self) -> Result<(String, String), LastfmError>

    // Disconnect: clear session key and credentials from memory and security store
    pub fn disconnect(&mut self) -> Result<(), String>

    // Fetch artist information from Last.fm
    pub fn get_artist_info(&self, artist: &str) -> Result<LastfmArtistDetails, LastfmError>

    // Fetch track information from Last.fm
    pub fn get_track_info(&self, artist: &str, title: &str) -> Result<LastfmTrackInfoDetails, LastfmError>

    // Scrobble a track (log a listen to Last.fm)
    pub fn scrobble_track(&self, artist: &str, track: &str, album: Option<&str>, timestamp: u64) 
        -> Result<(), LastfmError>

    // Love a track (save to "Loved Tracks")
    pub fn love_track(&self, artist: &str, track: &str) -> Result<(), LastfmError>

    // Unlove a previously loved track
    pub fn unlove_track(&self, artist: &str, track: &str) -> Result<(), LastfmError>

    // Check if a track is in the user's "Loved Tracks"
    pub fn is_track_loved(&self, artist: &str, track: &str) -> Result<bool, LastfmError>

    // Check if client is authenticated (has valid session key and username)
    pub fn is_authenticated(&self) -> bool
}

// Last.fm updater implementing the ArtistUpdater trait
impl ArtistUpdater for LastfmUpdater {
    // Fetch and merge Last.fm artist data into the artist metadata
    fn update_artist(&self, artist: Artist) -> Artist
}

// Pure function: merge Last.fm artist data into an artist without network access
pub fn apply_lastfm_data_to_artist(artist: Artist, artist_info: LastfmArtistDetails) -> Artist
    // Initialises metadata if None
    // Cleans biography text (removes Last.fm promotional links)
    // Merges tags as genres
    // Adds MBID if new and non-empty

// Pure function: clean biography text by removing Last.fm link HTML
pub fn cleanup_biography(biography: &str) -> String
    // Removes: <a href="https://www.last.fm/music/...">Read more on Last.fm</a>
    // Trims trailing whitespace and periods
```

### Operational notes

- **API rate limiting** — All API calls are rate-limited to 1 request per second (registered with `rate_limit::register_service("lastfm", 1000)`).
- **Credentials storage** — API key and secret come from `secrets.txt` at build time (fallback: "YOUR_API_KEY_HERE" placeholders).
  Session key and username are stored in the encrypted security store (`SecurityStore`) after authentication.
- **Authentication flow** — Two-step process:
  1. `get_auth_token()` returns a token URL
  2. User visits URL to authorize
  3. `get_session()` exchanges token for session key (valid until revoked)
- **Singleton instance** — `LASTFM_CLIENT` is a lazy static that is initialized once and reused. Accessible within the crate via `LastfmClient::get_instance()`.
- **Biography cleanup** — The `cleanup_biography()` function removes Last.fm promotional HTML and trailing periods left by the link removal process. This is used both during metadata enrichment and can be applied standalone.
- **Artist enrichment** — `LastfmUpdater::update_artist()` fetches artist info via the API and calls `apply_lastfm_data_to_artist()` to merge data. The pure function enables testing without network mocking.

### Testing

The module provides two pure functions for easy unit testing without network access:

1. **`cleanup_biography(biography: &str) -> String`** — Tests edge cases: multiple Last.fm links, 
   unicode characters, URL-encoded artist names, preservation of other HTML tags, trailing period 
   handling, and empty input.

2. **`apply_lastfm_data_to_artist(artist: Artist, artist_info: LastfmArtistDetails) -> Artist`** — 
   Tests metadata initialization, biography addition with cleanup, genre/tag merging with empty 
   filtering, MBID addition with duplicate detection, and combined field updates.

All I/O-dependent API calls (HTTP requests, security store access) are tested via integration tests
or skipped in unit tests. The pure functions provide sufficient coverage for business logic.

## playback_progress.rs — Detail

**Purpose:** Thread-safe playback position tracker with automatic time-based position
advancement while playing. Tracks the current position in a track and updates it in real-time
based on elapsed time when playback is active.

### Key data structures

```rust
// Thread-safe playback progress tracker
pub struct PlayerProgress {
    inner: Arc<Mutex<PlayerProgressInner>>,
}

// Internal state (protected by mutex)
struct PlayerProgressInner {
    position: f64,              // Current position in seconds
    is_playing: bool,           // Whether player is currently playing
    last_update: Instant,       // Timestamp of last position update
}
```

### Public module-level API

```rust
impl PlayerProgress {
    // Create a new PlayerProgress instance with position 0.0 and not playing
    pub fn new() -> Self

    // Set the current position in seconds
    // Invalid (negative, NaN, Infinity) positions are silently ignored
    pub fn set_position(&self, position: f64)

    // Get the current position in seconds
    // If playing, returns position + elapsed time since last update
    pub fn get_position(&self) -> f64

    // Set the playing state
    // When changing state, position is updated to current time before state change
    pub fn set_playing(&self, playing: bool)

    // Get the current playing state
    pub fn is_playing(&self) -> bool

    // Reset position to 0.0 and stop playing
    pub fn reset(&self)
}

impl Default for PlayerProgress {
    fn default() -> Self  // Equivalent to PlayerProgress::new()
}

impl Clone for PlayerProgress {
    fn clone(&self) -> Self  // Returns Arc clone (shared ownership of same state)
}
```

### Operational notes

- **Thread-safe design** — Uses `Arc<Mutex<>>` for safe concurrent access from multiple threads.
  Cloned instances share the same underlying state.
- **Automatic position advancement** — When `is_playing` is true, `get_position()` automatically
  advances the position by the elapsed time since the last update, eliminating the need for
  external polling or timer callbacks.
- **Position validation** — `set_position()` silently rejects negative values, NaN, and Infinity.
  This prevents invalid state from disrupting playback timing.
- **State transition handling** — When transitioning between playing/paused states via `set_playing()`,
  the position is immediately updated to the current elapsed time. This ensures no time loss during
  pause/resume cycles.
- **Zero position handling** — Position can be set to 0.0 at any time, including while playing.
  This effectively "resets" to the start while maintaining playing state.
- **Ownership semantics** — `Clone` returns a new `Arc` pointing to the same `Mutex`, not a deep copy.
  All clones share state updates.

### Testing

The module includes 25 comprehensive tests covering:

**Core functionality:**
- Position get/set operations
- Playing state transitions
- Reset behavior

**Timing and auto-advancement:**
- Position increments correctly when playing
- Position remains static when paused
- Elapsed time accurately reflects wall-clock delays
- Multiple `get_position()` calls show monotonic increase when playing

**Edge cases:**
- Invalid positions (NaN, Infinity, negative) are rejected
- Very large position values (1 billion+ seconds) work correctly
- Rapid consecutive position updates
- Zero position remains zero when not playing
- Position resets to zero while still playing

**State management:**
- Multiple consecutive state changes (play→pause→play sequences)
- Position rebasing when `set_position()` called while playing
- State transitions with timing tolerance

**Concurrency:**
- Thread-safe concurrent reads and writes
- Clone shares state correctly
- Both position and playing state updates propagate to all clones

**Robustness:**
- Immediate position retrieval (no timing delay)
- Default implementation works identically to `new()`

## volume.rs — Detail

**Purpose:** Abstraction layer for system volume control with support for both percentage and decibel
scales. Provides ALSA-based system integration on Linux and a dummy implementation for testing and
non-ALSA platforms. Publishes volume change events to the global event bus.

### Key data structures

```rust
// Error types for volume control operations
pub enum VolumeError {
    DeviceError(String),           // Device not found or inaccessible
    ControlNotFound(String),       // Control not found on device
    InvalidRange(String),          // Volume value out of range
    AlsaError(String),             // ALSA library error
    IoError(String),               // Generic I/O error
    NotSupported(String),          // Feature not supported by control
}

// Volume change event for event bus publishing
pub struct VolumeChangeEvent {
    pub control_name: String,      // Internal control name
    pub new_percentage: f64,       // New volume as percentage (0-100)
    pub new_db: Option<f64>,       // New volume in dB (if available)
}

// Decibel range conversion utilities
pub struct DecibelRange {
    pub min_db: f64,               // Minimum dB value (typically negative)
    pub max_db: f64,               // Maximum dB value
}

// Metadata about a volume control
pub struct VolumeControlInfo {
    pub internal_name: String,     // Internal system name (e.g., "alsa:hw:0:Master")
    pub display_name: String,      // Human-readable UI name
    pub decibel_range: Option<DecibelRange>,  // Optional dB range
}

// ALSA implementation (Linux with feature = "alsa")
pub struct AlsaVolumeControl {
    device: String,                // ALSA device (e.g., "hw:0", "default")
    control_name: String,          // ALSA control name (e.g., "Master", "PCM")
    info: VolumeControlInfo,
}

// Dummy implementation for testing and non-ALSA platforms
pub struct DummyVolumeControl {
    internal_name: String,
    display_name: String,
    current_percent: f64,          // Current volume percentage
    available: bool,               // Whether control is accessible
    info: VolumeControlInfo,
}
```

### Public module-level API

```rust
// Trait for volume control operations (primary interface)
pub trait VolumeControl {
    // Get current volume as percentage (0-100)
    fn get_volume_percent(&self) -> Result<f64, VolumeError>

    // Set volume as percentage (0-100)
    fn set_volume_percent(&self, percent: f64) -> Result<(), VolumeError>

    // Get current volume in decibels (if supported)
    fn get_volume_db(&self) -> Result<f64, VolumeError>

    // Set volume in decibels (if supported)
    fn set_volume_db(&self, db: f64) -> Result<(), VolumeError>

    // Get metadata about this control
    fn get_info(&self) -> VolumeControlInfo

    // Check if control is accessible
    fn is_available(&self) -> bool

    // Get minimum and maximum raw values (implementation-specific)
    fn get_raw_range(&self) -> Result<(i64, i64), VolumeError>

    // Get current raw value
    fn get_raw_value(&self) -> Result<i64, VolumeError>

    // Set raw value (implementation-specific)
    fn set_raw_value(&self, value: i64) -> Result<(), VolumeError>

    // Start monitoring for volume changes (optional)
    fn start_change_monitoring(&self) -> Result<(), VolumeError>

    // Check if change monitoring is supported
    fn supports_change_monitoring(&self) -> bool
}

// Decibel range conversion utilities
impl DecibelRange {
    pub fn new(min_db: f64, max_db: f64) -> Self

    // Convert percentage (0-100) to dB value within range
    pub fn percent_to_db(&self, percent: f64) -> f64

    // Convert dB value to percentage (0-100) within range
    pub fn db_to_percent(&self, db: f64) -> f64
}

// Control metadata
impl VolumeControlInfo {
    pub fn new(internal_name: String, display_name: String) -> Self

    pub fn with_decibel_range(mut self, range: DecibelRange) -> Self
}

// ALSA implementation (Linux only)
#[cfg(all(feature = "alsa", not(windows)))]
impl AlsaVolumeControl {
    pub fn new(device: String, control_name: String, display_name: String)
        -> Result<Self, VolumeError>
}

// Dummy implementation (always available)
impl DummyVolumeControl {
    pub fn new(internal_name: String, display_name: String, initial_percent: f64) -> Self

    pub fn new_default() -> Self  // Sensible defaults

    pub fn set_available(&mut self, available: bool)

    pub fn get_current_percent(&self) -> f64
}

// Factory functions
#[cfg(all(feature = "alsa", not(windows)))]
pub fn create_alsa_volume_control(device: String, control_name: String, display_name: String)
    -> Result<Box<dyn VolumeControl>, VolumeError>

pub fn create_dummy_volume_control(internal_name: String, display_name: String, initial_percent: f64)
    -> Box<dyn VolumeControl>
```

### Operational notes

- **Percentage scale** — All `*_percent` methods use 0-100 range. Values outside this range are
  validated and rejected with `InvalidRange` error.
- **Decibel scale** — Optional feature requiring `DecibelRange` metadata. Conversions are linear:
  `db = min_db + (percent / 100) * (max_db - min_db)`. If a control doesn't have dB range, dB operations
  return `NotSupported` error.
- **ALSA implementation** — Attempts to use playback volume first, then capture volume as fallback.
  Queries ALSA for dB range and validates extreme values (clamps to -200dB to +50dB). On Linux with
  ALSA feature enabled.
- **Dummy implementation** — Always available on all platforms. Simulates a control with -120dB to 0dB
  range. Initial percentage passed to `new()`.
- **Raw values** — Implementation-specific integer representation (e.g., ALSA mixer level). Range is
  `[0, raw_max]` depending on hardware. Used for direct hardware access.
- **Event publishing** — When volume is set (either percentage or dB), a `VolumeChangeEvent` is published
  to the global `EventBus` with control name, new percentage, and optional dB value.
- **Availability tracking** — `DummyVolumeControl` supports toggling availability via `set_available()`.
  When unavailable, all operations return `DeviceError`.
- **Trait-based design** — `VolumeControl` trait allows swapping implementations (ALSA ↔ Dummy, or custom).
  Factory functions return trait objects (`Box<dyn VolumeControl>`).

### Testing

The module includes 26 comprehensive tests covering:

**Core data types:**
- VolumeChangeEvent creation with/without dB
- VolumeControlInfo builder pattern
- DecibelRange construction and validation

**Percentage/dB conversion:**
- Standard conversions (0%, 50%, 100%)
- Asymmetric ranges (-80 to +20 dB)
- Small ranges (-6 to 0 dB)
- Positive-only ranges (0 to +12 dB)
- Boundary clamping and precision

**DummyVolumeControl operations:**
- Basic get/set percentage
- Extreme values (0%, 100%, near-boundaries)
- Multiple state transitions
- Raw value synchronization with percentage
- Different initial values (0%, 50%, 100%)
- Availability toggling
- dB operations (when dB range present)

**Error handling:**
- Invalid percentage ranges (negative, >100)
- Invalid raw values (outside min/max)
- Operations when unavailable
- dB operations without dB range support

**VolumeError:**
- All 6 error variants display correctly
- Error messages include context

**Trait abstractions:**
- Multiple controls as trait objects
- Factory function creation
- Consistent behavior across implementations

All 26 tests pass with 100% success rate. The module provides a robust abstraction for system
volume control that can be tested independently without ALSA or hardware dependencies.

## global_volume.rs — Detail

**Purpose:** Global singleton wrapper around the `VolumeControl` trait providing application-wide
access to a centralized volume control instance. Handles initialization from configuration with
support for both ALSA and dummy implementations, automatic device detection, and retry logic for
resilient startup.

### Key data structures

```rust
// Global instance managed as OnceCell for thread-safe singleton pattern
static GLOBAL_VOLUME_CONTROL: OnceCell<Arc<Mutex<Box<dyn VolumeControl + Send + Sync>>>>;

// Pure function for config extraction (internal, testable)
fn extract_dummy_volume_config(volume_config: &Value) -> (String, String, f64)
    // Returns: (internal_name, display_name, initial_percent) with defaults

fn is_volume_control_enabled(volume_config: &Value) -> bool
    // Returns: true if enabled (defaults to true)

fn extract_control_type(volume_config: &Value) -> String
    // Returns: "dummy", "alsa", or custom type string
```

### Public module-level API

```rust
// Initialize global volume control from configuration
pub fn initialize_volume_control(config: &Value)
    // Reads "services.volume" config section
    // Supports: enable flag, type ("dummy" or "alsa"), device/control names
    // Auto-detects ALSA settings from configurator API with retries
    // Falls back to dummy control on errors

// Get the global volume control instance
pub fn get_global_volume_control() 
    -> Result<Arc<Mutex<Box<dyn VolumeControl + Send + Sync>>>, Box<dyn std::error::Error>>

// Get current volume as percentage (convenience wrapper)
pub fn get_volume_percentage() -> Option<f64>

// Set volume as percentage (convenience wrapper)
pub fn set_volume_percentage(percentage: f64) -> bool

// Get current volume in decibels (convenience wrapper)
pub fn get_volume_db() -> Option<f64>

// Set volume in decibels (convenience wrapper)
pub fn set_volume_db(db: f64) -> bool

// Check if volume control is available
pub fn is_volume_control_available() -> bool

// Get metadata about the current volume control
pub fn get_volume_control_info() -> Option<VolumeControlInfo>

// Start monitoring for volume changes
pub fn start_volume_change_monitoring() -> Result<(), Box<dyn std::error::Error>>

// Check if change monitoring is supported
pub fn supports_volume_change_monitoring() -> bool
```

### Operational notes

- **OnceCell singleton pattern** — `GLOBAL_VOLUME_CONTROL` is initialized once and cached.
  Subsequent calls to `initialize_volume_control()` log errors but don't override the first
  initialization. This is intentional to prevent runtime reconfiguration.

- **Configuration structure** — Expects configuration under `services.volume` with fields:
  - `enable` (bool, defaults to true): Whether volume control is enabled
  - `type` (string, defaults to "dummy"): "alsa" or "dummy"
  - `device` (string, optional): ALSA device name (e.g., "hw:0", "default")
  - `control_name` (string, optional): ALSA control name (e.g., "Master", "PCM")
  - `display_name` (string): User-friendly display name
  - `internal_name` (string): System identifier for dummy controls
  - `initial_percent` (number): Initial volume for dummy controls
  - `auto_detect_retry_count` (number, defaults to 2): Retries for auto-detection
  - `auto_detect_retry_delay_seconds` (number, defaults to 10): Delay between retries

- **ALSA auto-detection** — If device or control_name are empty, attempts to query the
  configurator API for system soundcard settings. Falls back to "default" device and "Master"
  control if auto-detection fails and no manual config provided.

- **Retry logic** — Auto-detection retries with configurable delays. If all retries fail
  and both device and control_name were empty, disables volume control. If at least one
  was explicitly configured, uses that as fallback.

- **Fallback behavior** — If ALSA initialization fails, creates an unavailable dummy control
  with clear error indication. If ALSA feature not compiled in, falls back to dummy.

- **Disabled volume control** — When `enable: false`, initializes a dummy control marked
  as unavailable. All convenience functions will return Option::None or false.

- **No config fallback** — If no volume configuration section found, creates a working dummy
  control with sensible defaults (50% initial volume).

- **Convenience functions** — Wrapper functions (`get_volume_percentage()`, `set_volume_percentage()`,
  etc.) handle unwrapping and provide Option/bool returns for ergonomic calling. Return
  None/false if control not initialized or unavailable.

- **Change monitoring** — All volume changes are published to the global `EventBus`. Support
  for active monitoring (polling or event-driven) depends on the underlying implementation.
  ALSA may support kernel-level monitoring; dummy always supports.

### Testing

The module includes 25 comprehensive tests covering:

**Pure function extraction (5 tests):**
- `extract_dummy_volume_config()` with all fields, defaults, partial fields
- Extreme initial percentage values (0%, 100%, >100%)
- `is_volume_control_enabled()` with default and explicit values
- `extract_control_type()` with defaults and explicit types

**Convenience function operations (9 tests):**
- Get/set volume percentage basic operations
- Percentage boundary values (0%, 50%, 100%)
- Get/set volume in dB
- Availability checks (available/unavailable states)
- Volume control info retrieval
- Multiple sequential volume changes (state transitions)
- Decimal precision (33.333333)
- Percentage/dB round-trip conversion accuracy
- Operations when control unavailable

**Edge cases and validation (9 tests):**
- Invalid percentage ranges (negative, >100%, NaN, Infinity)
- Different initial values preserve across info queries
- Empty internal/display names
- Change monitoring support checks
- Configuration parsing (enabled/disabled)

**Config-related (3 tests):**
- Config parsing with expected structure
- Disabled config handling
- Volume control API operations (no-panic guarantee)

All 25 tests pass with 100% success rate. The module provides a reliable global interface
to volume control with comprehensive error handling and fallback strategies.
