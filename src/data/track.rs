use serde::{Serialize, Deserialize};

use super::Identifier;
use crate::data::song::Song;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackConversionError {
    MissingTitle,
    InvalidTrackNumber(i32),
}

/// Represents a Track in an album
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    // ID might be used by some backends
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Identifier>,

    /// Disc number (as a string to support formats like "1/2")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc_number: Option<String>,
    /// Track number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_number: Option<u16>,
    /// Track name
    pub name: String,
    /// Track artist (only stored if different from album artist)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    /// URI/filename of the track (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

impl Track {
    /// Create a new Track
    pub fn new(disc_number: Option<String>, track_number: Option<u16>, name: String) -> Self {
        Self {
            id: None,
            disc_number,
            track_number,
            name,
            artist: None,
            uri: None,
        }
    }

    /// Create a new Track with just the name (convenience method)
    pub fn with_name(name: String) -> Self {
        Self {
            id: None,
            disc_number: None,
            track_number: None,
            name,
            artist: None,
            uri: None,
        }
    }

    /// Create a new Track with an artist
    pub fn with_artist(disc_number: Option<String>, track_number: Option<u16>, name: String, artist: String, album_artist: Option<&str>) -> Self {
        // Only store artist if it differs from the album artist
        let track_artist = if let Some(album_artist) = album_artist {
            if artist != album_artist {
                // log if artist is different from album artist
                log::debug!("Track artist '{}' differs from album artist '{}'", artist, album_artist);
                Some(artist)
            } else {
                None
            }
        } else {
            Some(artist)
        };

        Self {
            id: None,
            disc_number,
            track_number,
            name,
            artist: track_artist,
            uri: None,
        }
    }
      /// Set the URI/filename for this track
    pub fn with_uri(mut self, uri: String) -> Self {
        self.uri = Some(uri);
        self
    }

    /// Set the ID for this track
    pub fn with_id(mut self, id: crate::data::Identifier) -> Self {
        self.id = Some(id);
        self
    }
}

impl From<Track> for Song {
    fn from(track: Track) -> Self {
        Song {
            title: Some(track.name),
            artist: track.artist,
            track_number: track.track_number.map(i32::from),
            stream_url: track.uri,
            ..Song::default()
        }
    }
}

impl From<&Track> for Song {
    fn from(track: &Track) -> Self {
        Song {
            title: Some(track.name.clone()),
            artist: track.artist.clone(),
            track_number: track.track_number.map(i32::from),
            stream_url: track.uri.clone(),
            ..Song::default()
        }
    }
}

impl TryFrom<Song> for Track {
    type Error = TrackConversionError;

    fn try_from(song: Song) -> Result<Self, Self::Error> {
        let Some(name) = song.title else {
            return Err(TrackConversionError::MissingTitle);
        };

        let track_number = match song.track_number {
            Some(num) => Some(u16::try_from(num).map_err(|_| TrackConversionError::InvalidTrackNumber(num))?),
            None => None,
        };

        Ok(Self {
            id: None,
            disc_number: None,
            track_number,
            name,
            artist: song.artist,
            uri: song.stream_url,
        })
    }
}

impl TryFrom<&Song> for Track {
    type Error = TrackConversionError;

    fn try_from(song: &Song) -> Result<Self, Self::Error> {
        let Some(name) = &song.title else {
            return Err(TrackConversionError::MissingTitle);
        };

        let track_number = match song.track_number {
            Some(num) => Some(u16::try_from(num).map_err(|_| TrackConversionError::InvalidTrackNumber(num))?),
            None => None,
        };

        Ok(Self {
            id: None,
            disc_number: None,
            track_number,
            name: name.clone(),
            artist: song.artist.clone(),
            uri: song.stream_url.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Track, TrackConversionError};
    use crate::data::song::Song;

    #[test]
    fn track_to_song_maps_common_fields() {
        let track = Track::new(Some("1".to_string()), Some(7), "Test Track".to_string())
            .with_uri("file:///tmp/test.flac".to_string());

        let song: Song = track.into();
        assert_eq!(song.title.as_deref(), Some("Test Track"));
        assert_eq!(song.track_number, Some(7));
        assert_eq!(song.stream_url.as_deref(), Some("file:///tmp/test.flac"));
    }

    #[test]
    fn song_to_track_requires_title() {
        let song = Song::default();
        let result = Track::try_from(song);
        assert!(matches!(result, Err(TrackConversionError::MissingTitle)));
    }

    #[test]
    fn song_to_track_rejects_invalid_track_number() {
        let song = Song {
            title: Some("Bad Track".to_string()),
            track_number: Some(-1),
            ..Song::default()
        };
        let result = Track::try_from(song);
        assert!(matches!(result, Err(TrackConversionError::InvalidTrackNumber(-1))));
    }
}
