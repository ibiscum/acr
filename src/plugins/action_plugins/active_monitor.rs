use std::sync::{Arc, Weak};
use std::any::Any;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use crate::data::{PlayerCapability, PlayerCommand, PlayerEvent, PlaybackState};
use crate::plugins::plugin::Plugin;
use crate::plugins::action_plugin::{ActionPlugin, BaseActionPlugin};
use crate::audiocontrol::AudioController;
use log::{debug, info, warn, trace};
use delegate::delegate;

/// A plugin that monitors player state changes and sets the active player
/// to any player that enters the Playing state.
pub struct ActiveMonitor {
    /// Base implementation for common functionality
    base: BaseActionPlugin,

    /// Timestamp of the last successful active-player switch.
    last_switch_at: Arc<Mutex<Option<Instant>>>,

    /// Debounce window to avoid rapid active-player flapping.
    switch_debounce: Duration,
}

impl Default for ActiveMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl ActiveMonitor {
    /// Create a new ActiveMonitor plugin
    pub fn new() -> Self {
        Self {
            base: BaseActionPlugin::new("ActiveMonitor"),
            last_switch_at: Arc::new(Mutex::new(None)),
            switch_debounce: Duration::from_millis(500),
        }
    }

    /// Try to find a player controller by name and ID and make it active
    fn set_active_player(&self, player_name: &str, player_id: &str) -> bool {
        if let Some(controller) = self.base.get_controller() {
            // First check if the given player is already active
            if let Some(active_controller) = controller.get_active_controller() {
                let active_player = active_controller.read();
                info!(
                    "ActiveMonitor: Switch request from {}:{} while active is {}:{}",
                    player_name,
                    player_id,
                    active_player.get_player_name(),
                    active_player.get_player_id()
                );
                if active_player.get_player_name() == player_name &&
                   active_player.get_player_id() == player_id {
                    debug!("ActiveMonitor: Player {}:{} is already active, no change needed",
                           player_name, player_id);
                    return true;
                }
            }

            // Find the controller with matching name and ID
            let controllers = controller.list_controllers();
            let mut target_index = None;

            // First find the matching player and store its index
            for (idx, player_controller) in controllers.iter().enumerate() {
                let player = player_controller.read();
                if player.get_player_name() == player_name && player.get_player_id() == player_id {
                    target_index = Some(idx);
                    break;
                }
            }

            // Set the active controller after all locks have been released.
            // Do not debounce source changes here: some players emit a single
            // Playing event, and dropping it can make switch-back unreliable.
            if let Some(idx) = target_index {
                let now = Instant::now();
                {
                    let last_switch = self.last_switch_at.lock();
                    if let Some(last) = *last_switch {
                        if now.duration_since(last) < self.switch_debounce {
                            debug!(
                                "ActiveMonitor: Debounced active switch to {}:{} (within {:?})",
                                player_name,
                                player_id,
                                self.switch_debounce
                            );
                            return false;
                        }
                    }
                }

                info!("ActiveMonitor: Setting player {}:{} as active", player_name, player_id);
                if controller.set_active_controller(idx) {
                    *self.last_switch_at.lock() = Some(now);
                    info!("ActiveMonitor: Successfully set active player to {}:{}",
                          player_name, player_id);
                    true
                } else {
                    warn!("ActiveMonitor: Failed to set active player");
                    false
                }
            } else {
                warn!("ActiveMonitor: Could not find player {}:{} to set active", player_name, player_id);
                false
            }
        } else {
            warn!("ActiveMonitor: No valid AudioController reference available");
            false
        }
    }

