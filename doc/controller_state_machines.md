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

## 3) Bluetooth Controller Runtime State Machine (Current Code)

```mermaid
stateDiagram-v2
    [*] --> Constructed
    Constructed --> AutoDiscoverMode: new_with_address none
    Constructed --> FixedAddressMode: new_with_address with address

    AutoDiscoverMode --> Scanning: start scan thread
    Scanning --> DeviceResolved: device and player path found

    FixedAddressMode --> DeviceResolved: find player path success
    FixedAddressMode --> WaitingForPlayerPath: find player path failed

    DeviceResolved --> Starting: start controller
    WaitingForPlayerPath --> Starting: start controller

    Starting --> Polling: start polling thread
    Polling --> Polling: check active player path
    Polling --> WaitingForPlayerPath: no valid player path
    WaitingForPlayerPath --> DeviceResolved: path discovered later

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
```

### Potential inconsistent states or transitions

- Stale player-path transition: when the stored player path disappears, the code logs and searches for a new path but does not clear the old path immediately on failure. This can keep the controller in a stale "path exists" branch instead of transitioning cleanly to "no path".
- Duration unit inconsistency: one track parsing path converts `Duration` using microseconds to seconds (`/ 1_000_000.0`), while polling track parsing converts with milliseconds to seconds (`/ 1000.0`). That can create inconsistent song duration state depending on call path.
- Auto-discover rediscovery gap on restart: after auto-discover resolves a concrete address once, restart logic only re-enables scanning when address is none. If that remembered address is no longer available, transitions to rediscover a different device are limited.

## Practical conclusion

- The system has one active player pointer, not one active playback source.
- Therefore, collisions are possible in practice: simultaneous playback can occur, while active control focus remains singular and now has anti-flap protection through switch debounce.
