use audiocontrol::data::{LoopMode, PlaybackState};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::error::Error;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Name of the player
    player_name: String,

    #[clap(long, default_value = "http://localhost:1080/api")]
    baseurl: String,

    /// Enable verbose output with JSON payloads
    #[clap(long, short = 'v', help = "Enable verbose output")]
    verbose: bool,

    /// Suppress all output
    #[clap(long, short = 'q', help = "Quiet mode - suppress all output")]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Update song information and optionally playback state
    ///
    /// Example: audiocontrol_send_update player1 song --title "Song Title" --artist "Artist Name" --album "Album Name"
    Song {
        #[clap(long, help = "Artist name")]
        artist: Option<String>,

        #[clap(long, help = "Song title")]
        title: Option<String>,

        #[clap(long, help = "Album name")]
        album: Option<String>,

        #[clap(long, help = "Song duration in seconds")]
        length: Option<f64>, // Duration in seconds

        #[clap(long, help = "Stream URI or track identifier")]
        uri: Option<String>, // Stream URI

        /// Playback state to set with the song (default: playing)
        #[clap(
            long,
            default_value = "playing",
            help = "Playback state (playing, paused, stopped, etc.)"
        )]
        state: PlaybackState,
    },

    /// Update playback state
    ///
    /// Example: audiocontrol_send_update player1 state playing
    State {
        /// Playback state (playing, paused, stopped, etc.)
        #[clap(help = "Playback state (playing, paused, stopped, killed, disconnected, unknown)")]
        state: PlaybackState,
    },

    /// Update shuffle setting
    ///
    /// Example: audiocontrol_send_update player1 shuffle true
    Shuffle {
        /// Enable or disable shuffle (true/false)
        #[clap(help = "Enable shuffle (true) or disable shuffle (false)")]
        enabled: String,
    },

    /// Update loop mode
    ///
    /// Example: audiocontrol_send_update player1 loop playlist
    Loop {
        /// Loop mode (no, song, playlist)
        #[clap(
            help = "Loop mode: no (no looping), song (repeat current track), playlist (repeat playlist)"
        )]
        mode: LoopMode,
    },

    /// Update playback position
    ///
    /// Example: audiocontrol_send_update player1 position 45.5
    Position {
        /// Current playback position in seconds
        #[clap(help = "Playback position in seconds (e.g., 45.5 for 45.5 seconds)")]
        position: f64,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let client = ureq::agent();

    match args.command {
        Commands::Song {
            artist,
            title,
            album,
            length,
            uri,
            state,
        } => {
            // Send song change event first
            let mut song = json!({});

            if let Some(artist) = artist {
                song["artist"] = json!(artist);
            }
            if let Some(title) = title {
                song["title"] = json!(title);
            }
            if let Some(album) = album {
                song["album"] = json!(album);
            }
            if let Some(length) = length {
                ensure_non_negative("length", length)?;
                song["duration"] = json!(length);
            }
            if let Some(uri) = uri {
                song["uri"] = json!(uri);
            }

            let song_event = json!({
                "type": "song_changed",
                "song": song
            });

            send_event(
                &client,
                &args.baseurl,
                &args.player_name,
                &song_event,
                args.verbose,
                args.quiet,
            )?;

            // Send state change event (default to Playing)
            let state_str = playback_state_to_str(state);

            let state_event = json!({
                "type": "state_changed",
                "state": state_str
            });

            send_event(
                &client,
                &args.baseurl,
                &args.player_name,
                &state_event,
                args.verbose,
                args.quiet,
            )?;
        }

        Commands::State { state } => {
            let state_str = playback_state_to_str(state);

            let event = json!({
                "type": "state_changed",
                "state": state_str
            });

            send_event(
                &client,
                &args.baseurl,
                &args.player_name,
                &event,
                args.verbose,
                args.quiet,
            )?;
        }

        Commands::Shuffle { enabled } => {
            let enabled_bool = parse_bool_like_arg(&enabled)?;
            let event = json!({
                "type": "shuffle_changed",
                "enabled": enabled_bool
            });

            send_event(
                &client,
                &args.baseurl,
                &args.player_name,
                &event,
                args.verbose,
                args.quiet,
            )?;
        }

        Commands::Loop { mode } => {
            let mode_str = match mode {
                LoopMode::Track => "track",
                LoopMode::Playlist => "playlist",
                LoopMode::None => "none",
            };

            let event = json!({
                "type": "loop_mode_changed",
                "loop_mode": mode_str
            });

            send_event(
                &client,
                &args.baseurl,
                &args.player_name,
                &event,
                args.verbose,
                args.quiet,
            )?;
        }

        Commands::Position { position } => {
            ensure_non_negative("position", position)?;
            let event = json!({
                "type": "position_changed",
                "position": position
            });

            send_event(
                &client,
                &args.baseurl,
                &args.player_name,
                &event,
                args.verbose,
                args.quiet,
            )?;
        }
    }

    Ok(())
}

