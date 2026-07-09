use audiocontrol::data::PlaybackState;
use clap::Parser;
use serde_json::{json, Value};
use std::env;
use std::error::Error;

#[derive(Parser, Debug)]
#[clap(author, version, about = "Send librespot events to audiocontrol API", long_about = None)]
struct Args {
    /// Base URL for the audiocontrol API
    #[clap(long, default_value = "http://127.0.0.1:1080/api")]
    baseurl: String,

    /// Player name to use in API calls
    #[clap(long, default_value = "librespot")]
    player_name: String,

    /// Enable verbose output with full request details
    #[clap(long, short = 'v', help = "Enable verbose output")]
    verbose: bool,

    /// Suppress all output
    #[clap(long, short = 'q', help = "Quiet mode - suppress all output")]
    quiet: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let client = ureq::agent();

    // Get the player event type from environment
    let player_event = env::var("PLAYER_EVENT").unwrap_or_else(|_| "unknown".to_string());
    let normalized_event = normalize_player_event(&player_event);

    if !args.quiet {
        println!("Received event: {}", player_event);
    }

    match normalized_event.as_str() {
        "track_changed" => {
            handle_track_changed(&client, &args)?;
        }
        "playing" => {
            handle_playback_state(&client, &args, PlaybackState::Playing)?;
        }
        "paused" => {
            handle_playback_state(&client, &args, PlaybackState::Paused)?;
        }
        "stopped" => {
            handle_playback_state(&client, &args, PlaybackState::Stopped)?;
        }
        "seeked" => {
            handle_position_changed(&client, &args)?;
        }
        "shuffle_changed" => {
            handle_shuffle_changed(&client, &args)?;
        }
        "repeat_changed" => {
            handle_repeat_changed(&client, &args)?;
        }
        "preloading" => {
            handle_preloading(&client, &args)?;
        }
        _ => {
            if !args.quiet {
                eprintln!("Unknown or unsupported event type: {}", player_event);
            }
        }
    }

    Ok(())
}

fn handle_track_changed(client: &ureq::Agent, args: &Args) -> Result<(), Box<dyn Error>> {
    let mut song = json!({});

    // Parse track information from environment variables
    if let Ok(title) = env::var("NAME") {
        song["title"] = json!(title);
    }

    if let Ok(artist) = env::var("ARTISTS") {
        song["artist"] = json!(artist);
    }

    if let Ok(album) = env::var("ALBUM") {
        song["album"] = json!(album);
    }

    if let Ok(duration_ms) = env::var("DURATION_MS") {
        if let Some(duration_seconds) = parse_millis_to_seconds(&duration_ms) {
            // Convert to seconds and ensure it's set
            song["duration"] = json!(duration_seconds);

            // Log duration for debugging
            if !args.quiet {
                println!("Setting song duration: {} ms -> {} seconds", duration_ms, duration_seconds);
            }
        }
    }

    if let Ok(uri) = env::var("URI") {
        song["uri"] = json!(uri);

        // Also set stream_url for compatibility
        song["stream_url"] = json!(uri);
    }

    // Add additional metadata if available
    if let Ok(track_number) = env::var("NUMBER") {
        song["track_number"] = json!(track_number);
    }

    if let Ok(disc_number) = env::var("DISC_NUMBER") {
        song["disc_number"] = json!(disc_number);
    }

    if let Ok(covers) = env::var("COVERS") {
        // Pick the first non-empty line to avoid sending empty cover URLs.
        if let Some(cover_url) = first_non_empty_line(&covers) {
            // Set both field names to ensure compatibility
            song["cover_url"] = json!(cover_url);
            song["cover_art_url"] = json!(cover_url);

            // Log cover URL for debugging
            if !args.quiet {
                println!("Setting cover URL: {}", cover_url);
            }
        }
    }

    let event = json!({
        "type": "song_changed",
        "song": song
    });

    send_event(
        client,
        &args.baseurl,
        &args.player_name,
        &event,
        args.verbose,
        args.quiet,
    )?;

    // Also send playing state since track_changed usually means we're playing
    let state_event = json!({
        "type": "state_changed",
        "state": "playing"
    });

    send_event(
        client,
        &args.baseurl,
        &args.player_name,
        &state_event,
        args.verbose,
        args.quiet,
    )?;

    Ok(())
}

fn handle_playback_state(
    client: &ureq::Agent,
    args: &Args,
    state: PlaybackState,
) -> Result<(), Box<dyn Error>> {
    let state_str = match state {
        PlaybackState::Playing => "playing",
        PlaybackState::Paused => "paused",
        PlaybackState::Stopped => "stopped",
        PlaybackState::Killed => "killed",
        PlaybackState::Disconnected => "disconnected",
        PlaybackState::Unknown => "unknown",
    };

    let mut event = json!({
        "type": "state_changed",
        "state": state_str
    });

    // Add position if available
    if let Ok(position_ms) = env::var("POSITION_MS") {
        if let Some(position) = parse_millis_to_seconds(&position_ms) {
            event["position"] = json!(position); // Convert to seconds
        }
    }

    send_event(
        client,
        &args.baseurl,
        &args.player_name,
        &event,
        args.verbose,
        args.quiet,
    )?;

    Ok(())
}

