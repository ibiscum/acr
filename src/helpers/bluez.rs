use dbus::blocking::Connection;
use dbus::blocking::stdintf::org_freedesktop_dbus::{Properties, ObjectManager};
use dbus::arg::RefArg;
use std::time::Duration;
use std::collections::HashMap;
use log::{debug, info};

/// BlueZ D-Bus interface helper for Bluetooth device management
pub struct BlueZManager {
    connection: Connection,
}

/// Information about a Bluetooth audio device
#[derive(Debug, Clone)]
pub struct BluetoothDeviceInfo {
    pub device_address: String,
    pub device_name: Option<String>,
    pub player_path: String,
    pub is_connected: bool,
    pub is_playing: bool,
}

/// Current track information from a Bluetooth device
#[derive(Debug, Clone)]
pub struct BluetoothTrackInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<u32>, // in milliseconds
    pub position: Option<u32>, // in milliseconds
}

/// Playback status from MediaPlayer1 interface
#[derive(Debug, Clone, PartialEq)]
pub enum BluetoothPlaybackStatus {
    Playing,
    Paused,
    Stopped,
    Unknown,
}

impl BlueZManager {
    /// Create a new BlueZ manager with D-Bus connection
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let connection = Connection::new_system()
            .map_err(|e| format!("Failed to connect to D-Bus system bus: {}", e))?;

