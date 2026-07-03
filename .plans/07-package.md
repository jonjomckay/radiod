# Plan 07: Systemd User Unit & Installation

## Goal

Package the daemon for desktop use: a systemd user unit, a `.desktop` file for
MPRIS discovery, and a simple install script.

## systemd User Unit (`radio-devil.service`)

```ini
[Unit]
Description=Radio Devil - Internet Radio Daemon
Documentation=https://github.com/.../radio-devil
After=network-online.target sound.target
Wants=network-online.target sound.target

[Service]
Type=simple
ExecStart=/usr/bin/radio-devild
# Or for cargo-installed: ExecStart=%h/.cargo/bin/radio-devild
Restart=on-failure
RestartSec=5

# Sandboxing (best-effort)
NoNewPrivileges=yes
PrivateTmp=yes
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=%h/.config/radio-devil
ReadWritePaths=%h/.local/share/radio-devil
ReadWritePaths=%h/.cache
ReadOnlyPaths=/usr/lib
ReadOnlyPaths=/usr/share

[Install]
WantedBy=default.target
```

### Key points

- `Type=simple` — the daemon runs in the foreground (systemd manages the
  lifecycle; no fork/detach needed)
- `Restart=on-failure` — if GStreamer crashes or the daemon panics, restart it
- `After=sound.target` — wait for the audio system (PulseAudio/PipeWire) before
  starting
- Sandbox directives are a nice-to-have; if they cause issues on some systems,
  they can be relaxed

### Installation target

`$XDG_CONFIG_HOME/systemd/user/radio-devil.service` (typically
`~/.config/systemd/user/radio-devil.service`).

Commands:
```bash
systemctl --user daemon-reload
systemctl --user enable --now radio-devil
```

## Desktop Entry (`radio-devil.desktop`)

Required for MPRIS identification (the `DesktopEntry` property on the
`MediaPlayer2` interface must match the basename of this file).

```ini
[Desktop Entry]
Type=Application
Name=Radio Devil
Comment=Internet Radio Daemon
Exec=/usr/bin/radio-devild
Icon=radio-devil
Categories=AudioVideo;Player;
NoDisplay=true
StartupNotify=false
X-systemd-skip-watch=true
```

- `NoDisplay=true` — don't show in app launchers (it's a headless daemon)
- `Categories=AudioVideo;Player` — helps desktop environments classify it as a
  media player
- `X-systemd-skip-watch=true` — prevents systemd from trying to track it as a
  transient unit

Install to `/usr/share/applications/` (system-wide) or
`~/.local/share/applications/` (per-user). The latter is sufficient for MPRIS
discovery.

## Icon

A simple SVG or PNG icon installed at one of:
- `~/.local/share/icons/hicolor/scalable/apps/radio-devil.svg`
- `~/.local/share/icons/hicolor/48x48/apps/radio-devil.png`

This is cosmetic — the desktop entry references it but the daemon has no GUI.

## Example Config (`config.example.toml`)

Ship a commented example config that users can copy:

```toml
# Copy to ~/.config/radio-devil/config.toml

[daemon]
volume = 0.8
# default_station = "orbox:uk/bbcradio1"
metadata_poll_interval_secs = 30

[[stations]]
name = "BBC Radio 1 Dance"
uri = "orbox:uk/bbcdance"

# [lastfm]
# api_key = "your_api_key"
# api_secret = "your_api_secret"
# username = "your_username"
```

## Install Script or Makefile

Provide one of:

- A simple `install.sh` that copies binaries, service file, desktop entry, and
  example config to the right places
- A `Makefile` with `install` target
- A `justfile` if the project uses `just`

The install should:
1. Build release binaries (`cargo build --release`)
2. Copy `radio-devild` and `radio-devil-ctl` to `~/.cargo/bin/` (or `/usr/local/bin/` if root)
3. Install the systemd user unit
4. Install the desktop entry
5. Optionally copy the example config to `~/.config/radio-devil/` if one doesn't exist

## Verification

- `systemctl --user status radio-devil` shows the service running after install
- `playerctl -l` lists `radio_devild` (or `radio-devil`)
- `journalctl --user -u radio-devil -f` shows log output
- Rebooting auto-starts the daemon (if `enable` was used)
- Stopping the service: `systemctl --user stop radio-devil`
- Uninstalling: remove service, disable, remove binaries

## Dependencies on Other Plans

- All prior plans must be complete (this is the packaging layer)
