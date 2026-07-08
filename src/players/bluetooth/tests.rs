use crate::players::BluetoothPlayerController;
use crate::players::PlayerController;
use crate::data::PlayerCommand;

#[test]
fn test_bluetooth_controller_creation() {
    let controller = BluetoothPlayerController::new_with_address(Some("80:B9:89:1E:B5:6F".to_string()));
    
    assert_eq!(controller.get_player_name(), "bluetooth");
    assert_eq!(controller.get_player_id(), "bluetooth:80:B9:89:1E:B5:6F");
    
    let aliases = controller.get_aliases();
    assert!(aliases.contains(&"bluetooth".to_string()));
    assert!(aliases.contains(&"bluez".to_string()));
    assert!(aliases.contains(&"bt".to_string()));
    
    // Test that it has basic capabilities
    let caps = controller.get_capabilities();
    assert!(caps.has_capability(crate::data::PlayerCapability::Play));
    assert!(caps.has_capability(crate::data::PlayerCapability::Pause));
    assert!(caps.has_capability(crate::data::PlayerCapability::Next));
    assert!(caps.has_capability(crate::data::PlayerCapability::Previous));
}

#[test]
fn test_bluetooth_controller_from_factory() {
    use crate::players::player_factory::create_player_from_json_str;
    
    let config = r#"
    {
        "bluetooth": {
            "device_address": "80:B9:89:1E:B5:6F"
        }
    }
    "#;
    
    let result = create_player_from_json_str(config);
    assert!(result.is_ok());
    
    let controller = result.unwrap();
    assert_eq!(controller.get_player_name(), "bluetooth");
}

#[test]
fn test_bluetooth_controller_auto_discover_id() {
    let controller = BluetoothPlayerController::new_with_address(None);
    assert_eq!(controller.get_player_id(), "bluetooth:auto-discover");
}

#[test]
fn test_bluetooth_controller_empty_address_edge_case() {
    let controller = BluetoothPlayerController::new_with_address(Some(String::new()));
    assert_eq!(controller.get_player_id(), "bluetooth:");
}

#[test]
fn test_bluetooth_unsupported_commands_return_false() {
    let controller = BluetoothPlayerController::new_with_address(Some("80:B9:89:1E:B5:6F".to_string()));

    assert!(!controller.send_command(PlayerCommand::Seek(42.0)));
    assert!(!controller.send_command(PlayerCommand::SetLoopMode(crate::data::LoopMode::Track)));
    assert!(!controller.send_command(PlayerCommand::SetRandom(true)));
}

#[test]
fn test_bluetooth_stop_is_idempotent() {
    let controller = BluetoothPlayerController::new_with_address(Some("80:B9:89:1E:B5:6F".to_string()));

    assert!(controller.stop());
    assert!(controller.stop());
}

#[test]
fn test_bluetooth_factory_missing_device_address_defaults_to_auto_discover() {
    use crate::players::player_factory::create_player_from_json_str;

    let config = r#"
    {
        "bluetooth": {}
    }
    "#;

    let result = create_player_from_json_str(config);
    assert!(result.is_ok());

    let controller = result.unwrap();
    assert_eq!(controller.get_player_name(), "bluetooth");
    assert_eq!(controller.get_player_id(), "bluetooth:auto-discover");
}

#[test]
fn test_bluetooth_factory_invalid_device_address_type_defaults_to_auto_discover() {
    use crate::players::player_factory::create_player_from_json_str;

    let config = r#"
    {
        "bluetooth": {
            "device_address": 12345
        }
    }
    "#;

    let result = create_player_from_json_str(config);
    assert!(result.is_ok());

    let controller = result.unwrap();
    assert_eq!(controller.get_player_name(), "bluetooth");
    assert_eq!(controller.get_player_id(), "bluetooth:auto-discover");
}