# Plan 03: MPRIS D-Bus Interface

## Goal

Implement the `org.mpris.MediaPlayer2` and `org.mpris.MediaPlayer2.Player`
D-Bus interfaces using `zbus`, registered on the session bus at
`/org/mpris/MediaPlayer2` with the well-known name
`org.mpris.MediaPlayer2.radio_devil`.

## Tooling

Use `zbus` (3.x) with its `#[dbus_interface]` derive macro. Define a struct
that holds shared state (playback status, metadata, station list) behind
`Arc<RwLock<>>` or similar interior mutability so the D-Bus handler and the
main event loop can read/write safely.

Property changes should emit `org.freedesktop.DBus.Properties.PropertiesChanged`
signals — `zbus` has built-in support for this via `#[dbus_interface(property)]`
or manual emission.

## Interface: `org.mpris.MediaPlayer2`

### Properties (read-only unless noted)

| Property | Value / Behavior |
|---|---|
| `Identity` | `"Radio Devil"` |
| `DesktopEntry` | `"radio-devil"` (matches `.desktop` file installed later) |
| `CanQuit` | `true` |
| `CanRaise` | `false` (no GUI) |
| `CanSetFullscreen` | `false` |
| `HasTrackList` | `false` |
| `SupportedUriSchemes` | `["orbox"]` |
| `SupportedMimeTypes` | `[]` (we delegate to GStreamer, not worth enumerating) |
| `Fullscreen` | `false` (read-write per spec, but setting it is a no-op) |

### Methods

| Method | Behavior |
|---|---|
| `Raise` | No-op (CanRaise is false) |
| `Quit` | Initiate daemon shutdown (send Quit to player, tear down D-Bus, exit) |

## Interface: `org.mpris.MediaPlayer2.Player`

### Properties

| Property | Value / Behavior |
|---|---|
| `PlaybackStatus` | `"Playing"`, `"Paused"`, or `"Stopped"` — derived from player thread events |
| `LoopStatus` | `"None"` (radio doesn't loop) |
| `Rate` | `1.0` (always; seek not supported) |
| `Shuffle` | `false` |
| `Volume` | `0.0`–`1.0`, read-write. Setting it sends `SetVolume` to player thread |
| `Position` | `-1` (unknown, per MPRIS spec for non-seekable streams) |
| `MinimumRate` | `1.0` |
| `MaximumRate` | `1.0` |
| `CanGoNext` | `true` — cycles to next station in config |
| `CanGoPrevious` | `true` — cycles to previous station in config |
| `CanPlay` | `true` |
| `CanPause` | `true` — mapped to stop/mute (see below) |
| `CanSeek` | `false` (live streams) |
| `CanControl` | `true` |

### Methods

| Method | Behavior |
|---|---|
| `Play` | Send `Play` to player thread |
| `Pause` | Send `Pause` to player thread |
| `PlayPause` | Toggle between Play and Pause |
| `Stop` | Send `Stop` to player thread |
| `Next` | Activate next station in config list |
| `Previous` | Activate previous station in config list |
| `Seek(offset)` | No-op (CanSeek is false) |
| `SetPosition(track_id, position)` | No-op |
| `OpenUri(uri)` | If URI starts with `orbox:`, change station to that URI |

### Play/Pause Semantics for Live Radio

Pausing a live stream doesn't make sense (you can't resume at the same point).
Map MPRIS Pause to GStreamer state Paused instead (muted but pipeline stays
active). This is what most radio clients do.

### Signals

| Signal | When emitted |
|---|---|
| `Seeked(position)` | Never (not seekable) |

## Metadata Mapping

When the metadata watch channel updates, map the ORBox metadata struct to MPRIS
`Metadata` dictionary:

| MPRIS Key | Source |
|---|---|
| `mpris:trackid` | Constructed from station alias + timestamp (e.g. `/org/mpris/MediaPlayer2/radio_devil/track/uk_bbcradio1_1783067791`) |
| `xesam:title` | `metadata.title` |
| `xesam:artist` | `[metadata.artist]` (array of strings) |
| `mpris:artUrl` | `metadata.art_url` |
| `xesam:album` | `metadata.album` if present |
| `mpris:length` | Omitted (unknown for streams) |

Emit `PropertiesChanged` on the `Metadata` property after each update so
desktop environments pick up track changes.

## Integration

The MPRIS struct lives on the tokio runtime (zbus is async). It:

1. **Subscribes** to the player event broadcast channel to track playback state
   and volume changes
2. **Subscribes** to the metadata watch channel to update track metadata
3. **Sends commands** to the player thread's mpsc channel for play/pause/volume
4. **Holds a reference** to the station list for next/previous logic and URI
   resolution

All of this shared state must be thread-safe — use `Arc<RwLock<SharedState>>` or
similar.

## Verification

- `playerctl -p radio_devil status` reports `Playing`/`Paused`/`Stopped`
- `playerctl -p radio_devil metadata` shows artist, title, art URL
- `playerctl -p radio_devil play-pause` toggles playback
- `playerctl -p radio_devil next` changes station
- `playerctl -p radio_devil volume 0.5` adjusts volume
- GNOME/KDE media controls widget shows the player and responds to clicks
- `playerctl -p radio_devil quit` shuts down the daemon

## Dependencies on Other Plans

- **01-core**: Player command/event channels, `PlaybackState` enum
- **02-orbox**: Metadata output type, `StationUri`
- **04-control**: Separate interface, no dependency (can be implemented in parallel)