    /// Ensure only one player remains in Playing state by pausing/stopping
    /// all other players that are currently playing.
    fn enforce_single_playback(&self, active_player_name: &str, active_player_id: &str) {
        if let Some(controller) = self.base.get_controller() {
            let controllers = controller.list_controllers();
            let mut deactivation_targets = Vec::new();

            // Gather targets first, then send commands after read-locks are released.
            for ctrl_lock in controllers {
                let ctrl = ctrl_lock.read();
                let player_name = ctrl.get_player_name();
                let player_id = ctrl.get_player_id();

                // Keep the source player untouched.
                if player_name == active_player_name && player_id == active_player_id {
                    continue;
                }

                if ctrl.get_playback_state() != PlaybackState::Playing {
                    continue;
                }

                let caps = ctrl.get_capabilities();
                let command = if caps.has_capability(PlayerCapability::Pause) {
                    Some(PlayerCommand::Pause)
                } else if caps.has_capability(PlayerCapability::Stop) {
                    Some(PlayerCommand::Stop)
                } else {
                    None
                };

                if let Some(command) = command {
                    deactivation_targets.push((ctrl_lock.clone(), player_name, player_id, command));
                } else {
                    warn!(
                        "ActiveMonitor: Player {}:{} is Playing but supports neither Pause nor Stop",
                        player_name, player_id
                    );
                }
            }

            if deactivation_targets.is_empty() {
                info!(
                    "ActiveMonitor: Single-playback check for {}:{} found no other playing players to deactivate",
                    active_player_name,
                    active_player_id
                );
            } else {
                info!(
                    "ActiveMonitor: Single-playback check for {}:{} will deactivate {} player(s)",
                    active_player_name,
                    active_player_id,
                    deactivation_targets.len()
                );
            }

            for (ctrl_lock, player_name, player_id, command) in deactivation_targets {
                let cmd_name = command.to_string();
                let success = ctrl_lock.read().send_command(command);
                if success {
                    info!(
                        "ActiveMonitor: Sent {} to {}:{} to enforce single-playback",
                        cmd_name, player_name, player_id
                    );
                } else {
                    warn!(
                        "ActiveMonitor: Failed to send {} to {}:{} while enforcing single-playback",
                        cmd_name, player_name, player_id
                    );
                }
            }
        } else {
            warn!("ActiveMonitor: No valid AudioController reference available");
        }
    }

    /// Handle events coming from the event bus
    fn handle_event_bus_events(&self, event: PlayerEvent) {
        trace!("Received event from event bus");

        // We only care about state changed events
        if let PlayerEvent::StateChanged { source, state } = event {
            // If a player state changes to Playing, make it the active player
            if state == PlaybackState::Playing {
                info!("ActiveMonitor: Detected state transition to Playing from {}:{}",
                       source.player_name(), source.player_id());
                if self.set_active_player(source.player_name(), source.player_id()) {
                    self.enforce_single_playback(source.player_name(), source.player_id());
                } else {
                    debug!(
                        "ActiveMonitor: Skipping single-playback enforcement because source {}:{} is not active/resolvable",
                        source.player_name(),
                        source.player_id()
                    );
                }
            }
        }
    }
}

impl Plugin for ActiveMonitor {
    delegate! {
        to self.base {
            fn name(&self) -> &str;
            fn version(&self) -> &str;
        }
    }

    fn init(&mut self) -> bool {
        log::info!("ActiveMonitor initializing");
        self.base.init()
    }