        debug!("Established D-Bus system connection for BlueZ management");
        Ok(BlueZManager { connection })
    }

    /// Discover all available Bluetooth audio devices
    pub fn discover_audio_devices(&self) -> Result<Vec<BluetoothDeviceInfo>, Box<dyn std::error::Error>> {
        debug!("Starting Bluetooth audio device discovery");

        let proxy = self.connection.with_proxy("org.bluez", "/", Duration::from_millis(5000));

        let objects: HashMap<dbus::Path, HashMap<String, HashMap<String, dbus::arg::Variant<Box<dyn RefArg>>>>> =
            proxy.get_managed_objects()
                .map_err(|e| format!("Failed to get managed objects from BlueZ: {}", e))?;

        let mut devices = Vec::new();

        for (path, interfaces) in objects {
            // Look for MediaPlayer1 interfaces
            if interfaces.contains_key("org.bluez.MediaPlayer1") {
                if let Some(device_part) = path.strip_prefix("/org/bluez/hci0/dev_") {
                    if let Some(addr_part) = device_part.split('/').next() {
                        let device_address = addr_part.replace('_', ":");

                        // Get device name and connection status
                        let device_path = format!("/org/bluez/hci0/dev_{}", addr_part);
                        let device_name = self.get_device_name(&device_path);
                        let is_connected = self.is_device_connected(&device_path);
                        let playback_status = self.get_playback_status(&path);

                        let device_info = BluetoothDeviceInfo {
                            device_address,
                            device_name,
                            player_path: path.to_string(),
                            is_connected,
                            is_playing: playback_status == BluetoothPlaybackStatus::Playing,
                        };

                        debug!("Found Bluetooth audio device: {:?}", device_info);
                        devices.push(device_info);
                    }
                }
            }
        }

        info!("Discovered {} Bluetooth audio devices", devices.len());
        Ok(devices)
    }

    /// Get the name of a Bluetooth device
    fn get_device_name(&self, device_path: &str) -> Option<String> {
        let proxy = self.connection.with_proxy("org.bluez", device_path, Duration::from_millis(1000));

        match proxy.get::<String>("org.bluez.Device1", "Name") {
            Ok(name) => {
                debug!("Got device name for {}: {}", device_path, name);
                Some(name)
            }
            Err(e) => {
                debug!("Failed to get device name for {}: {}", device_path, e);
                None
            }
        }
    }

    /// Check if a Bluetooth device is connected
    fn is_device_connected(&self, device_path: &str) -> bool {
        let proxy = self.connection.with_proxy("org.bluez", device_path, Duration::from_millis(1000));

        match proxy.get::<bool>("org.bluez.Device1", "Connected") {
            Ok(connected) => {
                debug!("Device {} connection status: {}", device_path, connected);
                connected
            }
            Err(e) => {
                debug!("Failed to get connection status for {}: {}", device_path, e);
                false
            }
        }
    }

    /// Get the current playback status from a MediaPlayer1 interface
    pub fn get_playback_status(&self, player_path: &str) -> BluetoothPlaybackStatus {
        let proxy = self.connection.with_proxy("org.bluez", player_path, Duration::from_millis(1000));

        match proxy.get::<String>("org.bluez.MediaPlayer1", "Status") {
            Ok(status) => {
                debug!("Playback status for {}: {}", player_path, status);
                parse_playback_status(&status)
            }
            Err(e) => {
                debug!("Failed to get playback status for {}: {}", player_path, e);
                BluetoothPlaybackStatus::Unknown
            }
        }
    }

    /// Get current track information from a MediaPlayer1 interface
    pub fn get_track_info(&self, player_path: &str) -> Result<BluetoothTrackInfo, Box<dyn std::error::Error>> {
        let proxy = self.connection.with_proxy("org.bluez", player_path, Duration::from_millis(1000));

        // Get track metadata
        let track_info = match proxy.get::<HashMap<String, dbus::arg::Variant<Box<dyn RefArg>>>>("org.bluez.MediaPlayer1", "Track") {
            Ok(track_data) => {
                debug!("Got track metadata for {}", player_path);

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
                    .map(|d| d as u32);

                BluetoothTrackInfo {
                    title,
                    artist,
                    album,
                    duration,
                    position: None, // Position is separate from track metadata
                }
            }
            Err(e) => {
                debug!("Failed to get track metadata for {}: {}", player_path, e);
                BluetoothTrackInfo {
                    title: None,
                    artist: None,
                    album: None,
                    duration: None,
                    position: None,
                }
            }
        };

        // Get current position
        let position = match proxy.get::<u32>("org.bluez.MediaPlayer1", "Position") {
            Ok(pos) => {
                debug!("Got position for {}: {} ms", player_path, pos);
                Some(pos)
            }
            Err(e) => {
                debug!("Failed to get position for {}: {}", player_path, e);
                None
            }
        };

        Ok(BluetoothTrackInfo {
            position,
            ..track_info
        })
    }

    /// Send a control command to a MediaPlayer1 interface
    pub fn send_control_command(&self, player_path: &str, command: &str) -> Result<(), Box<dyn std::error::Error>> {
        let proxy = self.connection.with_proxy("org.bluez", player_path, Duration::from_millis(2000));

        match command {
            "play" => {
                proxy.method_call::<(), _, _, _>("org.bluez.MediaPlayer1", "Play", ())
                    .map_err(|e| format!("Failed to send play command: {}", e))?;
            }
            "pause" => {
                proxy.method_call::<(), _, _, _>("org.bluez.MediaPlayer1", "Pause", ())
                    .map_err(|e| format!("Failed to send pause command: {}", e))?;
            }
            "stop" => {
                proxy.method_call::<(), _, _, _>("org.bluez.MediaPlayer1", "Stop", ())
                    .map_err(|e| format!("Failed to send stop command: {}", e))?;
            }
            "next" => {
                proxy.method_call::<(), _, _, _>("org.bluez.MediaPlayer1", "Next", ())
                    .map_err(|e| format!("Failed to send next command: {}", e))?;
            }
            "previous" => {
                proxy.method_call::<(), _, _, _>("org.bluez.MediaPlayer1", "Previous", ())
                    .map_err(|e| format!("Failed to send previous command: {}", e))?;
            }
            _ => {
                return Err(format!("Unknown command: {}", command).into());
            }
        }

        info!("Sent {} command to {}", command, player_path);
        Ok(())
    }

    /// Find a specific device by MAC address
    pub fn find_device_by_address(&self, target_address: &str) -> Result<Option<BluetoothDeviceInfo>, Box<dyn std::error::Error>> {
        let devices = self.discover_audio_devices()?;

        for device in devices {
            if device.device_address.eq_ignore_ascii_case(target_address) {
                return Ok(Some(device));
            }
        }

        Ok(None)
    }

    /// Get the currently active (playing) Bluetooth device
    pub fn get_active_device(&self) -> Result<Option<BluetoothDeviceInfo>, Box<dyn std::error::Error>> {
        let devices = self.discover_audio_devices()?;

        for device in devices {
            if device.is_playing {
                return Ok(Some(device));
            }
        }

        Ok(None)
    }
}

