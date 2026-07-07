# Controller State Machines

This document describes the runtime state machines for player orchestration and service startup/shutdown in AudioControl.

## Global Controller and Service State Machine

```mermaid
stateDiagram-v2
    [*] --> Boot
    Boot --> ConfigLoaded
    ConfigLoaded --> CoreServicesInit: initialize cache db security providers
    CoreServicesInit --> ControllerBuilt: build AudioController from_json
    ControllerBuilt --> PlayersStarted: start all configured players

    PlayersStarted --> WebserverDisabled: webserver.enable=false
    PlayersStarted --> WebserverStarting: webserver.enable=true
    WebserverStarting --> WebserverRunning: Rocket launch ok
    WebserverStarting --> WebserverFailed: Rocket launch error

    WebserverDisabled --> MainLoop
    WebserverRunning --> MainLoop
    WebserverFailed --> MainLoop

    MainLoop --> ShutdownRequested: Ctrl C signal
    ShutdownRequested --> Exit

    state MainLoop {
        [*] --> ActiveSelection
        ActiveSelection --> ActiveSelection: StateChanged(Playing) event from player
        ActiveSelection --> ActiveSelection: set active controller and emit ActivePlayerChanged
        ActiveSelection --> ActiveSelection: enforce single playback pause or stop other playing players
        ActiveSelection --> ActiveSelection: route command to active player
        ActiveSelection --> ActiveSelection: zero players playing is allowed
        ActiveSelection --> ActiveSelection: API pause-all or stop-all targets all players
    }
```

### What this means

- All configured players are started. There is one active selector (`active_index`) in `AudioController`.
- Commands routed through `AudioController` (`send_command`) target only the current active player.
- Active player selection is updated by `ActiveMonitor` when a player emits `StateChanged(Playing)`.
- On `StateChanged(Playing)`, `ActiveMonitor` also enforces single-playback by pausing (or stopping if pause is unavailable) other players that are currently `Playing`.
- A successful active switch publishes `ActivePlayerChanged` on the global event bus.
- API endpoints `pause-all`/`stop-all` intentionally target all players (with optional exclusion).

### Exclusivity vs collisions

- Active selection is exclusive: only one `active_index` exists at a time.
- Playback is enforced to be single-source: at most one player should remain in `Playing` after `StateChanged(Playing)` handling completes.
- Allowed idle behavior remains unchanged: zero players in `Playing` is valid.
- Collision behavior: if multiple players emit `Playing` close together, last processed event decides active focus, and non-source players are paused/stopped where capability support allows.

## Backend-Specific Player State Machine (MPD, Bluetooth, Shairport, Librespot)

```mermaid
stateDiagram-v2
    [*] --> BackendInit

    state BackendInit {
        [*] --> MPDStart
        [*] --> BluetoothStart
        [*] --> ShairportStart
        [*] --> LibrespotStart

        MPDStart --> MPDIdleLoop: start() + listener thread
        BluetoothStart --> BTPolling: start() + polling thread
        ShairportStart --> SHAListener: start() + UDP listener + watcher
        LibrespotStart --> SPEventMode: start() API/update mode
    }

    state MPDIdleLoop {
        [*] --> MPDDisconnected
        MPDDisconnected --> MPDConnected: connect ok
        MPDConnected --> MPDPlaying: MPD state=Play
        MPDConnected --> MPDPaused: MPD state=Pause
        MPDConnected --> MPDStopped: MPD state=Stop
        MPDPlaying --> MPDPaused: Pause/PlayPause
        MPDPlaying --> MPDStopped: Stop
        MPDPaused --> MPDPlaying: Play/PlayPause
        MPDStopped --> MPDPlaying: Play/PlayQueueIndex
        MPDConnected --> MPDDisconnected: idle/connection error
    }

    state BTPolling {
        [*] --> BTNoPath
        BTNoPath --> BTPathFound: discover/find MediaPlayer1
        BTPathFound --> BTPlaying: D-Bus status=playing
        BTPathFound --> BTPaused: D-Bus status=paused
        BTPathFound --> BTStopped: D-Bus status=stopped
        BTPlaying --> BTPaused: Pause/PlayPause
        BTPaused --> BTPlaying: Play/PlayPause
        BTPlaying --> BTStopped: Stop
        BTPathFound --> BTNoPath: player path vanished
    }

    state SHAListener {
        [*] --> SHAStopped
        SHAStopped --> SHAPlaying: AUDIO_BEGIN/PLAYBACK_BEGIN/RESUME/METADATA_START
        SHAPlaying --> SHAPaused: PAUSE
        SHAPaused --> SHAPlaying: RESUME
        SHAPlaying --> SHAStopped: SESSION_END
        SHAPaused --> SHAStopped: SESSION_END
    }

    state SPEventMode {
        [*] --> SPUnknown
        SPUnknown --> SPPlaying: API state_changed=playing
        SPUnknown --> SPPaused: API state_changed=paused
        SPUnknown --> SPStopped: API state_changed=stopped
        SPPlaying --> SPPaused: Pause/Stop or API update
        SPPaused --> SPPlaying: Play or API update
        SPStopped --> SPPlaying: Play or API update
        SPUnknown --> SPKilled: API state_changed=killed
        SPUnknown --> SPDisconnected: API state_changed=disconnected
    }
```