    fn shutdown(&mut self) -> bool {
        log::info!("ActiveMonitor shutting down");
        self.base.shutdown()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ActionPlugin for ActiveMonitor {
    fn initialize(&mut self, controller: Weak<AudioController>) {
        self.base.set_controller(controller);

        // Subscribe to event bus in the initialize method
        log::debug!("ActiveMonitor initializing and subscribing to event bus");
        let self_clone = self.clone();
        self.base.subscribe_to_event_bus(move |event| {
            self_clone.handle_event(event);
        });
    }

    fn handle_event(&self, event: PlayerEvent) {
        // Handle events using the existing method
        self.handle_event_bus_events(event);
    }
}

// Clone implementation for ActiveMonitor to allow for passing to thread
impl Clone for ActiveMonitor {
    fn clone(&self) -> Self {
        let mut new_base = BaseActionPlugin::new(self.base.name());

        // Get the controller reference from the original object
        if let Some(controller) = self.base.get_controller() {
            // The controller is already an Arc, we need to downgrade it to a Weak
            let controller_weak = Arc::downgrade(&controller);
            new_base.set_controller(controller_weak);
        }

        Self {
            base: new_base,
            last_switch_at: Arc::clone(&self.last_switch_at),
            switch_debounce: self.switch_debounce,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{LoopMode, PlayerCapabilitySet, PlayerSource, Song, Track};
    use crate::players::PlayerController;
    use parking_lot::Mutex;
    use std::any::Any;
    use std::sync::Arc;
    use std::time::SystemTime;

    struct TestPlayerController {
        name: String,
        id: String,
        state: PlaybackState,
        capabilities: PlayerCapabilitySet,
        commands: Arc<Mutex<Vec<PlayerCommand>>>,
    }

    impl TestPlayerController {
        fn new(
            name: &str,
            id: &str,
            state: PlaybackState,
            capabilities: PlayerCapabilitySet,
            commands: Arc<Mutex<Vec<PlayerCommand>>>,
        ) -> Self {
            Self {
                name: name.to_string(),
                id: id.to_string(),
                state,
                capabilities,
                commands,
            }
        }
    }

    impl PlayerController for TestPlayerController {
        fn get_capabilities(&self) -> PlayerCapabilitySet {
            self.capabilities
        }

        fn get_song(&self) -> Option<Song> {
            None
        }

        fn get_queue(&self) -> Vec<Track> {
            Vec::new()
        }

        fn get_loop_mode(&self) -> LoopMode {
            LoopMode::None
        }

        fn get_playback_state(&self) -> PlaybackState {
            self.state
        }

        fn get_position(&self) -> Option<f64> {
            None
        }

        fn get_shuffle(&self) -> bool {
            false
        }

        fn get_player_name(&self) -> String {
            self.name.clone()
        }

        fn get_player_id(&self) -> String {
            self.id.clone()
        }

        fn get_last_seen(&self) -> Option<SystemTime> {
            None
        }

        fn send_command(&self, command: PlayerCommand) -> bool {
            self.commands.lock().push(command);
            true
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn start(&self) -> bool {
            true
        }

        fn stop(&self) -> bool {
            true
        }
    }

    #[test]
    fn enforce_single_playback_pauses_or_stops_other_playing_players() {
        let mut controller = AudioController::new();

        let active_commands = Arc::new(Mutex::new(Vec::new()));
        let other_pause_commands = Arc::new(Mutex::new(Vec::new()));
        let other_stop_commands = Arc::new(Mutex::new(Vec::new()));
        let idle_commands = Arc::new(Mutex::new(Vec::new()));

        let pause_caps = PlayerCapabilitySet::from_slice(&[PlayerCapability::Pause]);
        let stop_caps = PlayerCapabilitySet::from_slice(&[PlayerCapability::Stop]);

        controller.add_controller(Box::new(TestPlayerController::new(
            "source",
            "1",
            PlaybackState::Playing,
            pause_caps,
            Arc::clone(&active_commands),
        )));
        controller.add_controller(Box::new(TestPlayerController::new(
            "other-pause",
            "2",
            PlaybackState::Playing,
            pause_caps,
            Arc::clone(&other_pause_commands),
        )));
        controller.add_controller(Box::new(TestPlayerController::new(
            "other-stop",
            "3",
            PlaybackState::Playing,
            stop_caps,
            Arc::clone(&other_stop_commands),
        )));
        controller.add_controller(Box::new(TestPlayerController::new(
            "idle",
            "4",
            PlaybackState::Paused,
            pause_caps,
            Arc::clone(&idle_commands),
        )));

        let controller = Arc::new(controller);
        let mut monitor = ActiveMonitor::new();
        monitor.base.set_controller(Arc::downgrade(&controller));

        monitor.enforce_single_playback("source", "1");

        assert_eq!(active_commands.lock().len(), 0);
        assert_eq!(idle_commands.lock().len(), 0);

        let other_pause = other_pause_commands.lock();
        assert_eq!(other_pause.len(), 1);
        assert_eq!(other_pause[0], PlayerCommand::Pause);
        drop(other_pause);

        let other_stop = other_stop_commands.lock();
        assert_eq!(other_stop.len(), 1);
        assert_eq!(other_stop[0], PlayerCommand::Stop);
    }

    #[test]
    fn playing_event_switches_active_and_deactivates_previous_playing_source() {
        let mut controller = AudioController::new();

        let first_commands = Arc::new(Mutex::new(Vec::new()));
        let second_commands = Arc::new(Mutex::new(Vec::new()));

        let pause_caps = PlayerCapabilitySet::from_slice(&[PlayerCapability::Pause]);

        controller.add_controller(Box::new(TestPlayerController::new(
            "first",
            "A",
            PlaybackState::Playing,
            pause_caps,
            Arc::clone(&first_commands),
        )));
        controller.add_controller(Box::new(TestPlayerController::new(
            "second",
            "B",
            PlaybackState::Playing,
            pause_caps,
            Arc::clone(&second_commands),
        )));

        let controller = Arc::new(controller);
        let mut monitor = ActiveMonitor::new();
        monitor.base.set_controller(Arc::downgrade(&controller));

        monitor.handle_event_bus_events(PlayerEvent::StateChanged {
            source: PlayerSource::new("second".to_string(), "B".to_string()),
            state: PlaybackState::Playing,
        });

        assert_eq!(controller.get_player_name(), "second");
        assert_eq!(controller.get_player_id(), "B");

        let first = first_commands.lock();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0], PlayerCommand::Pause);

        assert_eq!(second_commands.lock().len(), 0);
    }

    #[test]
    fn immediate_switch_back_between_sources_is_not_dropped() {
        let mut controller = AudioController::new();

        let mpd_commands = Arc::new(Mutex::new(Vec::new()));
        let shairport_commands = Arc::new(Mutex::new(Vec::new()));

        let pause_caps = PlayerCapabilitySet::from_slice(&[PlayerCapability::Pause]);

        controller.add_controller(Box::new(TestPlayerController::new(
            "mpd",
            "localhost:6600",
            PlaybackState::Playing,
            pause_caps,
            Arc::clone(&mpd_commands),
        )));
        controller.add_controller(Box::new(TestPlayerController::new(
            "shairport",
            "shairport",
            PlaybackState::Playing,
            pause_caps,
            Arc::clone(&shairport_commands),
        )));

        let controller = Arc::new(controller);
        let mut monitor = ActiveMonitor::new();
        monitor.base.set_controller(Arc::downgrade(&controller));

        // First switch to shairport.
        monitor.handle_event_bus_events(PlayerEvent::StateChanged {
            source: PlayerSource::new("shairport".to_string(), "shairport".to_string()),
            state: PlaybackState::Playing,
        });
        assert_eq!(controller.get_player_name(), "shairport");

        // Then immediately switch back to mpd.
        monitor.handle_event_bus_events(PlayerEvent::StateChanged {
            source: PlayerSource::new("mpd".to_string(), "localhost:6600".to_string()),
            state: PlaybackState::Playing,
        });

        assert_eq!(controller.get_player_name(), "mpd");
        assert_eq!(controller.get_player_id(), "localhost:6600");
    }

    #[test]
    fn unknown_playing_source_does_not_pause_known_players() {
        let mut controller = AudioController::new();

        let first_commands = Arc::new(Mutex::new(Vec::new()));
        let second_commands = Arc::new(Mutex::new(Vec::new()));

        let pause_caps = PlayerCapabilitySet::from_slice(&[PlayerCapability::Pause]);

        controller.add_controller(Box::new(TestPlayerController::new(
            "first",
            "A",
            PlaybackState::Playing,
            pause_caps,
            Arc::clone(&first_commands),
        )));
        controller.add_controller(Box::new(TestPlayerController::new(
            "second",
            "B",
            PlaybackState::Playing,
            pause_caps,
            Arc::clone(&second_commands),
        )));

        let controller = Arc::new(controller);
        let initial_active_name = controller.get_player_name();
        let initial_active_id = controller.get_player_id();

        let mut monitor = ActiveMonitor::new();
        monitor.base.set_controller(Arc::downgrade(&controller));

        monitor.handle_event_bus_events(PlayerEvent::StateChanged {
            source: PlayerSource::new("ghost".to_string(), "missing".to_string()),
            state: PlaybackState::Playing,
        });

        assert_eq!(controller.get_player_name(), initial_active_name);
        assert_eq!(controller.get_player_id(), initial_active_id);
        assert_eq!(first_commands.lock().len(), 0);
        assert_eq!(second_commands.lock().len(), 0);
    }
}
