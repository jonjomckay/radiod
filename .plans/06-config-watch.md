# Plan 06: Configuration Auto-Reload

## Goal

Watch the config file for changes and hot-reload the daemon's runtime
configuration without restarting.

## File Watcher

Use the `notify` crate with the `macos_fsevent` or `inotify` backend. Integrate
it with tokio using `notify`'s event stream — convert the notify `Event`s into a
tokio `Stream` (there's a feature flag or an adapter pattern for this; if the
crate doesn't provide a direct tokio stream, wrap it with
`tokio::sync::mpsc`).

Watch the specific config file (`$XDG_CONFIG_HOME/radio-devil/config.toml`), not
the entire directory. Some text editors write to a temp file and rename, which
can trick `notify` — subscribe to the parent directory and filter events for the
config filename to handle this robustly.

## Debouncing

Editors may write multiple times in quick succession. Debounce events: on the
first file change event, start a short timer (e.g. 500ms). If another event
arrives before the timer fires, reset the timer. Only reload when the timer
expires.

## Reload Logic

When the debounced timer fires:

1. Attempt to parse the config file (reuse the existing `Config::load()` function)
2. If parsing fails, log an error with the parse details and **keep the current
   config** (don't crash or lose state)
3. If parsing succeeds:
   - **Station list**: Compare new vs old. Remove stations that were deleted
     and add new ones. If the currently-playing station was removed, stop
     playback and log a warning.
   - **Volume/daemon settings**: Apply new volume (send `SetVolume` to player).
     Update poll interval for future metadata polls.
   - **Last.fm credentials**: If credentials changed, re-authenticate (plan 05).

## Signals

After a successful reload:

- Emit `StationChanged` D-Bus signal (from plan 04) if the current station was
  removed
- Update D-Bus properties via `PropertiesChanged`: the `Stations` list on the
  Control interface should reflect the new config

## Verification

- Edit config, add a new station, save: `radio-devil-ctl list-stations` shows it
  without restarting
- Edit config, remove the currently-playing station, save: playback stops, log
  message appears
- Edit config with invalid TOML, save: error logged, daemon continues with
  previous config intact
- Repeated rapid saves (simulating editor behavior): only one reload occurs,
  after the final save settles
- Volume change in config: takes effect on next save

## Dependencies on Other Plans

- **01-core**: Config loading, player command channel (for volume changes),
  station model
- **03-mpris**: May need to notify MPRIS of station list changes
- **04-control**: `Stations` property and `StationChanged` signal
- **05-lastfm**: Re-authentication on credential changes
