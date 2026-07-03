use std::collections::HashMap;

use anyhow::Context;
use clap::Parser;
use zbus::blocking::{fdo::PropertiesProxy, Connection};
use zbus::names::InterfaceName;
use zbus::zvariant::OwnedValue;

const DEST: &str = "org.mpris.MediaPlayer2.radiod";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const CONTROL_IFACE: &str = "org.mpris.MediaPlayer2.radiod.Control";

#[derive(Parser)]
#[command(name = "radiod-ctl", about = "Control the radiod daemon")]
enum Cli {
    /// Resume playback
    Play,
    /// Pause playback
    Pause,
    /// Stop playback
    Stop,
    /// Skip to next station
    Next,
    /// Go to previous station
    Previous,
    /// Set volume (0.0–1.0)
    Volume { value: f64 },
    /// Switch to a station by URI
    #[command(name = "set-station")]
    SetStation { uri: String },
    /// Show current track metadata
    #[command(name = "now-playing")]
    NowPlaying,
    /// List all configured stations
    #[command(name = "list-stations")]
    ListStations,
    /// Show playback status summary
    Status,
    /// Reload configuration from disk
    Reload,
    /// Print all MPRIS properties
    Info,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let conn = connect()?;
    let player_iface = InterfaceName::from_str_unchecked(PLAYER_IFACE);
    let _control_iface = InterfaceName::from_str_unchecked(CONTROL_IFACE);

    match cli {
        Cli::Play => player_call(&conn, "Play", &())?,
        Cli::Pause => player_call(&conn, "Pause", &())?,
        Cli::Stop => player_call(&conn, "Stop", &())?,
        Cli::Next => player_call(&conn, "Next", &())?,
        Cli::Previous => player_call(&conn, "Previous", &())?,
        Cli::Volume { value } => {
            let props = properties_proxy(&conn)?;
            let val = zbus::zvariant::Value::new(value);
            props
                .set(player_iface.clone(), "Volume", &val)
                .context("failed to set Volume")?;
        }
        Cli::SetStation { uri } => {
            control_call(&conn, "SetStation", &(uri,))?;
        }
        Cli::NowPlaying => cmd_now_playing(&conn, &player_iface)?,
        Cli::ListStations => cmd_list_stations(&conn)?,
        Cli::Status => cmd_status(&conn, &player_iface)?,
        Cli::Reload => {
            control_call(&conn, "ReloadConfig", &())?;
            println!("Configuration reloaded.");
        }
        Cli::Info => cmd_info(&conn, &player_iface)?,
    }

    Ok(())
}

fn connect() -> anyhow::Result<Connection> {
    Connection::session()
        .context("radiod daemon is not running. Start it with `systemctl --user start radiod`.")
}

fn properties_proxy(conn: &Connection) -> anyhow::Result<PropertiesProxy<'_>> {
    PropertiesProxy::new(conn, DEST, PATH).context("failed to create properties proxy")
}

fn player_call<B: serde::Serialize + zbus::zvariant::DynamicType>(
    conn: &Connection,
    method: &str,
    body: &B,
) -> anyhow::Result<()> {
    let iface = InterfaceName::from_str_unchecked(PLAYER_IFACE);
    conn.call_method(Some(DEST), PATH, Some(iface.as_str()), method, body)
        .context(format!("failed to call Player.{}", method))?;
    Ok(())
}

fn control_call<B: serde::Serialize + zbus::zvariant::DynamicType>(
    conn: &Connection,
    method: &str,
    body: &B,
) -> anyhow::Result<()> {
    let iface = InterfaceName::from_str_unchecked(CONTROL_IFACE);
    conn.call_method(Some(DEST), PATH, Some(iface.as_str()), method, body)
        .context(format!("failed to call Control.{}", method))?;
    Ok(())
}

fn get_prop<T: TryFrom<OwnedValue>>(
    props: &PropertiesProxy,
    iface: &InterfaceName<'_>,
    name: &str,
) -> anyhow::Result<T> {
    let val: OwnedValue = props.get(iface.clone(), name)?;
    val.try_into()
        .map_err(|_| anyhow::anyhow!("failed to convert property '{}'", name))
}

fn cmd_now_playing(conn: &Connection, player_iface: &InterfaceName<'_>) -> anyhow::Result<()> {
    let props = properties_proxy(conn)?;
    let meta: HashMap<String, OwnedValue> =
        get_prop(&props, player_iface, "Metadata").context("failed to read Metadata")?;

    let artist = extract_string_list(&meta, "xesam:artist").join(", ");
    let title = extract_string(&meta, "xesam:title");
    let album = extract_string(&meta, "xesam:album");
    let art_url = extract_string(&meta, "mpris:artUrl");

    if title.is_empty() && artist.is_empty() {
        println!("No track metadata available.");
        return Ok(());
    }

    if !title.is_empty() {
        println!("Title:  {}", title);
    }
    if !artist.is_empty() {
        println!("Artist: {}", artist);
    }
    if !album.is_empty() {
        println!("Album:  {}", album);
    }
    if !art_url.is_empty() {
        println!("Art:    {}", art_url);
    }

    Ok(())
}

fn extract_string(map: &HashMap<String, OwnedValue>, key: &str) -> String {
    map.get(key)
        .and_then(|v| v.downcast_ref::<String>().ok().map(|s| s.clone()))
        .unwrap_or_default()
}