fn playback_state_to_str(state: PlaybackState) -> &'static str {
    match state {
        PlaybackState::Playing => "playing",
        PlaybackState::Paused => "paused",
        PlaybackState::Stopped => "stopped",
        PlaybackState::Killed => "killed",
        PlaybackState::Disconnected => "disconnected",
        PlaybackState::Unknown => "unknown",
    }
}

fn parse_bool_like_arg(value: &str) -> Result<bool, Box<dyn Error>> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!(
            "Invalid boolean value '{}'. Use one of: true/false, yes/no, on/off, 1/0",
            value
        )
        .into()),
    }
}

fn ensure_non_negative(field_name: &str, value: f64) -> Result<(), Box<dyn Error>> {
    if value.is_sign_negative() {
        return Err(format!("{} must be >= 0", field_name).into());
    }
    Ok(())
}

fn send_event(
    client: &ureq::Agent,
    baseurl: &str,
    player_name: &str,
    event: &Value,
    verbose: bool,
    quiet: bool,
) -> Result<(), Box<dyn Error>> {
    let url = format!("{}/player/{}/update", baseurl, player_name);

    if !quiet {
        println!("Sending event to: {}", url);
        if verbose {
            println!("Payload: {}", serde_json::to_string_pretty(&event)?);
        }
    }

    let response = client
        .post(&url)
        .set("Content-Type", "application/json")
        .send_string(&serde_json::to_string(&event)?);

    match response {
        Ok(resp) => {
            if resp.status() >= 200 && resp.status() < 300 {
                if !quiet {
                    println!("Event sent successfully. Status: {}", resp.status());
                }
            } else {
                let status = resp.status();
                let response_body = resp
                    .into_string()
                    .unwrap_or_else(|_| "Failed to read response body".to_string());
                if !quiet {
                    eprintln!("Failed to send event. Status: {}", status);
                    eprintln!("Response: {}", response_body);
                }
                return Err(format!("HTTP error: {}", status).into());
            }
        }
        Err(e) => {
            if !quiet {
                eprintln!("Error sending request: {}", e);
            }
            return Err(Box::new(e));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_parse_bool_like_arg_rejects_invalid_input() {
        assert_eq!(parse_bool_like_arg("true").unwrap(), true);
        assert_eq!(parse_bool_like_arg("NO").unwrap(), false);
        assert!(parse_bool_like_arg("invalid").is_err());
    }

    #[test]
    fn integration_playback_state_to_str_maps_all_variants() {
        assert_eq!(playback_state_to_str(PlaybackState::Playing), "playing");
        assert_eq!(playback_state_to_str(PlaybackState::Paused), "paused");
        assert_eq!(playback_state_to_str(PlaybackState::Stopped), "stopped");
        assert_eq!(playback_state_to_str(PlaybackState::Killed), "killed");
        assert_eq!(playback_state_to_str(PlaybackState::Disconnected), "disconnected");
        assert_eq!(playback_state_to_str(PlaybackState::Unknown), "unknown");
    }

    #[test]
    fn regression_ensure_non_negative_rejects_invalid_values() {
        assert!(ensure_non_negative("position", 0.0).is_ok());
        assert!(ensure_non_negative("length", 42.5).is_ok());
        assert!(ensure_non_negative("position", -0.1).is_err());
    }
}