### Notes by backend

- MPD: event listener thread updates state from MPD idle/status; reconnect logic moves between disconnected and connected states.
- Bluetooth: polling thread maps BlueZ `Status` to playback state; auto-discovery/path switching can transition to and from "no path".
- Shairport: UDP control messages define transitions explicitly (`PAUSE`, `RESUME`, `AUDIO_BEGIN`, `SESSION_END`).
- Librespot: state is primarily event-driven via incoming API events; commands use Spotify API when token is valid.

## Bluetooth Controller Runtime State Machine Details

```mermaid
stateDiagram-v2
    [*] --> Constructed
    Constructed --> AutoDiscoverMode: new_with_address none
    Constructed --> FixedAddressMode: new_with_address with address

    AutoDiscoverMode --> Scanning: scan when no player path
    Scanning --> DeviceResolved: device and player path found

    FixedAddressMode --> DeviceResolved: find player path success
    FixedAddressMode --> WaitingForPlayerPath: find player path failed

    DeviceResolved --> Starting: start controller
    WaitingForPlayerPath --> Starting: start controller

    Starting --> Polling: start polling thread
    Polling --> Polling: validate or switch player path
    Polling --> WaitingForPlayerPath: no valid player path
    WaitingForPlayerPath --> Scanning: auto discover mode and no path
    WaitingForPlayerPath --> DeviceResolved: replacement path discovered

    Polling --> Playing: status playing
    Polling --> Paused: status paused
    Polling --> Stopped: status stopped
    Polling --> Unknown: status unknown or read error

    Playing --> Paused: pause or play pause
    Playing --> Stopped: stop
    Paused --> Playing: play or play pause
    Stopped --> Playing: play

    Polling --> Stopping: stop controller
    Stopping --> StoppedController: join threads and clear connection and path
    StoppedController --> Starting: start controller again

    Polling --> WaitingForPlayerPath: rescan clears address path and name
    WaitingForPlayerPath --> Scanning: rescan in auto discover mode
```

### Notes on behavior

- Stale player-path handling is explicit: invalid path transitions now clear to no-path when no replacement is found.
- Auto-discover restart and rescan behavior now keys off missing player path in auto-discover mode, so rediscovery can continue after path loss.

## Generic Controller State Machine Details

```mermaid
stateDiagram-v2
    [*] --> Constructed: new or from_config
    Constructed --> Started: start
    Started --> StoppedController: stop
    StoppedController --> Started: start again

    state PlaybackStateMachine {
        [*] --> Unknown
        Unknown --> Playing: command Play PlayPause or event state_changed playing
        Unknown --> Paused: command Pause or event state_changed paused
        Unknown --> Stopped: command Stop or event state_changed stopped
        Unknown --> Killed: event state_changed killed
        Unknown --> Disconnected: event state_changed disconnected

        Playing --> Paused: command Pause PlayPause or event paused
        Playing --> Stopped: command Stop or event stopped
        Paused --> Playing: command Play PlayPause or event playing
        Stopped --> Playing: command Play or event playing

        Unknown --> Unknown: invalid event state rejected
        Playing --> Playing: invalid event state rejected
        Paused --> Paused: invalid event state rejected
        Stopped --> Stopped: invalid event state rejected
    }

    state MetadataAndControls {
        [*] --> IdleMeta
        IdleMeta --> SongSet: event song_changed with song payload
        SongSet --> SongCleared: event song_changed without song payload
        IdleMeta --> PositionSet: command Seek or event position_changed valid numeric
        PositionSet --> PositionSet: command Seek or event position_changed valid numeric
        IdleMeta --> IdleMeta: invalid or negative position rejected
        PositionSet --> PositionSet: invalid or negative position rejected
        IdleMeta --> LoopSet: command SetLoopMode or event loop_mode_changed
        IdleMeta --> ShuffleSet: command SetRandom or event shuffle_changed
    }
```

### Current behavior notes

- Command and API-event paths are now aligned for state/loop/shuffle/position transitions: both update local state and emit corresponding notifications.
- Invalid state_changed strings are rejected and do not force a transition to Unknown.
- Seek and position_changed reject invalid numeric input (non-finite or negative values).
- PlayPause is implemented and included in default generic capabilities.
- Lifecycle transitions remain intentionally shallow: start and stop do not force playback-state changes.

## Practical conclusion

- The system has one active player pointer and now actively enforces one playback source.
- Temporary overlap can still occur during backend event races, but `ActiveMonitor` converges to at most one `Playing` player by pausing/stopping others as events are processed.