fn extract_string_list(map: &HashMap<String, OwnedValue>, key: &str) -> Vec<String> {
    map.get(key)
        .and_then(|v| v.downcast_ref::<zbus::zvariant::Array>().ok())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    v.downcast_ref::<zbus::zvariant::Str>()
                        .ok()
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn cmd_list_stations(conn: &Connection) -> anyhow::Result<()> {
    let control_iface = InterfaceName::from_str_unchecked(CONTROL_IFACE);

    let reply = conn.call_method(
        Some(DEST),
        PATH,
        Some(control_iface.as_str()),
        "ListStations",
        &(),
    )?;
    let stations: Vec<(String, String)> = reply
        .body()
        .deserialize()
        .context("failed to deserialize ListStations reply")?;

    let reply = conn.call_method(
        Some(DEST),
        PATH,
        Some(control_iface.as_str()),
        "GetStation",
        &(),
    )?;
    let current: (String, String) = reply
        .body()
        .deserialize()
        .context("failed to deserialize GetStation reply")?;

    if stations.is_empty() {
        println!("No stations configured.");
        return Ok(());
    }

    let name_width = stations
        .iter()
        .map(|(name, _)| name.len())
        .max()
        .unwrap_or(4)
        .max(4);

    println!("{:<name_width$}  URI", "Name", name_width = name_width);
    println!("{:-<name_width$}  {}", "", "---", name_width = name_width);

    for (name, uri) in &stations {
        let marker = if uri == &current.1 { "*" } else { " " };
        println!(
            "{} {:<name_width$}  {}",
            marker,
            name,
            uri,
            name_width = name_width
        );
    }

    Ok(())
}

fn cmd_status(conn: &Connection, player_iface: &InterfaceName<'_>) -> anyhow::Result<()> {
    let props = properties_proxy(conn)?;

    let status: String =
        get_prop(&props, player_iface, "PlaybackStatus").unwrap_or_else(|_| "Stopped".into());
    let volume: f64 = get_prop(&props, player_iface, "Volume").unwrap_or(0.0);

    let control_iface = InterfaceName::from_str_unchecked(CONTROL_IFACE);
    let reply = conn.call_method(
        Some(DEST),
        PATH,
        Some(control_iface.as_str()),
        "GetStation",
        &(),
    )?;
    let (name, _uri): (String, String) = reply.body().deserialize().unwrap_or_default();

    let vol_pct = (volume * 100.0).round() as u32;

    if name.is_empty() {
        println!("{} (vol {}%)", status, vol_pct);
    } else {
        println!("{}: {} (vol {}%)", status, name, vol_pct);
    }

    Ok(())
}

fn cmd_info(conn: &Connection, player_iface: &InterfaceName<'_>) -> anyhow::Result<()> {
    let props = properties_proxy(conn)?;

    let status: String =
        get_prop(&props, player_iface, "PlaybackStatus").unwrap_or_else(|_| "Stopped".into());
    let volume: f64 = get_prop(&props, player_iface, "Volume").unwrap_or(0.0);
    let loop_status: String =
        get_prop(&props, player_iface, "LoopStatus").unwrap_or_else(|_| "None".into());
    let shuffle: bool = get_prop(&props, player_iface, "Shuffle").unwrap_or(false);
    let rate: f64 = get_prop(&props, player_iface, "Rate").unwrap_or(1.0);
    let position: i64 = get_prop(&props, player_iface, "Position").unwrap_or(0);

    let meta: HashMap<String, OwnedValue> =
        get_prop(&props, player_iface, "Metadata").unwrap_or_default();
    let title = extract_string(&meta, "xesam:title");
    let artist = extract_string_list(&meta, "xesam:artist").join(", ");
    let album = extract_string(&meta, "xesam:album");
    let art_url = extract_string(&meta, "mpris:artUrl");

    let control_iface = InterfaceName::from_str_unchecked(CONTROL_IFACE);
    let reply = conn.call_method(
        Some(DEST),
        PATH,
        Some(control_iface.as_str()),
        "GetStation",
        &(),
    )?;
    let (station_name, station_uri): (String, String) =
        reply.body().deserialize().unwrap_or_default();

    let reply = conn.call_method(
        Some(DEST),
        PATH,
        Some(control_iface.as_str()),
        "ListStations",
        &(),
    )?;
    let stations: Vec<(String, String)> = reply.body().deserialize().unwrap_or_default();

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("Identity:       {}", "radiod"));
    lines.push(format!("DesktopEntry:   {}", "radiod"));
    lines.push(format!("PlaybackStatus: {}", status));
    lines.push(format!("LoopStatus:     {}", loop_status));
    lines.push(format!("Rate:           {}", rate));
    lines.push(format!("Shuffle:        {}", shuffle));
    lines.push(format!("Volume:         {:.2}", volume));
    lines.push(format!("Position:       {}", position));

    if !title.is_empty() {
        lines.push(format!("Title:          {}", title));
    }
    if !artist.is_empty() {
        lines.push(format!("Artist:         {}", artist));
    }
    if !album.is_empty() {
        lines.push(format!("Album:          {}", album));
    }
    if !art_url.is_empty() {
        lines.push(format!("ArtUrl:         {}", art_url));
    }

    if !station_name.is_empty() {
        lines.push(format!(
            "Station:        {} ({})",
            station_name, station_uri
        ));
    }
    lines.push(format!("Station count:  {}", stations.len()));

    for line in lines {
        println!("{}", line);
    }

    Ok(())
}
