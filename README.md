# radiod

A Rust daemon for streaming online radio with MPRIS control.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/jonjomckay/radiod/main/install.sh | bash
```

Or clone and run manually:

```bash
git clone https://github.com/jonjomckay/radiod.git
cd radiod
./install.sh             # latest release
./install.sh v0.1.0      # specific version
./install.sh --build     # build from source
```

> **Requires:** mpv runtime (provides `libmpv.so`). On Arch: `pacman -S mpv`. On Debian/Ubuntu: `apt install mpv libmpv-dev`.

## Configuration

Create `$XDG_CONFIG_HOME/radiod/config.toml` (usually `~/.config/radiod/config.toml`):

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
| `direct:` | `direct:<url>` | Passed directly to mpv for playback |

## Usage

Start and enable the daemon:

```bash
systemctl --user enable --now radiod
```

Control playback:

```bash
radiod-ctl play
radiod-ctl pause
radiod-ctl next          # next station
radiod-ctl previous      # previous station
radiod-ctl now-playing   # show current track
radiod-ctl list-stations # list configured stations
radiod-ctl stop
radiod-ctl volume 0.8           # set volume (0.0–1.0)
radiod-ctl set-station <uri>    # switch to station by URI
radiod-ctl status               # playback status summary
radiod-ctl reload               # reload config from disk
radiod-ctl info                 # full MPRIS properties dump
```

View logs:

```bash
journalctl --user -u radiod -f
```

Media players (e.g. KDE Connect, playerctl) discover the daemon via MPRIS.

## Development

```bash
devenv shell     # enter dev environment
cargo build
cargo run -p radiod
```

## Uninstall

```bash
systemctl --user disable --now radiod
rm ~/.local/bin/radiod
rm ~/.local/bin/radiod-ctl
rm ~/.config/systemd/user/radiod.service
rm ~/.local/share/applications/radiod.desktop
rm ~/.local/share/icons/hicolor/scalable/apps/radiod.svg
systemctl --user daemon-reload
```

Config and data directories (`~/.config/radiod`, `~/.local/share/radiod`) can be removed manually.

## Project Structure

| Crate | Description |
|-------|-------------|
| `radiod` | mpv-based audio daemon |
| `radiod-ctl` | CLI control tool |

## License

MIT