/// Map a BlueZ MediaPlayer1 Status string to the corresponding enum variant.
/// This is a pure function so it can be tested without a D-Bus connection.
pub fn parse_playback_status(status: &str) -> BluetoothPlaybackStatus {
    match status {
        "playing" => BluetoothPlaybackStatus::Playing,
        "paused" => BluetoothPlaybackStatus::Paused,
        "stopped" => BluetoothPlaybackStatus::Stopped,
        _ => BluetoothPlaybackStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bluez_manager_creation() {
        // Only attempt if the system D-Bus socket is present
        if std::path::Path::new("/var/run/dbus/system_bus_socket").exists() {
            let _result = BlueZManager::new();
            // Do not assert success — BlueZ daemon may not be running
        }
    }

    // --- parse_playback_status ---

    #[test]
    fn parse_playback_status_maps_all_known_strings() {
        assert_eq!(parse_playback_status("playing"), BluetoothPlaybackStatus::Playing);
        assert_eq!(parse_playback_status("paused"),  BluetoothPlaybackStatus::Paused);
        assert_eq!(parse_playback_status("stopped"), BluetoothPlaybackStatus::Stopped);
    }

    #[test]
    fn parse_playback_status_unknown_for_unrecognised_string() {
        assert_eq!(parse_playback_status(""),         BluetoothPlaybackStatus::Unknown);
        assert_eq!(parse_playback_status("Playing"),  BluetoothPlaybackStatus::Unknown); // case-sensitive
        assert_eq!(parse_playback_status("PAUSED"),   BluetoothPlaybackStatus::Unknown);
        assert_eq!(parse_playback_status("buffering"), BluetoothPlaybackStatus::Unknown);
    }

    // --- BluetoothDeviceInfo ---

    #[test]
    fn device_info_stores_all_fields_correctly() {
        let info = BluetoothDeviceInfo {
            device_address: "80:B9:89:1E:B5:6F".to_string(),
            device_name: Some("Test Device".to_string()),
            player_path: "/org/bluez/hci0/dev_80_B9_89_1E_B5_6F/player0".to_string(),
            is_connected: true,
            is_playing: false,
        };

        assert_eq!(info.device_address, "80:B9:89:1E:B5:6F");
        assert_eq!(info.device_name, Some("Test Device".to_string()));
        assert!(info.is_connected);
        assert!(!info.is_playing);
    }

    #[test]
    fn device_info_allows_no_name() {
        let info = BluetoothDeviceInfo {
            device_address: "AA:BB:CC:DD:EE:FF".to_string(),
            device_name: None,
            player_path: "/path".to_string(),
            is_connected: false,
            is_playing: false,
        };
        assert!(info.device_name.is_none());
    }

    // --- BluetoothTrackInfo ---

    #[test]
    fn track_info_stores_all_fields_correctly() {
        let info = BluetoothTrackInfo {
            title:    Some("Test Song".to_string()),
            artist:   Some("Test Artist".to_string()),
            album:    Some("Test Album".to_string()),
            duration: Some(180_000), // 3 min in ms
            position: Some(30_000),  // 30 s in ms
        };

        assert_eq!(info.title,    Some("Test Song".to_string()));
        assert_eq!(info.artist,   Some("Test Artist".to_string()));
        assert_eq!(info.album,    Some("Test Album".to_string()));
        assert_eq!(info.duration, Some(180_000));
        assert_eq!(info.position, Some(30_000));
    }

    #[test]
    fn track_info_all_none_is_valid() {
        let info = BluetoothTrackInfo {
            title:    None,
            artist:   None,
            album:    None,
            duration: None,
            position: None,
        };

        assert!(info.title.is_none());
        assert!(info.duration.is_none());
        assert!(info.position.is_none());
    }
}
