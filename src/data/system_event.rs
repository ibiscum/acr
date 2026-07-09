use serde::{Serialize, Deserialize};
use std::fmt;

/// Represents different system-level events that can occur
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEvent {
    /// Volume control has changed
    VolumeChanged {
        /// Name of the volume control that changed
        control_name: String,
        /// Display name of the control
        display_name: String,
        /// New volume percentage (0-100)
        percentage: f64,
        /// New volume in decibels (if supported)
        decibels: Option<f64>,
        /// Raw control value (implementation specific)
        raw_value: Option<i64>,
    },
}

impl SystemEvent {
    /// Create a new volume changed event
    pub fn volume_changed(
        control_name: String,
        display_name: String,
        percentage: f64,
        decibels: Option<f64>,
        raw_value: Option<i64>,
    ) -> Self {
        SystemEvent::VolumeChanged {
            control_name,
            display_name,
            percentage,
            decibels,
            raw_value,
        }
    }

    /// Get the event type as a string for filtering
    pub fn event_type(&self) -> &'static str {
        match self {
            SystemEvent::VolumeChanged { .. } => "volume_changed",
        }
    }
}

impl fmt::Display for SystemEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SystemEvent::VolumeChanged { control_name, percentage, .. } => {
                write!(f, "Volume changed on {}: {:.1}%", control_name, percentage)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SystemEvent;

    #[test]
    fn volume_changed_constructor_sets_all_fields() {
        let e = SystemEvent::volume_changed(
            "Master".to_string(),
            "Master Volume".to_string(),
            75.0,
            Some(-10.5),
            Some(150),
        );

        match e {
            SystemEvent::VolumeChanged {
                control_name,
                display_name,
                percentage,
                decibels,
                raw_value,
            } => {
                assert_eq!(control_name, "Master");
                assert_eq!(display_name, "Master Volume");
                assert_eq!(percentage, 75.0);
                assert_eq!(decibels, Some(-10.5));
                assert_eq!(raw_value, Some(150));
            }
        }
    }

    #[test]
    fn event_type_returns_canonical_string() {
        let e = SystemEvent::volume_changed("ctrl".to_string(), "Ctrl".to_string(), 50.0, None, None);
        assert_eq!(e.event_type(), "volume_changed");
    }

    #[test]
    fn display_includes_control_name_and_percentage() {
        let e = SystemEvent::volume_changed("Master".to_string(), "Master Volume".to_string(), 42.5, None, None);
        let s = e.to_string();
        assert!(s.contains("Master"), "{}", s);
        assert!(s.contains("42.5"), "{}", s);
    }
}
