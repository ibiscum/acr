/// Class representing metadata for a song/track
use std::collections::HashMap;
use std::fmt; // Added for Display
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Song {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_artist: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tracks: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>, // in seconds

    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_art_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>, // e.g., "spotify", "local", "radio"

    #[serde(skip_serializing_if = "Option::is_none")]
    pub liked: Option<bool>, // Indicates if the song is liked or favorited

    #[serde(skip_serializing_if = "Option::is_none")]
    pub composer: Option<String>,

    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

// The to_json method is now provided by the Serializable trait
// which is automatically implemented for all types that implement Serialize

impl PartialEq for Song {
    /// Identity-style equality used for matching the currently playing item.
    ///
    /// This intentionally does not compare enrichment fields like cover art,
    /// liked status, duration, genres, or metadata. Those fields can change
    /// over time for the same song identity and should be handled by explicit
    /// update/merge logic.
    fn eq(&self, other: &Self) -> bool {
        // Compare only title, artist and album for equality
        self.title == other.title &&
        self.artist == other.artist &&
        self.album == other.album
    }
}

impl fmt::Display for Song {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut display_str = self.title.as_deref().unwrap_or("Unknown Title").to_string();
        if let Some(artist_name) = &self.artist {
            if !artist_name.is_empty() {
                display_str.push_str(" by ");
                display_str.push_str(artist_name);
            }
        }
        if let Some(album_name) = &self.album {
            display_str.push_str(&format!(" (Album: {})", album_name));
        }
        write!(f, "{}", display_str)
    }
}

#[cfg(test)]
mod tests {
    use super::Song;
    use serde_json::json;

    #[test]
    fn identity_equality_ignores_enrichment_fields() {
        let left = Song {
            title: Some("Track A".to_string()),
            artist: Some("Artist A".to_string()),
            album: Some("Album A".to_string()),
            cover_art_url: Some("https://example.com/cover1.jpg".to_string()),
            liked: Some(true),
            duration: Some(123.4),
            ..Song::default()
        };

        let right = Song {
            title: Some("Track A".to_string()),
            artist: Some("Artist A".to_string()),
            album: Some("Album A".to_string()),
            cover_art_url: Some("https://example.com/cover2.jpg".to_string()),
            liked: Some(false),
            duration: Some(987.6),
            ..Song::default()
        };

        assert_eq!(left, right);
    }

    #[test]
    fn identity_equality_detects_identity_changes() {
        let base = Song {
            title: Some("Track A".to_string()),
            artist: Some("Artist A".to_string()),
            album: Some("Album A".to_string()),
            ..Song::default()
        };

        let different_title = Song {
            title: Some("Track B".to_string()),
            ..base.clone()
        };

        let different_artist = Song {
            artist: Some("Artist B".to_string()),
            ..base.clone()
        };

        let different_album = Song {
            album: Some("Album B".to_string()),
            ..base.clone()
        };

        assert_ne!(base, different_title);
        assert_ne!(base, different_artist);
        assert_ne!(base, different_album);
    }

    #[test]
    fn display_formats_title_artist_and_album() {
        let full = Song {
            title: Some("Track A".to_string()),
            artist: Some("Artist A".to_string()),
            album: Some("Album A".to_string()),
            ..Song::default()
        };
        assert_eq!(full.to_string(), "Track A by Artist A (Album: Album A)");

        let unknown = Song::default();
        assert_eq!(unknown.to_string(), "Unknown Title");
    }

    #[test]
    fn serialization_omits_none_and_empty_fields() {
        let song = Song::default();
        let serialized = serde_json::to_value(&song).unwrap();

        assert_eq!(serialized, json!({}));
    }
}
