#![cfg(unix)]

use std::env;
use audiocontrol::helpers::mpris::{find_mpris_players, BusType, MprisPlayer};

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_help();
        return;
    }

    println!("AudioControl MPRIS Player Scanner");
    println!("==================================");

    // Find all MPRIS players on both session and system buses
    let mut all_players = Vec::new();

    // Try session bus first (most common)
    println!("Scanning session bus for MPRIS players...");
    match find_mpris_players(BusType::Session) {
        Ok(players) => {
            println!("Found {} MPRIS player(s) on session bus", players.len());
            all_players.extend(players);
        }
        Err(e) => {
            println!("Warning: Failed to scan session bus: {}", e);
        }
    }

    // Try system bus (for system services like ShairportSync)
    println!("Scanning system bus for MPRIS players...");
    match find_mpris_players(BusType::System) {
        Ok(players) => {
            println!("Found {} MPRIS player(s) on system bus", players.len());
            all_players.extend(players);
        }
        Err(e) => {
            println!("Warning: Failed to scan system bus: {}", e);
        }
    }

    if all_players.is_empty() {
        println!("\nNo MPRIS players found on either session or system bus.");
        println!("\nTip: Make sure media players that support MPRIS are running.");
        println!("Common MPRIS-enabled players include: VLC, Spotify, Rhythmbox, Audacious, etc.");
        return;
    }

    println!("\nTotal: Found {} MPRIS player(s):\n", all_players.len());

    for (i, player) in all_players.iter().enumerate() {
        print_player_info(i + 1, player);
    }

    println!("\nSample Configuration:");
    println!("====================");
    if let Some(first_player) = all_players.first() {
        print_sample_config(first_player);
    }
}

fn print_help() {
    println!("AudioControl MPRIS Player Scanner");
    println!();
    println!("USAGE:");
    println!("    audiocontrol_list_mpris_players [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help    Print this help message");
    println!();
    println!("DESCRIPTION:");
    println!("    Scans the system D-Bus for MPRIS-compatible media players and displays");
    println!("    their capabilities and bus names. Use this tool to identify players");
    println!("    that can be controlled via the MPRIS interface.");
    println!();
    println!("EXAMPLES:");
    println!("    audiocontrol_list_mpris_players");
    println!("        List all available MPRIS players");
}

fn print_player_info(index: usize, player: &MprisPlayer) {
    println!("{}. Player Information:", index);
    println!("   Bus Name: {}", player.bus_name);
    println!("   Bus Type: {} bus", player.bus_type);

    // Extract player name from bus name
    let player_name = extract_player_name(&player.bus_name);
    println!("   Player Name: {}", player_name);

    // Print identity
    match &player.identity {
        Some(identity) => println!("   Identity: {}", identity),
        None => println!("   Identity: <not available>"),
    }

    // Print desktop entry
    match &player.desktop_entry {
        Some(entry) => println!("   Desktop Entry: {}", entry),
        None => println!("   Desktop Entry: <not available>"),
    }

    // Print capabilities
    println!("   Capabilities:");

    match player.can_control {
        Some(can_control) => println!("     - Can Control: {}", can_control),
        None => println!("     - Can Control: <not available>"),
    }

    match player.can_play {
        Some(can_play) => println!("     - Can Play: {}", can_play),
        None => println!("     - Can Play: <not available>"),
    }

    match player.can_pause {
        Some(can_pause) => println!("     - Can Pause: {}", can_pause),
        None => println!("     - Can Pause: <not available>"),
    }

    match player.can_seek {
        Some(can_seek) => println!("     - Can Seek: {}", can_seek),
        None => println!("     - Can Seek: <not available>"),
    }

    match player.can_go_next {
        Some(can_go_next) => println!("     - Can Go Next: {}", can_go_next),
        None => println!("     - Can Go Next: <not available>"),
    }

    match player.can_go_previous {
        Some(can_go_previous) => println!("     - Can Go Previous: {}", can_go_previous),
        None => println!("     - Can Go Previous: <not available>"),
    }

    // Print current status
    match &player.playback_status {
        Some(status) => println!("   Current Status: {}", status),
        None => println!("   Current Status: <not available>"),
    }

    // Print current track info
    for line in format_current_track_lines(player.current_track.as_deref(), player.current_artist.as_deref()) {
        println!("{}", line);
    }

    if player.bus_type == BusType::System {
        println!("   Note: This player is on the system bus. Full MPRIS control");
        println!("         may require special configuration or elevated privileges.");
    }

    println!();
}

fn print_sample_config(player: &MprisPlayer) {
    println!("{{");
    println!("  \"mpris\": {{");
    println!("    \"enable\": true,");
    println!("    \"bus_name\": \"{}\",", player.bus_name);
    println!("    \"bus_type\": \"{}\"", config_bus_type_value(&player.bus_type));
    println!("  }}");
    println!("}}");
    println!();
    println!("Add this configuration to your audiocontrol.json players array to");
    println!("enable control of this MPRIS player through AudioControl.");

    if player.bus_type == BusType::System {
        println!();
        println!("Note: System bus MPRIS players may require special configuration");
        println!("      and may not be fully supported by all MPRIS libraries.");
    }
}

fn extract_player_name(bus_name: &str) -> &str {
    bus_name
        .strip_prefix("org.mpris.MediaPlayer2.")
        .unwrap_or("Unknown")
}

fn config_bus_type_value(bus_type: &BusType) -> &'static str {
    if *bus_type == BusType::System {
        "system"
    } else {
        "session"
    }
}

fn format_current_track_lines(current_track: Option<&str>, current_artist: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    match (current_track, current_artist) {
        (Some(track), Some(artist)) => {
            lines.push(format!("   Current Track: {}", track));
            lines.push(format!("   Current Artist: {}", artist));
        }
        (Some(track), None) => lines.push(format!("   Current Track: {}", track)),
        (None, Some(artist)) => {
            lines.push("   Current Track: <no track loaded>".to_string());
            lines.push(format!("   Current Artist: {}", artist));
        }
        (None, None) => lines.push("   Current Track: <no track loaded>".to_string()),
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regression_extract_player_name_handles_expected_and_unexpected_bus_names() {
        assert_eq!(extract_player_name("org.mpris.MediaPlayer2.spotify"), "spotify");
        assert_eq!(extract_player_name("org.example.CustomPlayer"), "Unknown");
    }

    #[test]
    fn regression_config_bus_type_value_matches_config_literals() {
        assert_eq!(config_bus_type_value(&BusType::Session), "session");
        assert_eq!(config_bus_type_value(&BusType::System), "system");
    }

    #[test]
    fn regression_format_current_track_lines_keeps_artist_when_track_missing() {
        let lines = format_current_track_lines(None, Some("Test Artist"));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "   Current Track: <no track loaded>");
        assert_eq!(lines[1], "   Current Artist: Test Artist");
    }

    #[test]
    fn regression_format_current_track_lines_handles_all_combinations() {
        let both = format_current_track_lines(Some("Track"), Some("Artist"));
        assert_eq!(both, vec!["   Current Track: Track", "   Current Artist: Artist"]);

        let only_track = format_current_track_lines(Some("Track"), None);
        assert_eq!(only_track, vec!["   Current Track: Track"]);

        let none = format_current_track_lines(None, None);
        assert_eq!(none, vec!["   Current Track: <no track loaded>"]);
    }
}


