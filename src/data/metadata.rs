use serde::{Serialize, Deserialize};

/// Metadata for Artists including external IDs and image URLs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArtistMeta {
    /// MusicBrainz ID for the artist
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mbid: Vec<String>,

    /// Thumbnail image URL or filename
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub thumb_url: Vec<String>,

    /// Banner/background image URL or filename
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub banner_url: Vec<String>,

    /// Artist biography text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biography: Option<String>,

    /// Source where the biography was obtained from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biography_source: Option<String>,

    /// Musical genres associated with this artist
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,

    /// Indicates if this is a partial match (only some artists in a multi-artist name found)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_partial_match: bool,
}

impl ArtistMeta {
    /// Create a new empty ArtistMeta
    pub fn new() -> Self {
        Self {
            mbid: Vec::new(),
            thumb_url: Vec::new(),
            banner_url: Vec::new(),
            biography: None,
            biography_source: None,
            genres: Vec::new(),
            is_partial_match: false,
        }
    }

    /// Add a MusicBrainz ID
    pub fn add_mbid(&mut self, mbid: String) {
        if !self.mbid.contains(&mbid) {
            self.mbid.push(mbid);
        }
    }

    /// Add a thumbnail URL or filename
    pub fn add_thumb_url(&mut self, url: String) {
        if !self.thumb_url.contains(&url) {
            self.thumb_url.push(url);
        }
    }

    /// Add a banner URL or filename
    pub fn add_banner_url(&mut self, url: String) {
        if !self.banner_url.contains(&url) {
            self.banner_url.push(url);
        }
    }

    /// Add a genre if it doesn't already exist
    pub fn add_genre(&mut self, genre: String) {
        if !self.genres.contains(&genre) {
            self.genres.push(genre);
        }
    }

    /// Check if this metadata contains any actual data
    pub fn is_empty(&self) -> bool {
        self.mbid.is_empty() &&
        self.thumb_url.is_empty() &&
        self.banner_url.is_empty() &&
        self.biography.is_none() &&
        self.biography_source.is_none() &&
        self.genres.is_empty() &&
        !self.is_partial_match
    }

    /// Clear all metadata
    pub fn clear(&mut self) {
        self.mbid.clear();
        self.thumb_url.clear();
        self.banner_url.clear();
        self.biography = None;
        self.biography_source = None;
        self.genres.clear();
        self.is_partial_match = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn test_biography_source_field() {
        let mut meta = ArtistMeta::new();

        // Initially both biography and source should be None
        assert!(meta.biography.is_none());
        assert!(meta.biography_source.is_none());
        assert!(meta.is_empty());

        // Setting biography from LastFM
        meta.biography = Some("Artist biography from Last.fm".to_string());
        meta.biography_source = Some("LastFM".to_string());

        assert!(!meta.is_empty());
        assert_eq!(meta.biography.as_ref().unwrap(), "Artist biography from Last.fm");
        assert_eq!(meta.biography_source.as_ref().unwrap(), "LastFM");

        // Test serialization includes both fields with expected values
        let serialized: Value = serde_json::to_value(&meta).unwrap();
        assert_eq!(serialized.get("biography"), Some(&json!("Artist biography from Last.fm")));
        assert_eq!(serialized.get("biography_source"), Some(&json!("LastFM")));

        // Clear should reset both fields
        meta.clear();
        assert!(meta.biography.is_none());
        assert!(meta.biography_source.is_none());
        assert!(meta.is_empty());
    }

    #[test]
    fn test_add_methods_deduplicate_values() {
        let mut meta = ArtistMeta::new();

        meta.add_mbid("abc-123".to_string());
        meta.add_mbid("abc-123".to_string());
        assert_eq!(meta.mbid, vec!["abc-123".to_string()]);

        meta.add_thumb_url("thumb.jpg".to_string());
        meta.add_thumb_url("thumb.jpg".to_string());
        assert_eq!(meta.thumb_url, vec!["thumb.jpg".to_string()]);

        meta.add_banner_url("banner.jpg".to_string());
        meta.add_banner_url("banner.jpg".to_string());
        assert_eq!(meta.banner_url, vec!["banner.jpg".to_string()]);

        meta.add_genre("Rock".to_string());
        meta.add_genre("Rock".to_string());
        assert_eq!(meta.genres, vec!["Rock".to_string()]);
    }

    #[test]
    fn test_is_empty_semantics() {
        let mut meta = ArtistMeta::new();
        assert!(meta.is_empty());

        meta.biography_source = Some("LastFM".to_string());
        assert!(!meta.is_empty());

        meta.clear();
        assert!(meta.is_empty());

        meta.is_partial_match = true;
        assert!(!meta.is_empty());
    }
}
