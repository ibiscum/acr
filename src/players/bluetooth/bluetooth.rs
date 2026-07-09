use crate::players::player_controller::{BasePlayerController, PlayerController};
use crate::data::{PlayerCapability, PlayerCapabilitySet, Song, LoopMode, PlaybackState, PlayerCommand, PlayerState, Track};
use delegate::delegate;
use std::sync::Arc;
use parking_lot::{RwLock, Mutex};
use log::{debug, info, warn, error};
use std::any::Any;
use std::collections::HashMap;
use dbus::blocking::Connection;
use dbus::blocking::stdintf::org_freedesktop_dbus::{Properties, ObjectManager};
use dbus::arg::RefArg;
use std::time::{Duration, SystemTime};
use std::thread;
use std::sync::atomic::Ordering;

const MILLIS_PER_SECOND: f64 = 1000.0;
const BLUETOOTH_SCAN_INTERVAL_STEP_SECS: u64 = 5;
const BLUETOOTH_SCAN_INTERVAL_MAX_SECS: u64 = 60;

pub(crate) fn duration_millis_to_song_seconds(duration_ms: u64) -> f64 {
    duration_ms as f64 / MILLIS_PER_SECOND
}

pub(crate) fn resolve_player_path_transition(
    current_path: Option<String>,
    current_path_is_valid: bool,
    replacement_path: Option<String>,
) -> Option<String> {
    if current_path_is_valid {
        current_path
    } else {
        replacement_path
    }
}

pub(crate) fn should_scan_for_player_path(has_player_path: bool) -> bool {
    !has_player_path
}

pub(crate) fn should_start_scanning_in_start(
    auto_discover_mode: bool,
    has_player_path: bool,
) -> bool {
    auto_discover_mode && should_scan_for_player_path(has_player_path)
}

pub(crate) fn next_scan_interval_secs(current_secs: u64) -> u64 {
    (current_secs + BLUETOOTH_SCAN_INTERVAL_STEP_SECS).min(BLUETOOTH_SCAN_INTERVAL_MAX_SECS)
}

/// Bluetooth player controller implementation
/// This controller interfaces with Bluetooth audio devices via D-Bus using BlueZ MediaPlayer1 interface
pub struct BluetoothPlayerController {
    /// Base controller
    base: BasePlayerController,

    /// D-Bus connection (using Mutex instead of RwLock for thread safety)
    connection: Arc<Mutex<Option<Connection>>>,

    /// Current song information
    current_song: Arc<RwLock<Option<Song>>>,

    /// Current player state
    current_state: Arc<RwLock<PlayerState>>,

    /// Bluetooth device address (MAC address) - None means auto-discover
    device_address: Arc<RwLock<Option<String>>>,

    /// D-Bus object path for the MediaPlayer1 interface
    player_path: Arc<RwLock<Option<String>>>,

    /// Device name (friendly name)
    device_name: Arc<RwLock<Option<String>>>,

    /// True when controller was created without a fixed device address.
    auto_discover_mode: bool,

    /// Background thread handle for device scanning
    scan_thread: Arc<RwLock<Option<std::thread::JoinHandle<()>>>>,

    /// Flag to stop scanning thread
    stop_scanning: Arc<std::sync::atomic::AtomicBool>,

    /// Background thread handle for status polling
    poll_thread: Arc<RwLock<Option<std::thread::JoinHandle<()>>>>,

    /// Flag to stop polling thread
    stop_polling: Arc<std::sync::atomic::AtomicBool>,
}

