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
| `attribute_cache.rs` | Key-value cache (SQLite-backed) for artist/album enrichment results with TTL support |
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
| `bluez.rs` | BlueZ D-Bus helpers for Bluetooth audio device discovery and control |
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
