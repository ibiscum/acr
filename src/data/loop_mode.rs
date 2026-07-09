/// Loop mode enumeration for playback
use serde::{Serialize, Deserialize};
use strum_macros::EnumString;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, EnumString)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum LoopMode {
    /// No loop
    #[serde(rename = "no")]
    #[strum(serialize = "no")]
    #[default]
    None,
    /// Loop current track/song
    #[serde(rename = "song")]
    #[strum(serialize = "song")]
    Track,
    /// Loop entire playlist
    #[serde(rename = "playlist")]
    #[strum(serialize = "playlist")]
    Playlist,
}


impl std::fmt::Display for LoopMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Return the value as a string for backwards compatibility
        match self {
            LoopMode::None => write!(f, "no"),
            LoopMode::Track => write!(f, "song"),
            LoopMode::Playlist => write!(f, "playlist"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LoopMode;
    use std::str::FromStr;

    #[test]
    fn from_str_matches_public_wire_values() {
        assert_eq!(LoopMode::from_str("no").ok(), Some(LoopMode::None));
        assert_eq!(LoopMode::from_str("song").ok(), Some(LoopMode::Track));
        assert_eq!(LoopMode::from_str("playlist").ok(), Some(LoopMode::Playlist));
    }

    #[test]
    fn display_matches_wire_values() {
        assert_eq!(LoopMode::None.to_string(), "no");
        assert_eq!(LoopMode::Track.to_string(), "song");
        assert_eq!(LoopMode::Playlist.to_string(), "playlist");
    }
}
