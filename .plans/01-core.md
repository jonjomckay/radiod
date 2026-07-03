# Plan 01: Core — Workspace, Config, and GStreamer Player

## Goal

Cargo workspace skeleton, configuration model from XDG paths, and a GStreamer
audio player running on a dedicated glib thread with command/event channels.

## Workspace

Two crates under a root workspace `Cargo.toml`:

- `radio-devild` — the daemon
- `radio-devil-ctl` — CLI (stub only in this plan)

## Key Dependencies

- `gstreamer` + `gstreamer-audio` for playback
- `serde` + `toml` for config
- `dirs` for XDG paths
- `tokio` for async runtime
- `tracing` + `tracing-subscriber` for logging

## Configuration (`config.rs`)

Config file location: `$XDG_CONFIG_HOME/radio-devil/config.toml`

TOML format with three sections:

- `[lastfm]` — API key, secret, username, optional password (used later; load it
  now so the schema is stable)
- `[[stations]]` — array of `{ name, uri }` where URI uses the scheme
  `orbox:<country>/<alias>` (e.g. `orbox:uk/bbcradio1`)
- `[daemon]` — `volume` (0.0–1.0, default 0.8), `default_station` (optional
  URI), `metadata_poll_interval_secs` (default 30)

A `StationUri` type in `station.rs` parses the `orbox:` scheme into
`{ country, alias }`. It should reject unknown schemes. Future schemes (e.g.
`direct:https://...`) can be added to the parser without changing the config
format.

Data directory: `$XDG_DATA_HOME/radio-devil/` — created on startup, used later
for Last.fm session persistence.

## Player Subsystem (`player.rs`)

### Threading model

The player runs on a dedicated OS thread with its own `glib::MainLoop`.
The main tokio runtime communicates with it via two channels:

- **Commands** (`tokio::sync::mpsc`): Play, Pause, Stop, SetVolume(f64),
  SetUri(String), Quit
- **Events** (`tokio::sync::broadcast`): StateChanged(Playing|Paused|Stopped),
  Error(String), EndOfStream, VolumeChanged(f64)

Commands arriving on the mpsc channel get forwarded onto the glib context using
`glib::MainContext::channel` so the GStreamer API calls happen on the right
thread.

### GStreamer pipeline

Build programmatically (not from a string) with:

```
uridecodebin → audioconvert → audioresample → volume → autoaudiosink
```

- `uridecodebin` uses `pad-added` signal to handle dynamic pads — only link
  audio pads, ignore video. This handles any audio format GStreamer supports
  (ICY, DASH, HLS, plain HTTP MP3/AAC/OGG, etc).
- `volume` element is named so it can be looked up by `set_property("volume",
  ...)` for volume control.
- The pipeline is built once. `SetUri` changes the URI property on
  `uridecodebin` and transitions the pipeline state (Null → Playing).
- Play/Pause/Stop are mapped to GStreamer pipeline state transitions (Playing /
  Paused / Null).
- A bus watch forwards `Eos`, `Error`, and `StateChanged` messages to the
  event broadcast channel.

## Main Entry Point (`main.rs`)

1. Init logging (tracing with env-filter, default level `info`)
2. Load config from XDG path (fail with a clear error if missing or invalid)
3. Create `$XDG_DATA_HOME/radio-devil/` if it doesn't exist
4. Create the command/event channels
5. Resolve `default_station` from config (match against station list by URI)
6. Spawn player thread with channels, resolved URI (or none), and volume
7. Enter an event loop: print player events, listen for Ctrl+C
8. On shutdown, send `Quit` command to player thread and join it

## Verification

- `cargo build` compiles without errors
- Running `radio-devild` with a valid config plays audio from a stream URL
- Ctrl+C exits cleanly (no leaked threads, no hanging)
- Missing/invalid config file produces a clear error message

## Dependencies on Other Plans

- **02-orbox**: Uses `StationUri` from this plan
- **03-mpris**: Needs command/event channels and `PlaybackState` type
- **04-control**: Needs station config model
- **05-lastfm**: Needs config `[lastfm]` section and data dir
- **06-config-watch**: Needs config loading function to call on file change
