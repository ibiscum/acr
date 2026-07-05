# Controller State Machines

This document describes the runtime state machines for player orchestration and service startup/shutdown in AudioControl.

## 1) Global Controller and Service State Machine

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
        ActiveSelection --> ActiveSelection: Playing event from ActiveMonitor
        ActiveSelection --> ActiveSelection: ignore switch during 500ms debounce
        ActiveSelection --> ActiveSelection: set active controller
        ActiveSelection --> ActiveSelection: emit ActivePlayerChanged
        ActiveSelection --> ActiveSelection: route command to active player
        ActiveSelection --> ActiveSelection: pause all or stop all across players
    }
```

### What this means

- All configured players are started. There is one active selector (`active_index`) in `AudioController`.
- Commands routed through `AudioController` (`send_command`) target only the current active player.
- Active player selection is updated by `ActiveMonitor` when a player emits `StateChanged(Playing)`, with a 500ms debounce to reduce flapping.
- A successful active switch publishes `ActivePlayerChanged` on the global event bus.
- API endpoints `pause-all`/`stop-all` intentionally target all players (with optional exclusion).

### Exclusivity vs collisions

- Active selection is exclusive: only one `active_index` exists at a time.
- Playback is not exclusive: multiple players can be in `Playing` simultaneously.
- Collision behavior: if multiple players emit `Playing` close together inside the debounce window, later events are ignored; outside the window, event order still decides which one ends up active.

## 2) Backend-Specific Player State Machine (MPD, Bluetooth, Shairport, Librespot)

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

## Practical conclusion

- The system has one active player pointer, not one active playback source.
- Therefore, collisions are possible in practice: simultaneous playback can occur, while active control focus remains singular and now has anti-flap protection through switch debounce.
