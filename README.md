# Radio Devil

A Rust daemon for streaming online radio with MPRIS control.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/jonjomckay/radio-devil/main/install.sh | bash
```

Or clone and run manually:

```bash
git clone https://github.com/jonjomckay/radio-devil.git
cd radio-devil
./install.sh             # latest release
./install.sh v0.1.0      # specific version
./install.sh --build     # build from source
```

> **Requires:** GStreamer runtime (`gstreamer`, `gst-plugins-base`, `gst-plugins-good`, `gst-plugins-bad`, `gst-plugins-ugly`).

## Configuration

Create `$XDG_CONFIG_HOME/radio-devil/config.toml` (usually `~/.config/radio-devil/config.toml`):

```toml
[[stations]]
name = "BBC Radio 1"
uri = "orbox:uk/bbcradio1"

[[stations]]
name = "BBC Radio 2"
uri = "orbox:uk/bbcradio2"

[[stations]]
name = "My Direct Stream"
uri = "direct:https://example.com/stream.mp3"

[daemon]
volume = 0.8
default_station = "orbox:uk/bbcradio1"
metadata_poll_interval_secs = 30
```

A commented example is at `config.example.toml` in the repo.

### Station URIs

| Scheme | Format | Description |
|--------|--------|-------------|
| `orbox:` | `orbox:<country>/<alias>` | Resolved to a stream URL via the Online Radio Box API |
| `direct:` | `direct:<url>` | Passed directly to GStreamer |

## Usage

Start and enable the daemon:

```bash
systemctl --user enable --now radio-devil
```

Control playback:

```bash
radio-devil-ctl play
radio-devil-ctl pause
radio-devil-ctl next          # next station
radio-devil-ctl previous      # previous station
radio-devil-ctl now-playing   # show current track
radio-devil-ctl stations      # list configured stations
radio-devil-ctl stop
```

View logs:

```bash
journalctl --user -u radio-devil -f
```

Media players (e.g. KDE Connect, playerctl) discover the daemon via MPRIS.

## Development

```bash
devenv shell     # enter dev environment
cargo build
cargo run -p radio-devild
```

## Uninstall

```bash
systemctl --user disable --now radio-devil
rm ~/.local/bin/radio-devild
rm ~/.local/bin/radio-devil-ctl
rm ~/.config/systemd/user/radio-devil.service
rm ~/.local/share/applications/radio-devil.desktop
rm ~/.local/share/icons/hicolor/scalable/apps/radio-devil.svg
systemctl --user daemon-reload
```

Config and data directories (`~/.config/radio-devil`, `~/.local/share/radio-devil`) can be removed manually.

## Project Structure

| Crate | Description |
|-------|-------------|
| `radio-devild` | GStreamer-based audio daemon |
| `radio-devil-ctl` | CLI control tool (stub) |

## License

MIT
