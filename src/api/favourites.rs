use rocket::{get, post, delete, routes};
use rocket::serde::json::Json;
use rocket::serde::{Serialize, Deserialize};
use log::{info, error};

use crate::data::song::Song;
use crate::helpers::favourites;

fn normalize_provider_name(provider: &str) -> String {
    match provider {
        "settings_db" => "settingsdb".to_string(),
        _ => provider.to_string(),
    }
}

fn build_validated_song(artist: &str, title: &str) -> Result<Song, ErrorResponse> {
    let normalized_artist = artist.trim();
    let normalized_title = title.trim();

    if normalized_artist.is_empty() {
        return Err(ErrorResponse {
            error: "Invalid song: Artist cannot be empty".to_string(),
        });
    }

    if normalized_title.is_empty() {
        return Err(ErrorResponse {
            error: "Invalid song: Title cannot be empty".to_string(),
        });
    }

    Ok(Song {
        artist: Some(normalized_artist.to_string()),
        title: Some(normalized_title.to_string()),
        ..Default::default()
    })
}

/// Request payload for adding/removing favourites
#[derive(Deserialize)]
pub struct FavouriteRequest {
    artist: String,
    title: String,
}

/// Response for favourite status check
#[derive(Serialize)]
pub struct FavouriteStatusResponse {
    is_favourite: bool,
    providers: Vec<String>,
}

/// Response for favourite operations
#[derive(Serialize)]
pub struct FavouriteOperationResponse {
    success: bool,
    message: String,
    providers: Vec<String>,
    updated_providers: Vec<String>,
}

/// Error response
#[derive(Serialize)]
pub struct ErrorResponse {
    error: String,
}

/// Check if a song is favourite
#[get("/is_favourite?<artist>&<title>")]
pub fn is_favourite(artist: String, title: String) -> Json<Result<FavouriteStatusResponse, ErrorResponse>> {
    info!("Checking favourite status for '{}' by '{}'", title, artist);

    let song = match build_validated_song(&artist, &title) {
        Ok(song) => song,
        Err(e) => return Json(Err(e)),
    };

    match favourites::get_favourite_providers_display_names(&song) {
        Ok((is_fav, provider_display_names)) => {
            Json(Ok(FavouriteStatusResponse {
                is_favourite: is_fav,
                providers: provider_display_names,
            }))
        }
        Err(e) => {
            error!("Error checking favourite status: {}", e);
            Json(Err(ErrorResponse {
                error: e.to_string(),
            }))
        }
    }
}

/// Add a song to favourites
#[post("/add", data = "<request>")]
pub fn add_favourite(request: Json<FavouriteRequest>) -> Json<Result<FavouriteOperationResponse, ErrorResponse>> {
    info!("Adding favourite: '{}' by '{}'", request.title, request.artist);

    let song = match build_validated_song(&request.artist, &request.title) {
        Ok(song) => song,
        Err(e) => return Json(Err(e)),
    };

    let all_providers = favourites::get_enabled_providers()
        .into_iter()
        .map(|provider| normalize_provider_name(&provider))
        .collect::<Vec<String>>();

    match favourites::add_favourite(&song) {
        Ok(updated_providers) => {
            let normalized_updated_providers = updated_providers
                .into_iter()
                .map(|provider| normalize_provider_name(&provider))
                .collect::<Vec<String>>();
            info!("Successfully added favourite: '{}' by '{}' to providers: {:?}", request.title, request.artist, normalized_updated_providers);
            Json(Ok(FavouriteOperationResponse {
                success: true,
                message: format!("Added '{}' by '{}' to favourites", request.title, request.artist),
                providers: all_providers,
                updated_providers: normalized_updated_providers,
            }))
        }
        Err(e) => {
            error!("Error adding favourite: {}", e);
            Json(Err(ErrorResponse {
                error: e.to_string(),
            }))
        }
    }
}