fn handle_shuffle_changed(client: &ureq::Agent, args: &Args) -> Result<(), Box<dyn Error>> {
    let shuffle_enabled = env::var("SHUFFLE")
        .ok()
        .and_then(|v| parse_bool_like(&v))
        .unwrap_or(false);

    let event = json!({
        "type": "shuffle_changed",
        "enabled": shuffle_enabled
    });

    send_event(
        client,
        &args.baseurl,
        &args.player_name,
        &event,
        args.verbose,
        args.quiet,
    )?;

    Ok(())
}

fn handle_repeat_changed(client: &ureq::Agent, args: &Args) -> Result<(), Box<dyn Error>> {
    let repeat_enabled = env::var("REPEAT")
        .ok()
        .and_then(|v| parse_bool_like(&v))
        .unwrap_or(false);

    let repeat_track = env::var("REPEAT_TRACK")
        .ok()
        .and_then(|v| parse_bool_like(&v))
        .unwrap_or(false);

    let loop_mode = if !repeat_enabled {
        "none"
    } else if repeat_track {
        "track"
    } else {
        "playlist"
    };

    let event = json!({
        "type": "loop_mode_changed",
        "loop_mode": loop_mode
    });

    send_event(
        client,
        &args.baseurl,
        &args.player_name,
        &event,
        args.verbose,
        args.quiet,
    )?;

    Ok(())
}

fn handle_position_changed(client: &ureq::Agent, args: &Args) -> Result<(), Box<dyn Error>> {
    let mut event = json!({
        "type": "position_changed"
    });

    // Add position from environment variable
    if let Ok(position_ms) = env::var("POSITION_MS") {
        if let Some(position) = parse_millis_to_seconds(&position_ms) {
            event["position"] = json!(position); // Convert to seconds
        }
    }

    send_event(
        client,
        &args.baseurl,
        &args.player_name,
        &event,
        args.verbose,
        args.quiet,
    )?;

    Ok(())
}

fn parse_millis_to_seconds(value: &str) -> Option<f64> {
    let millis = value.trim().parse::<u64>().ok()?;
    Some(millis as f64 / 1000.0)
}

fn normalize_player_event(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn parse_bool_like(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn first_non_empty_line(value: &str) -> Option<&str> {
    value.lines().map(str::trim).find(|line| !line.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_parse_millis_to_seconds_handles_invalid_input() {
        assert_eq!(parse_millis_to_seconds("1000"), Some(1.0));
        assert_eq!(parse_millis_to_seconds(" 2500 "), Some(2.5));
        assert_eq!(parse_millis_to_seconds("-10"), None);
        assert_eq!(parse_millis_to_seconds("abc"), None);
        assert_eq!(parse_millis_to_seconds(""), None);
    }

    #[test]
    fn regression_parse_bool_like_accepts_common_variants() {
        assert_eq!(parse_bool_like("true"), Some(true));
        assert_eq!(parse_bool_like("YES"), Some(true));
        assert_eq!(parse_bool_like("1"), Some(true));
        assert_eq!(parse_bool_like("off"), Some(false));
        assert_eq!(parse_bool_like("0"), Some(false));
        assert_eq!(parse_bool_like("maybe"), None);
    }

    #[test]
    fn regression_normalize_player_event_trims_and_lowercases() {
        assert_eq!(normalize_player_event(" playing "), "playing");
        assert_eq!(normalize_player_event("TRACK_CHANGED"), "track_changed");
        assert_eq!(normalize_player_event("\tSeeked\n"), "seeked");
    }

    fn integration_first_non_empty_line_skips_blank_lines() {
        assert_eq!(first_non_empty_line("\n\nhttps://x\nhttps://y"), Some("https://x"));
        assert_eq!(first_non_empty_line("   \n  "), None);
        assert_eq!(first_non_empty_line("single"), Some("single"));
    }
}

fn handle_preloading(client: &ureq::Agent, args: &Args) -> Result<(), Box<dyn Error>> {
    // For preloading, we just need to send a simple ping event to update the "last seen" timestamp
    let event = json!({
        "type": "ping"
    });

    send_event(
        client,
        &args.baseurl,
        &args.player_name,
        &event,
        args.verbose,
        args.quiet,
    )?;

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

    if verbose && !quiet {
        println!("Sending event to: {}", url);
        println!("Payload: {}", serde_json::to_string_pretty(&event)?);
    }

    let response = client
        .post(&url)
        .set("Content-Type", "application/json")
        .send_string(&serde_json::to_string(&event)?);

    match response {
        Ok(resp) => {
            if resp.status() >= 200 && resp.status() < 300 {
                if verbose && !quiet {
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
