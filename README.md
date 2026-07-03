# Radio Devil

A Rust daemon for streaming online radio with MPRIS control.

## Quick Start

```bash
# Enter the development environment (requires Nix with flakes enabled)
devenv shell

# Build
cargo build
```

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

### Station URIs

| Scheme | Format | Description |
|--------|--------|-------------|
| `orbox:` | `orbox:<country>/<alias>` | Resolved to a stream URL via the Online Radio Box API (requires Plan 02) |
| `direct:` | `direct:<url>` | Passed directly to GStreamer — works now for testing |

## Usage

```bash
# Run the daemon
cargo run -p radio-devild
```

Press `Ctrl+C` to shut down.

## Project Structure

| Crate | Description |
|-------|-------------|
| `radio-devild` | GStreamer-based audio daemon |
| `radio-devil-ctl` | CLI control tool (stub) |

## License

MIT
