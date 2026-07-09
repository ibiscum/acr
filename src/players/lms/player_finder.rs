use log::{debug, info, error};

use crate::players::lms::json_rps::{LmsRpcClient, Player};
use crate::helpers::mac_address::{normalize_mac_address, mac_equal_ignore_case, mac_to_lowercase_string};

fn any_player_matches(players: &[Player], server: &str, mac_addresses: &[String]) -> bool {
    // Check if any of the provided MAC addresses matches a connected player
    for player in players {
        debug!("Server player: {} (MAC: {})", player.name, player.playerid);

        // Normalize the player's MAC address for comparison
        match normalize_mac_address(&player.playerid) {
            Ok(player_mac) => {
                // Convert to lowercase string for consistent comparison
                let player_mac_str = mac_to_lowercase_string(&player_mac);

                // Check against each provided MAC address
                for mac in mac_addresses {
                    if mac_equal_ignore_case(&player_mac_str, mac) {
                        debug!("Found matching player: {} with MAC {} on server {}",
                            player.name, player_mac_str, server);
                        return true;
                    }
                }
            },
            Err(e) => {
                debug!("Failed to normalize MAC address {}: {}", player.playerid, e);
            }
        }
    }

    false
}

/// Check if any of the specified MAC addresses is connected to the given LMS server and port.
pub fn is_player_on_port(server: &str, port: u16, mac_addresses: &[String]) -> bool {
    if mac_addresses.is_empty() {
        debug!("No MAC addresses provided to check");
        return false;
    }

    let client = LmsRpcClient::new(server, port);

    let players = match client.get_players() {
        Ok(players) => players,
        Err(e) => {
            error!("Failed to get players from LMS server {}:{}: {}", server, port, e);
            return false;
        }
    };

    debug!("Found {} players on LMS server {}:{}", players.len(), server, port);
    any_player_matches(&players, server, mac_addresses)
}

/// Check if any of the specified MAC addresses is connected to the given LMS server
///
/// # Arguments
/// * `server` - Server address (hostname or IP)
/// * `mac_addresses` - Vector of MAC addresses to check
///
/// # Returns
/// `true` if any of the MAC addresses is connected to the server, `false` otherwise
pub fn is_player(server: &str, mac_addresses: Vec<String>) -> bool {
    is_player_on_port(server, 9000, &mac_addresses)
}

/// Find the first server from a list that has any of the specified MAC addresses connected on a specific port.
pub fn find_my_server_on_port(servers: &[String], port: u16, mac_addresses: &[String]) -> Option<String> {
    if servers.is_empty() {
        debug!("No servers provided to check");
        return None;
    }

    if mac_addresses.is_empty() {
        debug!("No MAC addresses provided to check");
        return None;
    }

    debug!("Checking {} servers for {} MAC addresses on port {}", servers.len(), mac_addresses.len(), port);

    for server in servers {
        debug!("Checking server: {}", server);
        if is_player_on_port(server, port, mac_addresses) {
            info!("Found matching server: {}", server);
            return Some(server.clone());
        }
    }

    debug!("No matching server found for the provided MAC addresses");
    None
}

/// Find the first server from a list that has any of the specified MAC addresses connected
///
/// # Arguments
/// * `servers` - Vector of server addresses (hostname or IP)
/// * `mac_addresses` - Vector of MAC addresses to check
///
/// # Returns
/// `Some(server)` with the first server that has any of the MAC addresses connected,
/// or `None` if no server has any of the MAC addresses connected
pub fn find_my_server(servers: Vec<String>, mac_addresses: Vec<String>) -> Option<String> {
    find_my_server_on_port(&servers, 9000, &mac_addresses)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_player(playerid: &str, name: &str) -> Player {
        Player {
            playerid: playerid.to_string(),
            name: name.to_string(),
            ip: String::new(),
            model: String::new(),
            is_connected: 1,
            power: 1,
        }
    }

    #[test]
    fn regression_any_player_matches_normalizes_separators_and_case() {
        let players = vec![make_player("AA:BB:CC:DD:EE:FF", "Kitchen")];
        let mac_addresses = vec!["aa-bb-cc-dd-ee-ff".to_string()];

        assert!(any_player_matches(&players, "127.0.0.1", &mac_addresses));
    }

    #[test]
    fn regression_any_player_matches_ignores_invalid_player_mac_entries() {
        let players = vec![
            make_player("invalid-mac", "Broken"),
            make_player("11:22:33:44:55:66", "Living Room"),
        ];
        let mac_addresses = vec!["11.22.33.44.55.66".to_string()];

        assert!(any_player_matches(&players, "127.0.0.1", &mac_addresses));
    }

    #[test]
    fn regression_find_my_server_on_port_rejects_empty_inputs() {
        let servers: Vec<String> = Vec::new();
        let mac_addresses = vec!["aa:bb:cc:dd:ee:ff".to_string()];

        assert_eq!(find_my_server_on_port(&servers, 9001, &mac_addresses), None);
        assert_eq!(find_my_server_on_port(&["127.0.0.1".to_string()], 9001, &[]), None);
    }
}