// Manually implement Clone for BluetoothPlayerController
impl Clone for BluetoothPlayerController {
    fn clone(&self) -> Self {
        BluetoothPlayerController {
            base: self.base.clone(),
            connection: Arc::clone(&self.connection),
            current_song: Arc::clone(&self.current_song),
            current_state: Arc::clone(&self.current_state),
            device_address: Arc::clone(&self.device_address),
            player_path: Arc::clone(&self.player_path),
            device_name: Arc::clone(&self.device_name),
            auto_discover_mode: self.auto_discover_mode,
            scan_thread: Arc::new(RwLock::new(None)),
            stop_scanning: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            poll_thread: Arc::new(RwLock::new(None)),
            stop_polling: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl Drop for BluetoothPlayerController {
    fn drop(&mut self) {
        // Signal both threads to stop
        self.stop_scanning.store(true, Ordering::Relaxed);
        self.stop_polling.store(true, Ordering::Relaxed);

        // Wait for the scanning thread to finish
        {
            let mut guard = self.scan_thread.write();
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }

        // Wait for the polling thread to finish
        {
            let mut guard = self.poll_thread.write();
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }

        debug!("BluetoothPlayerController dropped");
    }
}

impl Default for BluetoothPlayerController {
    fn default() -> Self {
        Self::new()
    }
}

impl BluetoothPlayerController {
    fn ensure_shared_dbus_connection(connection: &Arc<Mutex<Option<Connection>>>) -> bool {
        let mut conn_guard = connection.lock();
        if conn_guard.is_none() {
            match Connection::new_system() {
                Ok(conn) => {
                    debug!("Established D-Bus system connection (shared)");
                    *conn_guard = Some(conn);
                    true
                }
                Err(e) => {
                    error!("Failed to connect to D-Bus system bus (shared): {}", e);
                    false
                }
            }
        } else {
            true
        }
    }

    /// Create a new BluetoothPlayerController with auto-discovery
    pub fn new() -> Self {
        Self::new_with_address(None)
    }

    /// Create a new BluetoothPlayerController with a specific device address
    pub fn new_with_address(device_address: Option<String>) -> Self {
        let device_address = device_address.and_then(|addr| {
            let trimmed = addr.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });

        let auto_discover_mode = device_address.is_none();

        // Construct the player id WITH the "bluetooth:" prefix so it is the single
        // source of truth. The base controller stores this id and the inherent
        // BasePlayerController::notify_state_changed() stamps it onto every
        // StateChanged event (PlayerSource). ActiveMonitor looks players up by the
        // same id, so event id and lookup id must be identical. Previously the base
        // id was the bare "auto-discover" while get_player_id() returned the prefixed
        // "bluetooth:auto-discover", so the lookup never matched the event and the
        // Bluetooth player was never promoted to active (hifiberry/acr#11).
        let player_id = match &device_address {
            Some(addr) => format!("bluetooth:{}", addr),
            None => "bluetooth:auto-discover".to_string(),
        };

        let base = BasePlayerController::with_player_info("bluetooth", &player_id);

        // Set initial capabilities
        let capabilities = PlayerCapabilitySet::from_slice(&[
            PlayerCapability::Play,
            PlayerCapability::Pause,
            PlayerCapability::Stop,
            PlayerCapability::Next,
            PlayerCapability::Previous,
        ]);
        base.set_capabilities_set(capabilities, false);

        let controller = BluetoothPlayerController {
            base,
            connection: Arc::new(Mutex::new(None)),
            current_song: Arc::new(RwLock::new(None)),
            current_state: Arc::new(RwLock::new(PlayerState::new())),
            device_address: Arc::new(RwLock::new(device_address.clone())),
            player_path: Arc::new(RwLock::new(None)),
            device_name: Arc::new(RwLock::new(None)),
            auto_discover_mode,
            scan_thread: Arc::new(RwLock::new(None)),
            stop_scanning: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            poll_thread: Arc::new(RwLock::new(None)),
            stop_polling: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };

        info!("Created BluetoothPlayerController with address: {:?}", device_address);

        // If no specific device address is given, start auto-discovery
        if device_address.is_none() {
            info!("Starting auto-discovery for Bluetooth devices");
            controller.start_scanning_thread();
        } else {
            // Try to find the specific device immediately
            controller.find_player_path();
        }

        controller
    }

    /// Initialize D-Bus connection
    fn ensure_dbus_connection(&self) -> bool {
        let mut conn_guard = self.connection.lock();

        if conn_guard.is_none() {
            match Connection::new_system() {
                Ok(conn) => {
                    debug!("Established D-Bus system connection");
                    *conn_guard = Some(conn);
                    true
                }
                Err(e) => {
                    error!("Failed to connect to D-Bus system bus: {}", e);
                    false
                }
            }
        } else {
            true
        }
    }

    /// Find all available Bluetooth devices with MediaPlayer1 interface
    fn discover_bluetooth_devices(&self) -> Vec<(String, String)> {
        let mut devices = Vec::new();

        if !self.ensure_dbus_connection() {
            return devices;
        }

        let conn_guard = self.connection.lock();

        let conn = match conn_guard.as_ref() {
            Some(c) => c,
            None => return devices,
        };

        // Get the BlueZ object manager to enumerate all objects
        let proxy = conn.with_proxy("org.bluez", "/", Duration::from_millis(5000));

        // Try to get all managed objects
        if let Ok(objects) = proxy.get_managed_objects() {
            for (path, interfaces) in objects {
                // Look for MediaPlayer1 interfaces
                if interfaces.contains_key("org.bluez.MediaPlayer1") {
                    // Extract device address from path
                    // Path format: /org/bluez/hci0/dev_XX_XX_XX_XX_XX_XX/player0
                    if let Some(device_part) = path.strip_prefix("/org/bluez/hci0/dev_") {
                        if let Some(addr_part) = device_part.split('/').next() {
                            // Convert XX_XX_XX_XX_XX_XX back to XX:XX:XX:XX:XX:XX
                            let device_address = addr_part.replace('_', ":");

                            // Get device name
                            let device_path = format!("/org/bluez/hci0/dev_{}", addr_part);
                            let device_proxy = conn.with_proxy("org.bluez", &device_path, Duration::from_millis(1000));

                            let device_name = device_proxy.get::<String>("org.bluez.Device1", "Name")
                                .unwrap_or_else(|_| device_address.clone());

                            debug!("Found Bluetooth device with MediaPlayer1: {} ({})", device_name, device_address);
                            devices.push((device_address, device_name));
                        }
                    }
                }
            }
        }

        devices
    }
    /// Find the active player path for a given device address
    /// This scans for MediaPlayer1 interfaces (player0, player1, player2, etc.)
    fn find_active_player(&self, device_address: &str) -> Option<String> {
        if !self.ensure_dbus_connection() {
            return None;
        }

        // Convert MAC address format from 80:B9:89:1E:B5:6F to 80_B9_89_1E_B5_6F
        let device_path_part = device_address.replace(":", "_");

        // Use ObjectManager to find the actual player path (player index may vary: player0, player1, player2, etc.)
        let conn_guard = self.connection.lock();

        if let Some(conn) = conn_guard.as_ref() {
            let proxy = conn.with_proxy("org.bluez", "/", Duration::from_millis(5000));

            // Get all managed objects and find the MediaPlayer1 for our device
            if let Ok(objects) = proxy.get_managed_objects() {
                let device_prefix = format!("/org/bluez/hci0/dev_{}/player", device_path_part);

                for (path, interfaces) in objects {
                    // Look for MediaPlayer1 interface under our device path
                    if path.starts_with(&device_prefix) && interfaces.contains_key("org.bluez.MediaPlayer1") {
                        debug!("Found MediaPlayer1 at path: {}", path);
                        return Some(path.to_string());
                    }
                }

                debug!("MediaPlayer1 not found for device {}", device_address);
                None
            } else {
                debug!("Failed to get managed objects from BlueZ");
                None
            }
        } else {
            None
        }
    }

    /// Static helper for checking and updating active player in the polling thread
    fn check_and_update_active_player(
        player_path: &Arc<RwLock<Option<String>>>,
        connection: &Arc<Mutex<Option<Connection>>>,
        device_address: &Arc<RwLock<Option<String>>>,
    ) {
        let current_path = player_path.read().clone();
        let mut current_path_is_valid = false;

        let device_addr = device_address.read().clone();

        // If we have a stored path, check if it's still valid
        if let Some(path) = current_path.clone() {
            let conn_guard = connection.lock();

            if let Some(conn) = conn_guard.as_ref() {
                let proxy = conn.with_proxy("org.bluez", "/", Duration::from_millis(5000));

                // Check if the current path still exists
                if let Ok(objects) = proxy.get_managed_objects() {
                    if objects.contains_key(&dbus::Path::from(path.clone())) {
                        // Current player path is still valid
                        current_path_is_valid = true;
                    } else {
                        debug!("Current player path {} no longer exists, searching for new player", path);
                    }
                } else {
                    debug!("Failed to get managed objects from BlueZ");
                    return;
                }
            } else {
                return;
            }
        }

        if current_path_is_valid {
            return;
        }

        // Current path is invalid or doesn't exist, try to find a new player
        let replacement_path = if let Some(addr) = device_addr {
            Self::find_active_player_static(connection, &addr)
        } else {
            None
        };

        if let Some(ref new_path) = replacement_path {
            info!("Found new active player at path: {}", new_path);
        }

        let resolved_path = resolve_player_path_transition(current_path, false, replacement_path);
        let mut guard = player_path.write();
        *guard = resolved_path;
    }

    /// Static helper to find active player (for use in polling thread)
    fn find_active_player_static(
        connection: &Arc<Mutex<Option<Connection>>>,
        device_address: &str,
    ) -> Option<String> {
        // Convert MAC address format from 80:B9:89:1E:B5:6F to 80_B9_89_1E_B5_6F
        let device_path_part = device_address.replace(":", "_");

        let conn_guard = connection.lock();

        if let Some(conn) = conn_guard.as_ref() {
            let proxy = conn.with_proxy("org.bluez", "/", Duration::from_millis(5000));

            // Get all managed objects and find the MediaPlayer1 for our device
            if let Ok(objects) = proxy.get_managed_objects() {
                let device_prefix = format!("/org/bluez/hci0/dev_{}/player", device_path_part);

                for (path, interfaces) in objects {
                    // Look for MediaPlayer1 interface under our device path
                    if path.starts_with(&device_prefix) && interfaces.contains_key("org.bluez.MediaPlayer1") {
                        debug!("Found MediaPlayer1 at path: {}", path);
                        return Some(path.to_string());
                    }
                }

                debug!("MediaPlayer1 not found for device {}", device_address);
                None
            } else {
                debug!("Failed to get managed objects from BlueZ");
                None
            }
        } else {
            None
        }
    }

    /// Check if the currently stored active player is still available
    /// If not, attempt to find a new player (e.g., player0 -> player1 transition)
    fn check_active_player(&self) -> bool {
        if !self.ensure_dbus_connection() {
            return false;
        }

        let current_path = self.player_path.read().clone();
        let mut current_path_is_valid = false;

        let device_address = self.device_address.read().clone();

        // If we have a stored path, check if it's still valid
        if let Some(path) = current_path.clone() {
            let conn_guard = self.connection.lock();

            if let Some(conn) = conn_guard.as_ref() {
                let proxy = conn.with_proxy("org.bluez", "/", Duration::from_millis(5000));

                // Check if the current path still exists
                if let Ok(objects) = proxy.get_managed_objects() {
                    if objects.contains_key(&dbus::Path::from(path.clone())) {
                        // Current player path is still valid
                        current_path_is_valid = true;
                    } else {
                        debug!("Current player path {} no longer exists, searching for new player", path);
                    }
                } else {
                    debug!("Failed to get managed objects from BlueZ");
                    return false;
                }
            } else {
                return false;
            }
        }

        if current_path_is_valid {
            return true;
        }

        // Current path is invalid or doesn't exist, try to find a new player
        let replacement_path = if let Some(addr) = device_address {
            self.find_active_player(&addr)
        } else {
            None
        };

        if let Some(ref new_path) = replacement_path {
            info!("Found new active player at path: {}", new_path);
        }

        let resolved_path = resolve_player_path_transition(current_path, false, replacement_path);
        let has_resolved_path = resolved_path.is_some();
        let mut guard = self.player_path.write();
        *guard = resolved_path;

        has_resolved_path
    }

    /// Find the MediaPlayer1 object path for the device
    fn find_player_path(&self) -> Option<String> {
        if !self.ensure_dbus_connection() {
            return None;
        }

        // Get current device address
        let device_address = self.device_address.read().clone();

        // If no specific device address, try to discover one
        let device_address = match device_address {
            Some(addr) => addr,
            None => {
                // Auto-discover first available device
                let discovered = self.discover_bluetooth_devices();
                if let Some((addr, name)) = discovered.first() {
                    info!("Auto-discovered Bluetooth device: {} ({})", name, addr);

                    // Update our stored address and name
                    {
                        let mut guard = self.device_address.write();
                        *guard = Some(addr.clone());
                    }
                    {
                        let mut guard = self.device_name.write();
                        *guard = Some(name.clone());
                    }

                    addr.clone()
                } else {
                    debug!("No Bluetooth devices with MediaPlayer1 found");
                    return None;
                }
            }
        };

        // Use the new find_active_player function
        self.find_active_player(&device_address)
    }

    /// Get device friendly name
    fn get_device_name(&self) -> Option<String> {
        if !self.ensure_dbus_connection() {
            return None;
        }

        let conn_guard = self.connection.lock();

        let conn = conn_guard.as_ref()?;

        let device_address = self.device_address.read().clone();

        let device_address = device_address?;
        let device_path_part = device_address.replace(":", "_");
        let device_path = format!("/org/bluez/hci0/dev_{}", device_path_part);

        let proxy = conn.with_proxy("org.bluez", &device_path, Duration::from_millis(1000));

        // Try to get the Name property using D-Bus property interface
        use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;

        match proxy.get::<String>("org.bluez.Device1", "Name") {
            Ok(name) => {
                debug!("Device name: {}", name);
                Some(name)
            }
            Err(e) => {
                debug!("Failed to get device name: {}", e);
                None
            }
        }
    }

    /// Update current song from D-Bus
    fn update_song_from_dbus(&self) {
        // Check if the active player is still valid before querying
        self.check_active_player();

        let player_path = self.player_path.read().clone();

        let player_path = match player_path {
            Some(path) => path,
            None => {
                // Try to find the player path
                if let Some(path) = self.find_player_path() {
                    let mut guard = self.player_path.write();
                    *guard = Some(path.clone());
                    path
                } else {
                    return;
                }
            }
        };

        if !self.ensure_dbus_connection() {
            return;
        }

        let conn_guard = self.connection.lock();

        let conn = match conn_guard.as_ref() {
            Some(c) => c,
            None => return,
        };

        let proxy = conn.with_proxy("org.bluez", &player_path, Duration::from_millis(1000));

        // Use D-Bus Properties interface to get Track information
        if let Ok(track_info) = proxy.get::<HashMap<String, dbus::arg::Variant<Box<dyn dbus::arg::RefArg>>>>("org.bluez.MediaPlayer1", "Track") {
            let mut metadata = HashMap::new();
            let mut title = None;
            let mut artist = None;
            let mut album = None;
            let mut duration = None;

            for (key, variant) in track_info {
                match key.as_str() {
                    "Title" => {
                        if let Some(val) = variant.as_str() {
                            title = Some(val.to_string());
                            metadata.insert("title".to_string(), serde_json::Value::String(val.to_string()));
                        }
                    }
                    "Artist" => {
                        if let Some(val) = variant.as_str() {
                            artist = Some(val.to_string());
                            metadata.insert("artist".to_string(), serde_json::Value::String(val.to_string()));
                        }
                    }
                    "Album" => {
                        if let Some(val) = variant.as_str() {
                            album = Some(val.to_string());
                            metadata.insert("album".to_string(), serde_json::Value::String(val.to_string()));
                        }
                    }
                    "Duration" => {
                        if let Some(val) = variant.as_u64() {
                            // BlueZ MediaPlayer1 Track.Duration is milliseconds.
                            let duration_secs = duration_millis_to_song_seconds(val);
                            duration = Some(duration_secs);
                            metadata.insert("duration".to_string(), serde_json::Value::Number(
                                serde_json::Number::from_f64(duration_secs).unwrap_or(serde_json::Number::from(0))
                            ));
                        }
                    }
                    _ => {
                        // Store other metadata as strings
                        if let Some(val) = variant.as_str() {
                            metadata.insert(key.to_lowercase(), serde_json::Value::String(val.to_string()));
                        }
                    }
                }
            }

            // Create song if we have at least a title
            if let Some(title) = title {
                let song = Song {
                    title: Some(title),
                    artist,
                    album,
                    duration,
                    metadata,
                    ..Default::default()
                };

                {
                    let mut guard = self.current_song.write();
                    *guard = Some(song);
                    debug!("Updated Bluetooth song information");
                }
            }
        }
    }

    /// Send a D-Bus method call to the MediaPlayer1 interface
    fn send_dbus_command(&self, method: &str) -> bool {
        let player_path = self.player_path.read().clone();

        let player_path = match player_path {
            Some(path) => path,
            None => {
                if let Some(path) = self.find_player_path() {
                    let mut guard = self.player_path.write();
                    *guard = Some(path.clone());
                    path
                } else {
                    let addr = self.device_address.read().clone();
                    warn!("No MediaPlayer1 found for device {:?}", addr);
                    return false;
                }
            }
        };

        if !self.ensure_dbus_connection() {
            return false;
        }

        let conn_guard = self.connection.lock();

        let conn = match conn_guard.as_ref() {
            Some(c) => c,
            None => return false,
        };

        let proxy = conn.with_proxy("org.bluez", &player_path, Duration::from_millis(5000));

        match proxy.method_call("org.bluez.MediaPlayer1", method, ()) {
            Ok(()) => {
                debug!("Successfully sent {} command to Bluetooth device", method);
                true
            }
            Err(e) => {
                warn!("Failed to send {} command to Bluetooth device: {}", method, e);
                false
            }
        }
    }

    /// Start background scanning for devices
    fn start_scanning_thread(&self) {
        if self.scan_thread.read().is_some() {
            debug!("Bluetooth scanning thread already running");
            return;
        }

        // Don't start if we already have an active player path.
        if !should_scan_for_player_path(self.player_path.read().is_some()) {
            return;
        }

        let device_address = Arc::clone(&self.device_address);
        let device_name = Arc::clone(&self.device_name);
        let player_path = Arc::clone(&self.player_path);
        let stop_flag = Arc::clone(&self.stop_scanning);
        let connection = Arc::clone(&self.connection);

        let handle = thread::spawn(move || {
            info!("Starting Bluetooth device scanning thread");
            let mut scan_interval_secs = BLUETOOTH_SCAN_INTERVAL_STEP_SECS;

            if !Self::ensure_shared_dbus_connection(&connection) {
                debug!("Bluetooth scanning thread exiting due to missing D-Bus connection");
                return;
            }

            while !stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                // Check if we still need to scan
                if !should_scan_for_player_path(player_path.read().is_some()) {
                    // We already have a player path, stop scanning.
                    break;
                }

                // Try to discover devices
                {
                    let conn_guard = connection.lock();
                    if let Some(conn) = conn_guard.as_ref() {
                        // Simplified discovery logic for background thread
                        let proxy = conn.with_proxy("org.bluez", "/", Duration::from_millis(2000));

                        if let Ok(objects) = proxy.get_managed_objects() {
                            for (path, interfaces) in objects {
                                if interfaces.contains_key("org.bluez.MediaPlayer1") {
                                    if let Some(device_part) = path.strip_prefix("/org/bluez/hci0/dev_") {
                                        if let Some(addr_part) = device_part.split('/').next() {
                                            let discovered_address = addr_part.replace('_', ":");

                                            // Get device name
                                            let device_path = format!("/org/bluez/hci0/dev_{}", addr_part);
                                            let device_proxy = conn.with_proxy("org.bluez", &device_path, Duration::from_millis(1000));

                                            let discovered_name = device_proxy.get::<String>("org.bluez.Device1", "Name")
                                                .unwrap_or_else(|_| discovered_address.clone());

                                            info!("Background scan found Bluetooth device: {} ({})", discovered_name, discovered_address);

                                            // Update stored values
                                            *device_address.write() = Some(discovered_address);
                                            *device_name.write() = Some(discovered_name);
                                            *player_path.write() = Some(path.to_string());
                                            // Found a device, stop scanning
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Wait 5 seconds before next scan
                thread::sleep(Duration::from_secs(scan_interval_secs));
                scan_interval_secs = next_scan_interval_secs(scan_interval_secs);
            }

            debug!("Bluetooth scanning thread stopped");
        });

        *self.scan_thread.write() = Some(handle);
    }

    /// Manually trigger a rescan for devices
    pub fn rescan(&self) {
        debug!("Manually triggering Bluetooth device rescan");

        // Clear current device info to force rediscovery
        *self.device_address.write() = None;
        *self.player_path.write() = None;
        *self.device_name.write() = None;

        // Try to find a device immediately
        if self.find_player_path().is_none() && self.auto_discover_mode {
            self.start_scanning_thread();
        }
    }



    /// Poll and update playback state
    fn poll_playback_state(
        proxy: &dbus::blocking::Proxy<&dbus::blocking::Connection>,
        current_state: &Arc<RwLock<PlayerState>>,
        current_song: &Arc<RwLock<Option<Song>>>,
        base: &BasePlayerController,
    ) {
        if let Ok(status) = proxy.get::<String>("org.bluez.MediaPlayer1", "Status") {
            let new_state = match status.as_str() {
                "playing" => PlaybackState::Playing,
                "paused" => PlaybackState::Paused,
                "stopped" => PlaybackState::Stopped,
                _ => PlaybackState::Unknown,
            };

            // Update state if changed
            {
                let mut state_guard = current_state.write();
                let old_state = state_guard.state;
                if old_state != new_state {
                    debug!("Bluetooth playback state changed: {:?} -> {:?}", old_state, new_state);
                    state_guard.state = new_state;
                    base.notify_state_changed(new_state);

                    // When becoming active (starts playing), just notify about current song
                    if new_state == PlaybackState::Playing && old_state != PlaybackState::Playing {
                        debug!("Bluetooth player became active");

                        // Notify about current song when becoming active
                        let song_guard = current_song.read();
                        if let Some(ref song) = *song_guard {
                            base.notify_song_changed(Some(song));
                        }
                    }
                }
            }

            // Mark as alive
            base.alive();
        }
    }

    /// Poll and update track information
    fn poll_track_information(
        proxy: &dbus::blocking::Proxy<&dbus::blocking::Connection>,
        current_song: &Arc<RwLock<Option<Song>>>,
        base: &BasePlayerController,
    ) {
        if let Ok(track_data) = proxy.get::<HashMap<String, dbus::arg::Variant<Box<dyn RefArg>>>>("org.bluez.MediaPlayer1", "Track") {
            let title = track_data.get("Title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let artist = track_data.get("Artist")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let album = track_data.get("Album")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let duration = track_data.get("Duration")
                .and_then(|v| v.as_u64())
                .map(duration_millis_to_song_seconds);

            // Create new song if we have track data
            if title.is_some() || artist.is_some() || album.is_some() {
                let new_song = Song {
                    title: title.clone(),
                    artist: artist.clone(),
                    album: album.clone(),
                    duration,
                    ..Song::default()
                };

                // Update song if changed
                {
                    let mut song_guard = current_song.write();
                    let song_changed = song_guard.as_ref().map(|s| {
                        s.title != new_song.title ||
                        s.artist != new_song.artist ||
                        s.album != new_song.album
                    }).unwrap_or(true);

                    if song_changed {
                        info!("Bluetooth track changed: {:?} - {:?} ({:?})",
                               new_song.artist, new_song.title, new_song.album);
                        *song_guard = Some(new_song.clone());
                        base.notify_song_changed(Some(&new_song));

                        // Also mark as alive when song changes
                        base.alive();
                    }
                }
            }
        }
    }

    /// Poll and update position information
    fn poll_position_information(
        proxy: &dbus::blocking::Proxy<&dbus::blocking::Connection>,
        current_state: &Arc<RwLock<PlayerState>>,
        base: &BasePlayerController,
    ) {
        if let Ok(position) = proxy.get::<u32>("org.bluez.MediaPlayer1", "Position") {
            let position_seconds = position as f64 / 1000.0;

            // Update position in player state
            {
                let mut state_guard = current_state.write();
                if (state_guard.position.unwrap_or(0.0) - position_seconds).abs() > 1.0 {
                    state_guard.position = Some(position_seconds);
                    base.notify_position_changed(position_seconds);
                }
            }
        }
    }

    /// Main polling loop logic
    fn run_polling_loop(
        player_path: Arc<RwLock<Option<String>>>,
        connection: Arc<Mutex<Option<Connection>>>,
        current_song: Arc<RwLock<Option<Song>>>,
        current_state: Arc<RwLock<PlayerState>>,
        stop_flag: Arc<std::sync::atomic::AtomicBool>,
        base: BasePlayerController,
        device_address: Arc<RwLock<Option<String>>>,
    ) {
        info!("Starting Bluetooth status polling thread");

        let mut last_no_path_warning = SystemTime::UNIX_EPOCH;

        while !stop_flag.load(Ordering::Relaxed) {
            // Check if the active player is still available before polling
            // This handles transitions like player0 -> player1 -> player2
            Self::check_and_update_active_player(&player_path, &connection, &device_address);

            // Get current player path
            let path = player_path.read().clone();

            if let Some(ref path_str) = path {
                let conn_guard = connection.lock();
                if let Some(ref conn) = *conn_guard {
                    let proxy = conn.with_proxy("org.bluez", path_str, Duration::from_millis(1000));

                    // Poll different aspects of the player state
                    debug!("Polling Bluetooth player state at {}", path_str);
                    Self::poll_playback_state(&proxy, &current_state, &current_song, &base);
                    Self::poll_track_information(&proxy, &current_song, &base);
                    Self::poll_position_information(&proxy, &current_state, &base);
                }
            } else {
                // Only log this message every 10 seconds to avoid spam
                if let Ok(elapsed) = SystemTime::now().duration_since(last_no_path_warning) {
                    if elapsed >= Duration::from_secs(10) {
                        debug!("No player path available for polling");
                        last_no_path_warning = SystemTime::now();
                    }
                }
            }

            // Poll every 2 seconds
            thread::sleep(Duration::from_secs(2));
        }

        debug!("Bluetooth polling thread stopped");
    }

    /// Start the status polling thread
    fn start_polling_thread(&self) {
        if self.poll_thread.read().is_some() {
            debug!("Bluetooth status polling thread already running");
            return;
        }

        debug!("Starting Bluetooth status polling thread");

        let player_path = Arc::clone(&self.player_path);
        let connection = Arc::clone(&self.connection);
        let current_song = Arc::clone(&self.current_song);
        let current_state = Arc::clone(&self.current_state);
        let stop_flag = Arc::clone(&self.stop_polling);
        let base = self.base.clone();
        let device_address = Arc::clone(&self.device_address);

        let handle = thread::spawn(move || {
            Self::run_polling_loop(player_path, connection, current_song, current_state, stop_flag, base, device_address);
        });

        *self.poll_thread.write() = Some(handle);
    }
    fn get_playback_status(&self) -> PlaybackState {
        let player_path = self.player_path.read().clone();

        let player_path = match player_path {
            Some(path) => path,
            None => return PlaybackState::Unknown,
        };

        if !self.ensure_dbus_connection() {
            return PlaybackState::Unknown;
        }

        let conn_guard = self.connection.lock();

        let conn = match conn_guard.as_ref() {
            Some(c) => c,
            None => return PlaybackState::Unknown,
        };

        let proxy = conn.with_proxy("org.bluez", &player_path, Duration::from_millis(1000));

        match proxy.get::<String>("org.bluez.MediaPlayer1", "Status") {
            Ok(status) => {
                match status.as_str() {
                    "playing" => PlaybackState::Playing,
                    "paused" => PlaybackState::Paused,
                    "stopped" => PlaybackState::Stopped,
                    _ => PlaybackState::Unknown,
                }
            }
            Err(_) => PlaybackState::Unknown,
        }
    }
}

impl PlayerController for BluetoothPlayerController {
    delegate! {
        to self.base {
            fn get_capabilities(&self) -> PlayerCapabilitySet;
            fn get_last_seen(&self) -> Option<SystemTime>;
        }
    }

    fn get_song(&self) -> Option<Song> {
        // Update song information from D-Bus before returning
        self.update_song_from_dbus();

        self.current_song.read().clone()
    }

    fn get_queue(&self) -> Vec<Track> {
        // Bluetooth devices typically don't expose queue information via D-Bus
        Vec::new()
    }

    fn get_loop_mode(&self) -> LoopMode {
        // Most Bluetooth devices don't expose loop mode via D-Bus
        LoopMode::None
    }

    fn get_playback_state(&self) -> PlaybackState {
        let state = self.get_playback_status();

        // Update our internal state
        self.current_state.write().state = state;

        // Mark as alive
        self.base.alive();

        state
    }

    fn get_position(&self) -> Option<f64> {
        // Most Bluetooth devices don't expose precise position via D-Bus
        None
    }

    fn get_shuffle(&self) -> bool {
        // Most Bluetooth devices don't expose shuffle state via D-Bus
        false
    }

    fn get_player_name(&self) -> String {
        "bluetooth".to_string()
    }

    fn get_aliases(&self) -> Vec<String> {
        vec!["bluetooth".to_string(), "bluez".to_string(), "bt".to_string()]
    }

    fn get_player_id(&self) -> String {
        // Delegate to the base controller, which holds the already-prefixed id
        // ("bluetooth:auto-discover" or "bluetooth:<device_address>") set in
        // new_with_address(). This keeps a single source of truth so the id on
        // StateChanged events (stamped via the inherent base method) matches the id
        // ActiveMonitor looks up (hifiberry/acr#11).
        self.base.get_player_id()
    }

    fn send_command(&self, command: PlayerCommand) -> bool {
        info!("Sending command to Bluetooth device: {}", command);

        // Update player path if needed
        if self.player_path.read().is_none() {
            if let Some(path) = self.find_player_path() {
                *self.player_path.write() = Some(path);
            }
        }

        match command {
            PlayerCommand::Play => self.send_dbus_command("Play"),
            PlayerCommand::Pause => self.send_dbus_command("Pause"),
            PlayerCommand::PlayPause => {
                // Determine current state and toggle
                match self.get_playback_state() {
                    PlaybackState::Playing => self.send_dbus_command("Pause"),
                    _ => self.send_dbus_command("Play"),
                }
            }
            PlayerCommand::Stop => self.send_dbus_command("Stop"),
            PlayerCommand::Next => self.send_dbus_command("Next"),
            PlayerCommand::Previous => self.send_dbus_command("Previous"),
            _ => {
                debug!("Unsupported command for Bluetooth device: {}", command);
                false
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn start(&self) -> bool {
        let addr = self.device_address.read().clone();
        info!("Starting Bluetooth player controller for device: {:?}", addr);

        // Ensure a fresh run for restart scenarios.
        self.stop_polling.store(false, Ordering::Relaxed);
        self.stop_scanning.store(false, Ordering::Relaxed);

        // Initialize D-Bus connection
        if !self.ensure_dbus_connection() {
            error!("Failed to initialize D-Bus connection");
            return false;
        }

        // Try to find the player path
        if let Some(path) = self.find_player_path() {
            *self.player_path.write() = Some(path);
            let addr = self.device_address.read().clone();
            info!("Found MediaPlayer1 interface for device: {:?}", addr);
        } else {
            let addr = self.device_address.read().clone();
            warn!("MediaPlayer1 interface not found for device: {:?}", addr);
            // Don't return false here as the device might connect later

            // In auto-discover mode, keep scanning whenever no player path is available.
            if should_start_scanning_in_start(self.auto_discover_mode, self.player_path.read().is_some()) {
                self.start_scanning_thread();
            }
        }

        // Always start polling thread - it will wait for a device if none is available yet
        self.start_polling_thread();

        // Get device name
        if let Some(name) = self.get_device_name() {
            *self.device_name.write() = Some(name);
        }

        // Mark as alive
        self.base.alive();

        true
    }

    fn stop(&self) -> bool {
        let addr = self.device_address.read().clone();
        info!("Stopping Bluetooth player controller for device: {:?}", addr);

        // Signal scanning thread to stop
        self.stop_scanning.store(true, Ordering::Relaxed);

        // Wait for scanning thread to finish
        {
            let mut guard = self.scan_thread.write();
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }

        // Signal polling thread to stop
        self.stop_polling.store(true, Ordering::Relaxed);

        // Wait for polling thread to finish
        {
            let mut guard = self.poll_thread.write();
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }

        // Clear connection
        *self.connection.lock() = None;

        // Clear player path
        *self.player_path.write() = None;

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::players::PlayerController;

    #[test]
    fn test_bluetooth_controller_creation() {
        let controller = BluetoothPlayerController::new_with_address(Some("80:B9:89:1E:B5:6F".to_string()));

        assert_eq!(controller.get_player_name(), "bluetooth");
        assert_eq!(controller.get_player_id(), "bluetooth:80:B9:89:1E:B5:6F");

        let aliases = controller.get_aliases();
        assert!(aliases.contains(&"bluetooth".to_string()));
        assert!(aliases.contains(&"bluez".to_string()));
        assert!(aliases.contains(&"bt".to_string()));

        // Test that it has basic capabilities
        let caps = controller.get_capabilities();
        assert!(caps.has_capability(crate::data::PlayerCapability::Play));
        assert!(caps.has_capability(crate::data::PlayerCapability::Pause));
        assert!(caps.has_capability(crate::data::PlayerCapability::Next));
        assert!(caps.has_capability(crate::data::PlayerCapability::Previous));
    }

    #[test]
    fn test_bluetooth_controller_from_factory() {
        use crate::players::player_factory::create_player_from_json_str;

        let config = r#"
        {
            "bluetooth": {
                "device_address": "80:B9:89:1E:B5:6F"
            }
        }
        "#;

        let result = create_player_from_json_str(config);
        assert!(result.is_ok());

        let controller = result.unwrap();
        assert_eq!(controller.get_player_name(), "bluetooth");
    }

    #[test]
    fn test_bluetooth_controller_auto_discover_id() {
        let controller = BluetoothPlayerController::new_with_address(None);
        assert_eq!(controller.get_player_id(), "bluetooth:auto-discover");
    }

    #[test]
    fn test_bluetooth_controller_empty_address_edge_case() {
        let controller = BluetoothPlayerController::new_with_address(Some(String::new()));
        assert_eq!(controller.get_player_id(), "bluetooth:auto-discover");
    }

    #[test]
    fn regression_bluetooth_controller_whitespace_address_defaults_to_auto_discover() {
        let controller = BluetoothPlayerController::new_with_address(Some("   ".to_string()));
        assert_eq!(controller.get_player_id(), "bluetooth:auto-discover");
    }

    #[test]
    fn test_bluetooth_unsupported_commands_return_false() {
        let controller = BluetoothPlayerController::new_with_address(Some("80:B9:89:1E:B5:6F".to_string()));

        assert!(!controller.send_command(PlayerCommand::Seek(42.0)));
        assert!(!controller.send_command(PlayerCommand::SetLoopMode(crate::data::LoopMode::Track)));
        assert!(!controller.send_command(PlayerCommand::SetRandom(true)));
    }

    #[test]
    fn test_bluetooth_stop_is_idempotent() {
        let controller = BluetoothPlayerController::new_with_address(Some("80:B9:89:1E:B5:6F".to_string()));

        assert!(controller.stop());
        assert!(controller.stop());
    }

    #[test]
    fn test_bluetooth_factory_missing_device_address_defaults_to_auto_discover() {
        use crate::players::player_factory::create_player_from_json_str;

        let config = r#"
        {
            "bluetooth": {}
        }
        "#;

        let result = create_player_from_json_str(config);
        assert!(result.is_ok());

        let controller = result.unwrap();
        assert_eq!(controller.get_player_name(), "bluetooth");
        assert_eq!(controller.get_player_id(), "bluetooth:auto-discover");
    }

    #[test]
    fn test_bluetooth_factory_invalid_device_address_type_defaults_to_auto_discover() {
        use crate::players::player_factory::create_player_from_json_str;

        let config = r#"
        {
            "bluetooth": {
                "device_address": 12345
            }
        }
        "#;

        let result = create_player_from_json_str(config);
        assert!(result.is_ok());

        let controller = result.unwrap();
        assert_eq!(controller.get_player_name(), "bluetooth");
        assert_eq!(controller.get_player_id(), "bluetooth:auto-discover");
    }

    #[test]
    fn test_duration_millis_to_song_seconds_is_consistent() {
        assert_eq!(duration_millis_to_song_seconds(0), 0.0);
        assert_eq!(duration_millis_to_song_seconds(1000), 1.0);
        assert_eq!(duration_millis_to_song_seconds(2500), 2.5);
    }

    #[test]
    fn test_resolve_player_path_transition_keeps_valid_current_path() {
        let current = Some("/org/bluez/hci0/dev_AA_BB/player0".to_string());
        let replacement = Some("/org/bluez/hci0/dev_AA_BB/player1".to_string());

        let resolved = resolve_player_path_transition(current.clone(), true, replacement);

        assert_eq!(resolved, current);
    }

    #[test]
    fn test_resolve_player_path_transition_clears_stale_path_when_no_replacement() {
        let current = Some("/org/bluez/hci0/dev_AA_BB/player0".to_string());

        let resolved = resolve_player_path_transition(current, false, None);

        assert!(resolved.is_none());
    }

    #[test]
    fn test_resolve_player_path_transition_uses_replacement_path() {
        let replacement = Some("/org/bluez/hci0/dev_AA_BB/player1".to_string());

        let resolved = resolve_player_path_transition(None, false, replacement.clone());

        assert_eq!(resolved, replacement);
    }

    #[test]
    fn test_should_scan_for_player_path() {
        assert!(should_scan_for_player_path(false));
        assert!(!should_scan_for_player_path(true));
    }

    #[test]
    fn test_should_start_scanning_in_start_only_for_auto_discover_without_path() {
        assert!(should_start_scanning_in_start(true, false));
        assert!(!should_start_scanning_in_start(true, true));
        assert!(!should_start_scanning_in_start(false, false));
    }

    #[test]
    fn test_next_scan_interval_secs_steps_to_60() {
        assert_eq!(next_scan_interval_secs(5), 10);
        assert_eq!(next_scan_interval_secs(10), 15);
        assert_eq!(next_scan_interval_secs(55), 60);
        assert_eq!(next_scan_interval_secs(60), 60);
        assert_eq!(next_scan_interval_secs(120), 60);
    }
}
