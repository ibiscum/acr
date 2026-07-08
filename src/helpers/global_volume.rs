use crate::helpers::volume::VolumeControl;
#[cfg(all(feature = "alsa", not(windows)))]
use crate::helpers::volume::AlsaVolumeControl;
use crate::helpers::volume::DummyVolumeControl;
use crate::helpers::configurator;
use std::sync::Arc;
use parking_lot::Mutex;
use once_cell::sync::OnceCell;
use log::{info, warn, error};
use serde_json::Value;
use crate::config::get_service_config;

/// Global volume control instance
static GLOBAL_VOLUME_CONTROL: OnceCell<Arc<Mutex<Box<dyn VolumeControl + Send + Sync>>>> = OnceCell::new();

/// Initialize the global volume control from configuration
pub fn initialize_volume_control(config: &Value) {
    info!("Initializing volume control from configuration");

    if let Some(volume_config) = get_service_config(config, "volume") {
        // Check if volume control is enabled
        let enabled = volume_config
            .get("enable")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);  // Default to enabled

        if !enabled {
            info!("Volume control is explicitly disabled in configuration");
            // Initialize with a dummy control that's not available
            let mut dummy_control = DummyVolumeControl::new(
                "disabled".to_string(),
                "Disabled Volume Control".to_string(),
                0.0
            );
            dummy_control.set_available(false);
            let dummy_control: Box<dyn VolumeControl + Send + Sync> = Box::new(dummy_control);
            let _ = GLOBAL_VOLUME_CONTROL.set(Arc::new(Mutex::new(dummy_control)));
            return;
        }

        // Get the volume control type
        let control_type = volume_config
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("dummy");

        let control: Box<dyn VolumeControl + Send + Sync> = match control_type {
            #[cfg(all(feature = "alsa", not(windows)))]
            "alsa" => {
                let device = volume_config
                    .get("device")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let control_name = volume_config
                    .get("control_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let display_name = volume_config
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Master Volume");

                // Auto-detect device and control name from configurator API if not provided
                let (final_device, final_control_name) = if device.is_empty() || control_name.is_empty() {
                    info!("Auto-detecting ALSA volume settings from configurator API (device='{}', control_name='{}')", device, control_name);

                    // Get retry configuration from volume config or use defaults
                    let retry_count = volume_config
                        .get("auto_detect_retry_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(2) as usize;

                    let retry_delay_seconds = volume_config
                        .get("auto_detect_retry_delay_seconds")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(10);

                    // Attempt to get system info with retries
                    let mut last_error = String::new();
                    let mut result: Option<(String, String)> = None;

                    for attempt in 0..=retry_count {
                        if attempt > 0 {
                            info!("Retrying auto-detection after {} seconds (attempt {}/{})", retry_delay_seconds, attempt + 1, retry_count + 1);
                            std::thread::sleep(std::time::Duration::from_secs(retry_delay_seconds));
                        }

                        match configurator::get_system_info() {
                            Ok(system_info) => {
                                let auto_device = if device.is_empty() {
                                    if let Some(soundcard) = &system_info.soundcard {
                                        if let Some(hw_index) = soundcard.hardware_index {
                                            format!("hw:{}", hw_index)
                                        } else {
                                            "default".to_string()
                                        }
                                    } else {
                                        "default".to_string()
                                    }
                                } else {
                                    device.to_string()
                                };

                                let auto_control_name = if control_name.is_empty() {
                                    if let Some(soundcard) = &system_info.soundcard {
                                        if let Some(vol_control) = &soundcard.volume_control {
                                            vol_control.clone()
                                        } else {
                                            "Master".to_string()
                                        }
                                    } else {
                                        "Master".to_string()
                                    }
                                } else {
                                    control_name.to_string()
                                };

                                info!("Auto-detected ALSA volume settings from configurator API: device='{}', control='{}'", auto_device, auto_control_name);
                                result = Some((auto_device, auto_control_name));
                                break;
                            }
                            Err(e) => {
                                last_error = e.to_string();
                                if attempt < retry_count {
                                    warn!("Failed to get system info from configurator API (attempt {}/{}): {}. Retrying...", attempt + 1, retry_count + 1, e);
                                } else {
                                    warn!("Failed to get system info from configurator API after {} attempts: {}", retry_count + 1, e);
                                }
                            }
                        }
                    }

                    // Check if we got a result from the retry loop
                    if let Some((detected_device, detected_control)) = result {
                        (detected_device, detected_control)
                    } else {
                        // If all retries failed
                        // If both device and control_name were empty (auto-detection requested)
                        // and API fails after all retries, disable volume control
                        if device.is_empty() && control_name.is_empty() {
                            error!("Auto-detection failed after {} retries and no manual configuration provided. Disabling volume control.", retry_count + 1);
                            let mut dummy_control = DummyVolumeControl::new(
                                "auto_detection_failed".to_string(),
                                format!("Auto-detection Failed ({})", last_error),
                                0.0
                            );
                            dummy_control.set_available(false);
                            let dummy_control: Box<dyn VolumeControl + Send + Sync> = Box::new(dummy_control);
                            let _ = GLOBAL_VOLUME_CONTROL.set(Arc::new(Mutex::new(dummy_control)));
                            return;
                        }

                        // Only use fallback if at least one value was explicitly configured
                        let fallback_device = if device.is_empty() { "default".to_string() } else { device.to_string() };
                        let fallback_control = if control_name.is_empty() { "Master".to_string() } else { control_name.to_string() };
                        warn!("Using fallback ALSA volume settings after auto-detection failure: device='{}', control='{}'", fallback_device, fallback_control);
                        (fallback_device, fallback_control)
                    }
                } else {
                    info!("Using configured ALSA volume settings: device='{}', control='{}'", device, control_name);
                    (device.to_string(), control_name.to_string())
                };

                match AlsaVolumeControl::new(final_device.clone(), final_control_name.clone(), display_name.to_string()) {
                    Ok(alsa_control) => {
                        info!("Successfully initialized ALSA volume control on device '{}', control '{}'", final_device, final_control_name);
                        log::debug!("ALSA volume control supports change monitoring: {}", alsa_control.supports_change_monitoring());
                        log::debug!("To start volume change monitoring, call start_volume_change_monitoring()");
                        Box::new(alsa_control)
                    }
                    Err(e) => {
                        error!("Failed to initialize ALSA volume control: {}. Falling back to dummy control.", e);
                        let mut dummy_control = DummyVolumeControl::new(
                            "alsa_fallback".to_string(),
                            "ALSA Fallback".to_string(),
                            50.0
                        );
                        dummy_control.set_available(false);
                        Box::new(dummy_control)
                    }
                }
            }
            #[cfg(not(all(feature = "alsa", not(windows))))]
            "alsa" => {
                warn!("ALSA volume control requested but ALSA support not compiled in. Falling back to dummy control.");
                let mut dummy_control = DummyVolumeControl::new(
                    "alsa_not_available".to_string(),
                    "ALSA Not Available".to_string(),
                    50.0
                );
                dummy_control.set_available(false);
                Box::new(dummy_control)
            }
            "dummy" => {
                let internal_name = volume_config
                    .get("internal_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("dummy");

                let display_name = volume_config
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Dummy Volume Control");

                let initial_percent = volume_config
                    .get("initial_percent")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(50.0);

                info!("Initialized dummy volume control '{}' with initial volume {}%", display_name, initial_percent);
                Box::new(DummyVolumeControl::new(
                    internal_name.to_string(),
                    display_name.to_string(),
                    initial_percent
                ))
            }
            _ => {
                warn!("Unknown volume control type '{}', falling back to dummy control", control_type);
                let mut dummy_control = DummyVolumeControl::new(
                    "unknown_fallback".to_string(),
                    "Unknown Type Fallback".to_string(),
                    50.0
                );
                dummy_control.set_available(false);
                Box::new(dummy_control)
            }
        };

        // Store the global volume control
        if GLOBAL_VOLUME_CONTROL.set(Arc::new(Mutex::new(control))).is_err() {
            error!("Failed to set global volume control - already initialized");
        } else {
            info!("Global volume control initialized successfully");
        }
    } else {
        info!("No volume configuration found, using dummy volume control");
        // Create a working dummy volume control
        let dummy_control: Box<dyn VolumeControl + Send + Sync> = Box::new(DummyVolumeControl::new(
            "no_config".to_string(),
            "Default Volume Control".to_string(),
            50.0
        ));

        if GLOBAL_VOLUME_CONTROL.set(Arc::new(Mutex::new(dummy_control))).is_err() {
            error!("Failed to set global volume control - already initialized");
        } else {
            info!("Dummy volume control initialized successfully");
        }
    }
}

/// Get the global volume control instance
///
/// # Returns
///
/// An Arc<Mutex<Box<dyn VolumeControl + Send + Sync>>> if initialized, error otherwise
pub fn get_global_volume_control() -> Result<Arc<Mutex<Box<dyn VolumeControl + Send + Sync>>>, Box<dyn std::error::Error>> {
    GLOBAL_VOLUME_CONTROL.get()
        .cloned()
        .ok_or_else(|| "Volume control not initialized".into())
}

/// Get the current volume as a percentage (0-100%)
///
/// # Returns
///
/// The current volume percentage, or None if volume control is not available
pub fn get_volume_percentage() -> Option<f64> {
    get_global_volume_control().ok()?.lock().get_volume_percent().ok()
}

/// Set the volume as a percentage (0-100%)
///
/// # Arguments
///
/// * `percentage` - Volume level as a percentage (0.0 to 100.0)
///
/// # Returns
///
/// true if the volume was set successfully, false otherwise
pub fn set_volume_percentage(percentage: f64) -> bool {
    if let Ok(control) = get_global_volume_control() {
        return control.lock().set_volume_percent(percentage).is_ok();
    }
    false
}

/// Get the current volume in decibels
///
/// # Returns
///
/// The current volume in dB, or None if volume control is not available or doesn't support dB
pub fn get_volume_db() -> Option<f64> {
    get_global_volume_control().ok()?.lock().get_volume_db().ok()
}

/// Set the volume in decibels
///
/// # Arguments
///
/// * `db` - Volume level in decibels
///
/// # Returns
///
/// true if the volume was set successfully, false otherwise
pub fn set_volume_db(db: f64) -> bool {
    if let Ok(control) = get_global_volume_control() {
        return control.lock().set_volume_db(db).is_ok();
    }
    false
}

/// Check if volume control is available
///
/// # Returns
///
/// true if volume control is available and functional, false otherwise
pub fn is_volume_control_available() -> bool {
    if let Ok(control) = get_global_volume_control() {
        return control.lock().is_available();
    }
    false
}

/// Get volume control information
///
/// # Returns
///
/// VolumeControlInfo if available, None otherwise
pub fn get_volume_control_info() -> Option<crate::helpers::volume::VolumeControlInfo> {
    Some(get_global_volume_control().ok()?.lock().get_info())
}

/// Start monitoring volume changes on the global volume control
///
/// # Returns
///
/// Ok(()) if monitoring was started successfully, or an error if monitoring cannot be started
pub fn start_volume_change_monitoring() -> Result<(), Box<dyn std::error::Error>> {
    log::debug!("Starting global volume change monitoring");
    let control = get_global_volume_control()?;
    let control = control.lock();

    let supports_monitoring = control.supports_change_monitoring();
    log::debug!("Global volume control supports change monitoring: {}", supports_monitoring);

    control.start_change_monitoring()
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

/// Check if the current volume control supports change monitoring
///
/// # Returns
///
/// true if change monitoring is supported, false otherwise
pub fn supports_volume_change_monitoring() -> bool {
    if let Ok(control) = get_global_volume_control() {
        return control.lock().supports_change_monitoring();
    }
    false
}

// Pure function for extracting dummy volume control configuration
#[cfg(test)]
fn extract_dummy_volume_config(volume_config: &Value) -> (String, String, f64) {
    let internal_name = volume_config
        .get("internal_name")
        .and_then(|v| v.as_str())
        .unwrap_or("dummy")
        .to_string();

    let display_name = volume_config
        .get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Dummy Volume Control")
        .to_string();

    let initial_percent = volume_config
        .get("initial_percent")
        .and_then(|v| v.as_f64())
        .unwrap_or(50.0);

    (internal_name, display_name, initial_percent)
}

// Pure function for checking if volume control is enabled
#[cfg(test)]
fn is_volume_control_enabled(volume_config: &Value) -> bool {
    volume_config
        .get("enable")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

// Pure function for extracting control type
#[cfg(test)]
fn extract_control_type(volume_config: &Value) -> String {
    volume_config
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("dummy")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Since GLOBAL_VOLUME_CONTROL is a OnceCell, we can only set it once per test run
    // These tests demonstrate the functionality but may interfere with each other
    // In a real application, you'd want separate instances for testing

    #[test]
    fn test_volume_control_api() {
        // Test the volume control API functions regardless of which control is initialized

        // These functions should not panic even if no volume control is available
        let _available = is_volume_control_available();
        let _volume = get_volume_percentage();
        let _db_volume = get_volume_db();
        let _info = get_volume_control_info();

        // Set operations should return false if no control is available, true if successful
        let set_result = set_volume_percentage(75.0);
        let set_db_result = set_volume_db(-10.0);

        // These are successful if they don't panic
        println!("Volume control available: {}", _available);
        println!("Set percentage result: {}", set_result);
        println!("Set dB result: {}", set_db_result);
    }

    #[test]
    fn test_dummy_volume_control_creation() {
        // Test creating dummy volume controls directly
        let dummy_control = DummyVolumeControl::new(
            "test".to_string(),
            "Test Control".to_string(),
            50.0
        );

        assert!(dummy_control.is_available());
        assert_eq!(dummy_control.get_volume_percent().unwrap(), 50.0);

        let info = dummy_control.get_info();
        assert_eq!(info.internal_name, "test");
        assert_eq!(info.display_name, "Test Control");
        assert!(info.decibel_range.is_some());
    }

    #[test]
    fn test_config_parsing() {
        // Test configuration parsing without setting global state
        let dummy_config = json!({
            "services": {
                "volume": {
                    "enable": true,
                    "type": "dummy",
                    "display_name": "Test Volume"
                }
            }
        });

        let volume_config = get_service_config(&dummy_config, "volume");
        assert!(volume_config.is_some());

        let enabled = volume_config.unwrap()
            .get("enable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(enabled);
    }

    #[test]
    fn test_disabled_config() {
        let disabled_config = json!({
            "services": {
                "volume": {
                    "enable": false,
                    "type": "dummy"
                }
            }
        });

        let volume_config = get_service_config(&disabled_config, "volume");
        assert!(volume_config.is_some());

        let enabled = volume_config.unwrap()
            .get("enable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        assert!(!enabled);
    }

    // Pure function tests
    #[test]
    fn test_extract_dummy_volume_config_with_all_fields() {
        let config = json!({
            "internal_name": "my_dummy",
            "display_name": "My Dummy Volume",
            "initial_percent": 75.0
        });

        let (internal_name, display_name, initial_percent) = extract_dummy_volume_config(&config);

        assert_eq!(internal_name, "my_dummy");
        assert_eq!(display_name, "My Dummy Volume");
        assert_eq!(initial_percent, 75.0);
    }

    #[test]
    fn test_extract_dummy_volume_config_with_defaults() {
        let config = json!({});

        let (internal_name, display_name, initial_percent) = extract_dummy_volume_config(&config);

        assert_eq!(internal_name, "dummy");
        assert_eq!(display_name, "Dummy Volume Control");
        assert_eq!(initial_percent, 50.0);
    }

    #[test]
    fn test_extract_dummy_volume_config_with_partial_fields() {
        let config = json!({
            "internal_name": "custom_dummy",
            "initial_percent": 25.0
        });

        let (internal_name, display_name, initial_percent) = extract_dummy_volume_config(&config);

        assert_eq!(internal_name, "custom_dummy");
        assert_eq!(display_name, "Dummy Volume Control"); // Default
        assert_eq!(initial_percent, 25.0);
    }

    #[test]
    fn test_extract_dummy_volume_config_extreme_percentages() {
        let config_min = json!({ "initial_percent": 0.0 });
        let (_, _, percent_min) = extract_dummy_volume_config(&config_min);
        assert_eq!(percent_min, 0.0);

        let config_max = json!({ "initial_percent": 100.0 });
        let (_, _, percent_max) = extract_dummy_volume_config(&config_max);
        assert_eq!(percent_max, 100.0);

        let config_over = json!({ "initial_percent": 150.0 });
        let (_, _, percent_over) = extract_dummy_volume_config(&config_over);
        assert_eq!(percent_over, 150.0); // Config can have out-of-range values
    }

    #[test]
    fn test_is_volume_control_enabled_default() {
        let config = json!({});
        assert!(is_volume_control_enabled(&config)); // Defaults to enabled
    }

    #[test]
    fn test_is_volume_control_enabled_explicit() {
        let config_enabled = json!({ "enable": true });
        assert!(is_volume_control_enabled(&config_enabled));

        let config_disabled = json!({ "enable": false });
        assert!(!is_volume_control_enabled(&config_disabled));
    }

    #[test]
    fn test_extract_control_type_defaults() {
        let config = json!({});
        assert_eq!(extract_control_type(&config), "dummy");
    }

    #[test]
    fn test_extract_control_type_explicit() {
        let config_dummy = json!({ "type": "dummy" });
        assert_eq!(extract_control_type(&config_dummy), "dummy");

        let config_alsa = json!({ "type": "alsa" });
        assert_eq!(extract_control_type(&config_alsa), "alsa");

        let config_custom = json!({ "type": "custom_type" });
        assert_eq!(extract_control_type(&config_custom), "custom_type");
    }

    // Convenience function tests with specific scenarios
    #[test]
    fn test_set_and_get_volume_percentage() {
        // Create a dummy control directly for testing
        let dummy_control = DummyVolumeControl::new(
            "test".to_string(),
            "Test".to_string(),
            50.0
        );

        // Test get on dummy
        assert_eq!(dummy_control.get_volume_percent().unwrap(), 50.0);

        // Test set on dummy
        assert!(dummy_control.set_volume_percent(75.0).is_ok());
        assert_eq!(dummy_control.get_volume_percent().unwrap(), 75.0);
    }

    #[test]
    fn test_volume_percentage_boundary_values() {
        let dummy = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 50.0);

        // Minimum
        assert!(dummy.set_volume_percent(0.0).is_ok());
        assert_eq!(dummy.get_volume_percent().unwrap(), 0.0);

        // Maximum
        assert!(dummy.set_volume_percent(100.0).is_ok());
        assert_eq!(dummy.get_volume_percent().unwrap(), 100.0);

        // Mid-range
        assert!(dummy.set_volume_percent(50.0).is_ok());
        assert_eq!(dummy.get_volume_percent().unwrap(), 50.0);
    }

    #[test]
    fn test_set_and_get_volume_db() {
        let dummy = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 50.0);

        // Test getting dB value (should work with dummy's default range)
        let db = dummy.get_volume_db().unwrap();
        assert!(db >= -120.0 && db <= 0.0); // Default range for dummy

        // Test setting dB value
        assert!(dummy.set_volume_db(-50.0).is_ok());
        let new_db = dummy.get_volume_db().unwrap();
        assert!((new_db - (-50.0)).abs() < 0.1); // Allow small tolerance
    }

    #[test]
    fn test_volume_control_availability_checks() {
        let available_control = DummyVolumeControl::new(
            "test".to_string(),
            "Test".to_string(),
            50.0
        );
        assert!(available_control.is_available());

        let mut unavailable_control = DummyVolumeControl::new(
            "test".to_string(),
            "Test".to_string(),
            50.0
        );
        unavailable_control.set_available(false);
        assert!(!unavailable_control.is_available());
    }

    #[test]
    fn test_get_volume_control_info() {
        let dummy = DummyVolumeControl::new("my_control".to_string(), "My Display".to_string(), 60.0);

        let info = dummy.get_info();
        assert_eq!(info.internal_name, "my_control");
        assert_eq!(info.display_name, "My Display");
        assert!(info.decibel_range.is_some());

        let db_range = info.decibel_range.unwrap();
        assert!(db_range.min_db <= -120.0);
        assert!(db_range.max_db >= 0.0);
    }

    #[test]
    fn test_volume_percentage_invalid_ranges() {
        let dummy = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 50.0);

        // Negative percentage should fail
        let result = dummy.set_volume_percent(-10.0);
        assert!(result.is_err());

        // Over 100% should fail
        let result = dummy.set_volume_percent(110.0);
        assert!(result.is_err());

        // NaN should fail
        let result = dummy.set_volume_percent(f64::NAN);
        assert!(result.is_err());

        // Infinity should fail
        let result = dummy.set_volume_percent(f64::INFINITY);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_sequential_volume_changes() {
        let dummy = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 20.0);

        // Start at 20%
        assert_eq!(dummy.get_volume_percent().unwrap(), 20.0);

        // Change to 30%
        assert!(dummy.set_volume_percent(30.0).is_ok());
        assert_eq!(dummy.get_volume_percent().unwrap(), 30.0);

        // Change to 80%
        assert!(dummy.set_volume_percent(80.0).is_ok());
        assert_eq!(dummy.get_volume_percent().unwrap(), 80.0);

        // Change back to 10%
        assert!(dummy.set_volume_percent(10.0).is_ok());
        assert_eq!(dummy.get_volume_percent().unwrap(), 10.0);
    }

    #[test]
    fn test_volume_percentage_precision() {
        let dummy = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 50.0);

        // Test decimal precision
        let precise_value = 33.333333;
        assert!(dummy.set_volume_percent(precise_value).is_ok());
        let retrieved = dummy.get_volume_percent().unwrap();
        assert!((retrieved - precise_value).abs() < 0.000001);
    }

    #[test]
    fn test_percentage_db_round_trip_conversion() {
        let dummy = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 50.0);

        // Set to 50% and get dB
        assert!(dummy.set_volume_percent(50.0).is_ok());
        let db_value = dummy.get_volume_db().unwrap();

        // Convert back through percentage
        assert!(dummy.set_volume_db(db_value).is_ok());
        let final_percent = dummy.get_volume_percent().unwrap();

        // Should be close to original
        assert!((final_percent - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_volume_control_operations_unavailable() {
        let mut dummy = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 50.0);
        dummy.set_available(false);

        // Operations should fail when unavailable
        assert!(dummy.set_volume_percent(75.0).is_err());
        assert!(dummy.set_volume_db(-20.0).is_err());
    }

    #[test]
    fn test_volume_control_info_with_different_initial_values() {
        let ctrl_0 = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 0.0);
        let ctrl_50 = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 50.0);
        let ctrl_100 = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 100.0);

        // Initial values should be preserved
        assert_eq!(ctrl_0.get_volume_percent().unwrap(), 0.0);
        assert_eq!(ctrl_50.get_volume_percent().unwrap(), 50.0);
        assert_eq!(ctrl_100.get_volume_percent().unwrap(), 100.0);

        // But info should be the same regardless
        let info_0 = ctrl_0.get_info();
        let info_50 = ctrl_50.get_info();
        let info_100 = ctrl_100.get_info();

        assert_eq!(info_0.display_name, info_50.display_name);
        assert_eq!(info_50.display_name, info_100.display_name);
    }

    #[test]
    fn test_dummy_volume_control_with_empty_names() {
        let dummy = DummyVolumeControl::new(String::new(), String::new(), 50.0);

        let info = dummy.get_info();
        assert_eq!(info.internal_name, "");
        assert_eq!(info.display_name, "");
        assert!(dummy.is_available());
        assert!(dummy.get_volume_percent().is_ok());
    }

    #[test]
    fn test_dummy_control_supports_change_monitoring() {
        let dummy = DummyVolumeControl::new("test".to_string(), "Test".to_string(), 50.0);

        // Dummy control behavior (may vary based on implementation)
        let supports = dummy.supports_change_monitoring();
        let start_result = dummy.start_change_monitoring();

        // Should be consistent - if it supports, start should succeed
        if supports {
            assert!(start_result.is_ok());
        } else {
            assert!(start_result.is_err());
        }
    }
}
