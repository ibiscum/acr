/// Player commands that can be sent to media players
use serde::{Serialize, Deserialize};
use super::LoopMode;

/// Metadata for tracks being added to the queue
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueueTrackMetadata {
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum PlayerCommand {
    /// Simple playback commands
    #[serde(rename = "play")]
    #[default]
    Play,

    #[serde(rename = "pause")]
    Pause,

    #[serde(rename = "playpause")]
    PlayPause,

    #[serde(rename = "stop")]
    Stop,

    #[serde(rename = "next")]
    Next,

    #[serde(rename = "previous")]
    Previous,

    /// Commands with additional arguments
    #[serde(rename = "set_loop")]
    SetLoopMode(LoopMode),

    #[serde(rename = "seek")]
    Seek(f64),

    #[serde(rename = "set_random")]
    SetRandom(bool),

    /// Kill (forcefully terminate) the player
    #[serde(rename = "kill")]
    Kill,

    /// Queue commands
    #[serde(rename = "queue_tracks")]
    QueueTracks {
        /// Track URIs to add to the queue
        uris: Vec<String>,
        /// Whether to insert at beginning (true) or append at end (false)
        insert_at_beginning: bool,
        /// Optional metadata for each URI (title and cover art URL)
        #[serde(default)]
        metadata: Vec<Option<QueueTrackMetadata>>,
    },
      #[serde(rename = "remove_track")]
    RemoveTrack(usize), // Changed from String to usize for position-based removal

    #[serde(rename = "clear_queue")]
    ClearQueue,

    #[serde(rename = "play_queue_index")]
    PlayQueueIndex(usize), // Play specific track in the queue by its index
}

impl PlayerCommand {
    /// Build a queue command while enforcing URI/metadata length consistency.
    pub fn queue_tracks(
        uris: Vec<String>,
        insert_at_beginning: bool,
        metadata: Vec<Option<QueueTrackMetadata>>,
    ) -> Result<Self, String> {
        if !metadata.is_empty() && metadata.len() != uris.len() {
            return Err(format!(
                "queue metadata count ({}) must match uri count ({})",
                metadata.len(),
                uris.len()
            ));
        }

        Ok(Self::QueueTracks {
            uris,
            insert_at_beginning,
            metadata,
        })
    }

    /// Validate command-level invariants after construction/deserialization.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::QueueTracks { uris, metadata, .. }
                if !metadata.is_empty() && metadata.len() != uris.len() =>
            {
                Err(format!(
                    "queue metadata count ({}) must match uri count ({})",
                    metadata.len(),
                    uris.len()
                ))
            }
            _ => Ok(()),
        }
    }
}


impl std::fmt::Display for PlayerCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayerCommand::Play => write!(f, "play"),
            PlayerCommand::Pause => write!(f, "pause"),
            PlayerCommand::PlayPause => write!(f, "playpause"),
            PlayerCommand::Stop => write!(f, "stop"),
            PlayerCommand::Next => write!(f, "next"),
            PlayerCommand::Previous => write!(f, "previous"),
            PlayerCommand::SetLoopMode(mode) => write!(f, "set_loop:{}", mode),
            PlayerCommand::Seek(position) => write!(f, "seek:{}", position),
            PlayerCommand::SetRandom(enabled) => write!(f, "set_random:{}", if *enabled { "on" } else { "off" }),
            PlayerCommand::Kill => write!(f, "kill"),
            PlayerCommand::QueueTracks { insert_at_beginning, .. } => {
                if *insert_at_beginning {
                    write!(f, "queue_tracks_beginning")
                } else {
                    write!(f, "queue_tracks_end")
                }
            }
            PlayerCommand::RemoveTrack(position) => write!(f, "remove_track:{}", position),
            PlayerCommand::ClearQueue => write!(f, "clear_queue"),
            PlayerCommand::PlayQueueIndex(index) => write!(f, "play_queue_index:{}", index),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_queue_tracks_validation_rejects_mismatched_metadata_len() {
        let command = PlayerCommand::queue_tracks(
            vec!["uri:1".to_string(), "uri:2".to_string()],
            false,
            vec![None],
        );

        assert!(command.is_err());
    }

    #[test]
    fn test_queue_tracks_validation_accepts_empty_or_matching_metadata_len() {
        let cmd_without_metadata = PlayerCommand::queue_tracks(
            vec!["uri:1".to_string(), "uri:2".to_string()],
            false,
            vec![],
        )
        .unwrap();
        assert!(cmd_without_metadata.validate().is_ok());

        let cmd_with_matching_metadata = PlayerCommand::queue_tracks(
            vec!["uri:1".to_string(), "uri:2".to_string()],
            true,
            vec![None, Some(QueueTrackMetadata { metadata: Default::default() })],
        )
        .unwrap();
        assert!(cmd_with_matching_metadata.validate().is_ok());
    }

    #[test]
    fn test_display_for_renamed_commands() {
        assert_eq!(PlayerCommand::PlayPause.to_string(), "playpause");
        assert_eq!(PlayerCommand::SetLoopMode(LoopMode::Track).to_string(), "set_loop:song");
        assert_eq!(PlayerCommand::SetRandom(true).to_string(), "set_random:on");
        assert_eq!(PlayerCommand::SetRandom(false).to_string(), "set_random:off");
        assert_eq!(PlayerCommand::RemoveTrack(3).to_string(), "remove_track:3");
        assert_eq!(PlayerCommand::PlayQueueIndex(7).to_string(), "play_queue_index:7");
    }

    #[test]
    fn test_serde_for_simple_and_data_commands() {
        let playpause = serde_json::to_value(&PlayerCommand::PlayPause).unwrap();
        assert_eq!(playpause, json!("playpause"));

        let set_random = serde_json::to_value(&PlayerCommand::SetRandom(true)).unwrap();
        assert_eq!(set_random, json!({"set_random": true}));

        let parsed: PlayerCommand = serde_json::from_value(json!({"set_loop": "playlist"})).unwrap();
        assert_eq!(parsed, PlayerCommand::SetLoopMode(LoopMode::Playlist));
    }
}
