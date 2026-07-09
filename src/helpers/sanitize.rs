use deunicode;

/// Safely truncate a UTF-8 string to a maximum number of characters
///
/// This function ensures that the truncation happens at character boundaries,
/// not byte boundaries, preventing panics when dealing with multi-byte UTF-8 characters.
///
/// # Arguments
/// * `input` - The string to truncate
/// * `max_chars` - Maximum number of characters to keep
///
/// # Returns
/// * `&str` - A slice of the input string, truncated to at most `max_chars` characters
///
/// # Example
/// ```
/// use audiocontrol::helpers::sanitize::safe_truncate;
/// let truncated = safe_truncate("Hello, 世界!", 8);
/// assert_eq!(truncated, "Hello, 世");
/// ```
pub fn safe_truncate(input: &str, max_chars: usize) -> &str {
    if input.len() <= max_chars {
        input
    } else {
        // Find a safe truncation point at a character boundary
        match input.char_indices().nth(max_chars) {
            Some((byte_index, _)) => &input[..byte_index],
            None => input, // Less than max_chars characters total
        }
    }
}

/// Create a "clean" filename without unicode characters (converted to ascii),
/// special characters or double spaces
/// convert to lowercase and trim whitespace
pub fn filename_from_string(input: &str) -> String {
    // Convert to ASCII (remove diacritics and other non-ascii characters)
    let ascii_name = deunicode::deunicode(input);

    // Keep only alphanumeric characters and spaces, replace others with spaces
    let mut clean_name = String::with_capacity(ascii_name.len());
    for c in ascii_name.chars() {
        if c.is_alphanumeric() || c == ' ' {
            clean_name.push(c);
        } else {
            clean_name.push(' ');
        }
    }

    // Convert to lowercase
    let lowercase_name = clean_name.to_lowercase();

    // Remove double spaces
    let mut result = String::with_capacity(lowercase_name.len());
    let mut last_was_space = false;

    for c in lowercase_name.chars() {
        if c == ' ' {
            if !last_was_space {
                result.push(c);
            }
            last_was_space = true;
        } else {
            result.push(c);
            last_was_space = false;
        }
    }

    // Trim whitespace
    result.trim().to_string()
}

/// Create a key for an album in the format "<artist>/<album>"
/// If there are multiple artists, concatenate them with "+"
///
/// # Arguments
/// * `album` - The album object
///
/// # Returns
/// * `String` - A key in the format "<sanitized_artist>/<sanitized_album>"
pub fn key_from_album(album: &crate::data::Album) -> String {
    // Get the list of artists for the album
    let artists = {
        let guard = album.artists.lock();
        guard.clone()
    };

    // Sanitize each artist name and drop entries that collapse to empty strings.
    let sanitized_artists = artists
        .iter()
        .map(|artist| filename_from_string(artist))
        .filter(|artist| !artist.is_empty())
        .collect::<Vec<String>>();

    let artists_key = if sanitized_artists.is_empty() {
        "unknown".to_string()
    } else {
        sanitized_artists.join("+")
    };

    // Create the final key
    format!("{}/{}", artists_key, filename_from_string(&album.name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use parking_lot::Mutex;

    fn make_album(name: &str, artists: Vec<&str>) -> crate::data::Album {
        crate::data::Album {
            id: crate::data::Identifier::String("test-album".to_string()),
            name: name.to_string(),
            artists: Arc::new(Mutex::new(artists.into_iter().map(|s| s.to_string()).collect())),
            artists_flat: None,
            release_date: None,
            tracks: Arc::new(Mutex::new(Vec::new())),
            cover_art: None,
            uri: None,
            genres: Vec::new(),
        }
    }

    #[test]
    fn test_safe_truncate_ascii() {
        let input = "Hello, World!";
        assert_eq!(safe_truncate(input, 5), "Hello");
        assert_eq!(safe_truncate(input, 15), "Hello, World!");
        assert_eq!(safe_truncate(input, 0), "");
    }

    #[test]
    fn test_safe_truncate_utf8() {
        let input = "Hello, 世界!";
        assert_eq!(safe_truncate(input, 8), "Hello, 世");
        assert_eq!(safe_truncate(input, 7), "Hello, ");
        assert_eq!(safe_truncate(input, 15), "Hello, 世界!");
    }

    #[test]
    fn test_safe_truncate_empty() {
        let input = "";
        assert_eq!(safe_truncate(input, 5), "");
        assert_eq!(safe_truncate(input, 0), "");
    }

    #[test]
    fn test_safe_truncate_edge_cases() {
        let input = "¥$";  // Multi-byte characters like in the original error
        assert_eq!(safe_truncate(input, 1), "¥");
        assert_eq!(safe_truncate(input, 2), "¥$");
        assert_eq!(safe_truncate(input, 0), "");
    }

    #[test]
    fn regression_key_from_album_uses_unknown_when_artists_sanitize_to_empty() {
        let album = make_album("Album Name", vec!["!!!", "   "]);
        assert_eq!(key_from_album(&album), "unknown/album name");
    }

    #[test]
    fn regression_key_from_album_ignores_empty_sanitized_artists() {
        let album = make_album("Album Name", vec!["!!!", "The Artist"]);
        assert_eq!(key_from_album(&album), "the artist/album name");
    }
}