/// Remove a song from favourites
#[delete("/remove", data = "<request>")]
pub fn remove_favourite(request: Json<FavouriteRequest>) -> Json<Result<FavouriteOperationResponse, ErrorResponse>> {
    info!("Removing favourite: '{}' by '{}'", request.title, request.artist);

    let song = match build_validated_song(&request.artist, &request.title) {
        Ok(song) => song,
        Err(e) => return Json(Err(e)),
    };

    let all_providers = favourites::get_enabled_providers()
        .into_iter()
        .map(|provider| normalize_provider_name(&provider))
        .collect::<Vec<String>>();

    match favourites::remove_favourite(&song) {
        Ok(updated_providers) => {
            let normalized_updated_providers = updated_providers
                .into_iter()
                .map(|provider| normalize_provider_name(&provider))
                .collect::<Vec<String>>();
            info!("Successfully removed favourite: '{}' by '{}' from providers: {:?}", request.title, request.artist, normalized_updated_providers);
            Json(Ok(FavouriteOperationResponse {
                success: true,
                message: format!("Removed '{}' by '{}' from favourites", request.title, request.artist),
                providers: all_providers,
                updated_providers: normalized_updated_providers,
            }))
        }
        Err(e) => {
            error!("Error removing favourite: {}", e);
            Json(Err(ErrorResponse {
                error: e.to_string(),
            }))
        }
    }
}

/// Get favourite provider status
#[get("/providers")]
pub fn get_providers() -> Json<serde_json::Value> {
    let (total, enabled) = favourites::get_provider_count();
    let enabled_providers = favourites::get_enabled_providers()
        .into_iter()
        .map(|provider| normalize_provider_name(&provider))
        .collect::<Vec<String>>();
    let provider_details = favourites::get_provider_details()
        .into_iter()
        .map(|provider| {
            let mut provider = provider;
            if let Some(name) = provider.get("name").and_then(|name| name.as_str()) {
                provider["name"] = serde_json::Value::String(normalize_provider_name(name));
            }
            provider
        })
        .collect::<Vec<serde_json::Value>>();

    Json(serde_json::json!({
        "enabled_providers": enabled_providers,
        "total_providers": total,
        "enabled_count": enabled,
        "providers": provider_details
    }))
}

/// Export routes for mounting in the main server
pub fn routes() -> Vec<rocket::Route> {
    routes![is_favourite, add_favourite, remove_favourite, get_providers]
}

#[cfg(test)]
mod tests {
    use super::{add_favourite, build_validated_song, is_favourite, normalize_provider_name, remove_favourite, FavouriteRequest};
    use rocket::serde::json::Json;

    #[test]
    fn build_validated_song_trims_values() {
        let song = match build_validated_song("  Artist  ", "  Title  ") {
            Ok(song) => song,
            Err(err) => panic!("expected valid song, got error: {}", err.error),
        };
        assert_eq!(song.artist.as_deref(), Some("Artist"));
        assert_eq!(song.title.as_deref(), Some("Title"));
    }

    #[test]
    fn build_validated_song_rejects_whitespace_artist() {
        let error = match build_validated_song("   ", "Title") {
            Ok(_) => panic!("expected error for empty artist"),
            Err(error) => error,
        };
        assert!(error.error.contains("Artist cannot be empty"));
    }

    #[test]
    fn build_validated_song_rejects_whitespace_title() {
        let error = match build_validated_song("Artist", "   ") {
            Ok(_) => panic!("expected error for empty title"),
            Err(error) => error,
        };
        assert!(error.error.contains("Title cannot be empty"));
    }

    #[test]
    fn is_favourite_rejects_whitespace_inputs() {
        let response = is_favourite("   ".to_string(), "   ".to_string());
        assert!(response.0.is_err());
    }

    #[test]
    fn add_favourite_rejects_whitespace_inputs() {
        let response = add_favourite(Json(FavouriteRequest {
            artist: "   ".to_string(),
            title: "Title".to_string(),
        }));
        assert!(response.0.is_err());
    }

    #[test]
    fn remove_favourite_rejects_whitespace_inputs() {
        let response = remove_favourite(Json(FavouriteRequest {
            artist: "Artist".to_string(),
            title: "   ".to_string(),
        }));
        assert!(response.0.is_err());
    }

    #[test]
    fn normalize_provider_name_maps_settings_db_legacy_identifier() {
        assert_eq!(normalize_provider_name("settings_db"), "settingsdb");
        assert_eq!(normalize_provider_name("lastfm"), "lastfm");
    }
}
