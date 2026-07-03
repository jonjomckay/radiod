# Plan 04: Custom Control Interface + CLI

## Goal

A custom D-Bus interface on the daemon for station management (beyond what
MPRIS offers), and a `radio-devil-ctl` CLI tool that talks to it.

## Custom D-Bus Interface

Well-known name: `org.mpris.MediaPlayer2.radio_devil` (shared with MPRIS)
Object path: `/org/mpris/MediaPlayer2`
Interface: `org.mpris.MediaPlayer2.radio_devil.Control`

### Methods

| Method | Signature | Behavior |
|---|---|---|
| `SetStation` | `(s uri) → ()` | Switch to a station by URI. Resolves via ORBox client, sends stream URL to player. Emits `StationChanged` signal. |
| `GetStation` | `() → (ss)` | Returns `(name, uri)` of current station. Returns empty strings if none. |
| `ListStations` | `() → a(ss)` | Returns array of `(name, uri)` tuples from config. |
| `ReloadConfig` | `() → ()` | Force-reload configuration from disk. |

### Properties

| Property | Type | Access | Description |
|---|---|---|---|
| `CurrentStation` | `(ss)` | read | `(name, uri)` of active station |
| `Stations` | `a(ss)` | read | Full station list |

### Signals

| Signal | Signature | When |
|---|---|---|
| `StationChanged` | `(s new_uri)` | Emitted when the active station changes (via SetStation, Next, Previous, or config reload that removes current station) |

## CLI Tool (`radio-devil-ctl`)

Use `clap` (derive mode) for argument parsing. Communicate with the daemon over
the session D-Bus using `zbus` (blocking mode is simpler for a CLI — no need
for async here).

### Commands

```
radio-devil-ctl play              → MPRIS Play
radio-devil-ctl pause             → MPRIS Pause
radio-devil-ctl stop              → MPRIS Stop
radio-devil-ctl next              → MPRIS Next
radio-devil-ctl previous          → MPRIS Previous
radio-devil-ctl volume <0.0-1.0>  → MPRIS Volume property set
radio-devil-ctl set-station <uri> → Control.SetStation(uri)
radio-devil-ctl now-playing       → Read MPRIS Metadata property, pretty-print
radio-devil-ctl list-stations     → Control.ListStations(), print table
radio-devil-ctl status            → Print PlaybackStatus, current station, volume
radio-devil-ctl reload            → Control.ReloadConfig()

radio-devil-ctl info              → Pretty-print all MPRIS properties
```

### Behavior

- All commands are one-shot: connect to D-Bus, make the call, print result,
  exit.
- If the daemon isn't running, print a clear error: "Radio Devil daemon is not
  running. Start it with `systemctl --user start radio-devil`."
- `set-station` accepts the full URI string (e.g. `orbox:uk/bbcradio1`).
- `now-playing` prints: artist, title, album (if any), art URL.
- `list-stations` prints a table with the current station highlighted.
- `status` prints a single-line summary: `Playing: BBC Radio 1 (vol 80%)`.

## Implementation Notes

- The CLI should have zero dependencies beyond `clap`, `zbus` (blocking), and
  `anyhow`.
- The daemon's control interface can be implemented as a separate `zbus` struct
  alongside the MPRIS struct, registered on the same connection and object path.
- `SetStation` on the daemon needs access to the ORBox client (from plan 02)
  to resolve the URI. Make sure the daemon's shared state includes the HTTP
  client.

## Verification

- `radio-devil-ctl play` starts playback (visible in `playerctl status`)
- `radio-devil-ctl set-station orbox:uk/bbcdance` switches station
- `radio-devil-ctl list-stations` shows all configured stations
- `radio-devil-ctl now-playing` shows current track metadata
- Running the CLI when daemon isn't running gives a helpful error, not a crash
- Tab-completing station URIs is not required but would be a nice-to-have

## Dependencies on Other Plans

- **01-core**: Config model for station list
- **02-orbox**: URI resolution in `SetStation` handler
- **03-mpris**: Shares the same D-Bus connection and well-known name; CLI also
  uses MPRIS for play/pause/next/prev commands
