use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use tokio::sync::{broadcast, watch, RwLock};
use tracing;
use zbus::connection;
use zbus::interface;
use zbus::zvariant::{ObjectPath, Value};

use crate::config::StationConfig;
use crate::metadata::Metadata;
use crate::player::{PlaybackState, PlayerCommand, PlayerEvent};
use crate::station::StationUri;

pub(crate) struct MprisState {
    pub(crate) playback_status: PlaybackState,
    pub(crate) volume: f64,
    pub(crate) metadata: Metadata,
    pub(crate) stations: Vec<StationConfig>,
    pub(crate) current_station_index: usize,
    pub(crate) current_station_uri: String,
    pub(crate) station_changed_tx: broadcast::Sender<String>,
}

struct MediaPlayer2 {
    quit_tx: Arc<watch::Sender<bool>>,
}

struct MediaPlayer2Player {
    state: Arc<RwLock<MprisState>>,
    cmd_tx: std::sync::mpsc::Sender<PlayerCommand>,
    station_tx: tokio::sync::mpsc::Sender<StationUri>,
}

// ---------------------------------------------------------------------------
// org.mpris.MediaPlayer2
// ---------------------------------------------------------------------------

#[interface(name = "org.mpris.MediaPlayer2")]
impl MediaPlayer2 {
    #[zbus(property)]
    async fn identity(&self) -> &str {
        "Radio Devil"
    }

    #[zbus(property)]
    async fn desktop_entry(&self) -> &str {
        "radio-devil"
    }

    #[zbus(property)]
    async fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn can_set_fullscreen(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["orbox".to_string()]
    }

    #[zbus(property)]
    async fn supported_mime_types(&self) -> &[String] {
        &[]
    }

    #[zbus(property)]
    async fn fullscreen(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn set_fullscreen(&mut self, _fullscreen: bool) {}

    async fn raise(&self) {
        // no-op: CanRaise is false
    }

    async fn quit(&mut self) {
        let _ = self.quit_tx.send(true);
    }
}

// ---------------------------------------------------------------------------
// org.mpris.MediaPlayer2.Player
// ---------------------------------------------------------------------------

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl MediaPlayer2Player {
    // --- properties ---

    #[zbus(property)]
    async fn playback_status(&self) -> String {
        match self.state.read().await.playback_status {
            PlaybackState::Playing => "Playing".to_string(),
            PlaybackState::Paused => "Paused".to_string(),
            PlaybackState::Stopped => "Stopped".to_string(),
        }
    }

    #[zbus(property)]
    async fn loop_status(&self) -> &str {
        "None"
    }

    #[zbus(property)]
    async fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    async fn shuffle(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn volume(&self) -> f64 {
        self.state.read().await.volume
    }

    #[zbus(property)]
    async fn set_volume(&mut self, vol: f64) {
        let vol = vol.clamp(0.0, 1.0);
        {
            self.state.write().await.volume = vol;
        }
        let _ = self.cmd_tx.send(PlayerCommand::SetVolume(vol));
    }

    #[zbus(property)]
    async fn position(&self) -> i64 {
        -1
    }

    #[zbus(property)]
    async fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    async fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    async fn can_go_next(&self) -> bool {
        self.state.read().await.stations.len() > 1
    }

    #[zbus(property)]
    async fn can_go_previous(&self) -> bool {
        self.state.read().await.stations.len() > 1
    }

    #[zbus(property)]
    async fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn can_seek(&self) -> bool {
        false
    }

    #[zbus(property)]
    async fn can_control(&self) -> bool {
        true
    }

    #[zbus(property)]
    async fn metadata(&self) -> HashMap<String, Value<'static>> {
        let state = self.state.read().await;
        make_metadata_dict(&state.metadata, &state.current_station_uri)
    }

    // --- methods ---

    async fn play(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Play);
    }

    async fn pause(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Pause);
    }

    async fn play_pause(&self) {
        let state = self.state.read().await;
        let cmd = match state.playback_status {
            PlaybackState::Playing => PlayerCommand::Pause,
            _ => PlayerCommand::Play,
        };
        drop(state);
        let _ = self.cmd_tx.send(cmd);
    }

    async fn stop(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Stop);
    }

    async fn next(&self) {
        let mut state = self.state.write().await;
        if state.stations.is_empty() {
            return;
        }
        state.current_station_index =
            (state.current_station_index + 1) % state.stations.len();
        notify_station_change(&mut state, &self.station_tx).await;
    }

    async fn previous(&self) {
        let mut state = self.state.write().await;
        if state.stations.is_empty() {
            return;
        }
        let n = state.stations.len();
        state.current_station_index = state.current_station_index.checked_sub(1).unwrap_or(n - 1);
        notify_station_change(&mut state, &self.station_tx).await;
    }

    #[allow(unused_variables)]
    async fn seek(&self, offset: i64) {
        // no-op: CanSeek is false
    }

    #[allow(unused_variables)]
    async fn set_position(
        &self,
        track_id: ObjectPath<'_>,
        position: i64,
    ) {
        // no-op: CanSeek is false
    }

    async fn open_uri(&self, uri: String) {
        let mut state = self.state.write().await;
        if let Some(idx) = state.stations.iter().position(|s| s.uri == uri) {
            state.current_station_index = idx;
            notify_station_change(&mut state, &self.station_tx).await;
        } else {
            tracing::warn!("mpris open_uri: station not found for '{}'", uri);
        }
    }

    // --- signals (Seeked declared but never emitted) ---
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

async fn notify_station_change(
    state: &mut MprisState,
    station_tx: &tokio::sync::mpsc::Sender<StationUri>,
) {
    let station = &state.stations[state.current_station_index];
    state.current_station_uri.clone_from(&station.uri);

    let _ = state.station_changed_tx.send(station.uri.clone());

    match StationUri::from_str(&station.uri) {
        Ok(station_uri) => {
            tracing::info!("mpris station change: {}", station_uri);
            let _ = station_tx.send(station_uri).await;
        }
        Err(e) => {
            tracing::error!("mpris: failed to parse station URI '{}': {}", station.uri, e);
        }
    }
}

fn make_track_id(station_uri: &str, track_id: &str) -> String {
    let station_part =
        match StationUri::from_str(station_uri) {
            Ok(StationUri::Orbox { country, alias }) => format!("{}_{}", country, alias),
            _ => "unknown".to_string(),
        };
    let ts = if track_id.is_empty() {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string()
    } else {
        track_id.to_string()
    };
    format!(
        "/org/mpris/MediaPlayer2/radio_devil/track/{}_{}",
        station_part, ts
    )
}

fn make_metadata_dict(meta: &Metadata, station_uri: &str) -> HashMap<String, Value<'static>> {
    let mut map: HashMap<String, Value<'static>> = HashMap::new();

    // mpris:trackid
    let track_id_path = make_track_id(station_uri, &meta.track_id);
    let obj_path = ObjectPath::try_from(track_id_path)
        .unwrap_or_else(|_| {
            ObjectPath::from_string_unchecked(
                "/org/mpris/MediaPlayer2/radio_devil/track/unknown".to_string(),
            )
        });
    map.insert("mpris:trackid".to_string(), Value::ObjectPath(obj_path));

    // xesam:title
    map.insert("xesam:title".to_string(), Value::from(meta.title.clone()));

    // xesam:artist
    let artists: Vec<String> = if meta.artist.is_empty() {
        vec![]
    } else {
        vec![meta.artist.clone()]
    };
    map.insert("xesam:artist".to_string(), Value::from(artists));

    // mpris:artUrl
    if !meta.art_url.is_empty() {
        map.insert("mpris:artUrl".to_string(), Value::from(meta.art_url.clone()));
    }

    // xesam:album
    if let Some(ref album) = meta.album {
        if !album.is_empty() {
            map.insert("xesam:album".to_string(), Value::from(album.clone()));
        }
    }

    map
}

// ---------------------------------------------------------------------------
// public entrypoint
// ---------------------------------------------------------------------------

pub async fn run_mpris(
    cmd_tx: std::sync::mpsc::Sender<PlayerCommand>,
    event_tx: broadcast::Sender<PlayerEvent>,
    metadata_tx: watch::Sender<Metadata>,
    station_tx: tokio::sync::mpsc::Sender<StationUri>,
    stations: Vec<StationConfig>,
    initial_volume: f64,
    initial_station_uri: Option<String>,
    quit_tx: watch::Sender<bool>,
    mut config_reload_rx: tokio::sync::mpsc::Receiver<anyhow::Result<crate::config::Config>>,
) -> anyhow::Result<()> {
    let mut quit_rx = quit_tx.subscribe();
    let quit_tx = Arc::new(quit_tx);

    let (station_changed_tx, mut station_changed_rx) = broadcast::channel::<String>(16);

    let initial_current_station_uri = initial_station_uri.clone().unwrap_or_default();
    let initial_playback = initial_station_uri
        .as_ref()
        .map(|_| PlaybackState::Playing)
        .unwrap_or(PlaybackState::Stopped);

    // find initial station index
    let initial_station_index = initial_station_uri
        .as_ref()
        .and_then(|uri| stations.iter().position(|s| &s.uri == uri))
        .unwrap_or(0);

    let state = Arc::new(RwLock::new(MprisState {
        playback_status: initial_playback,
        volume: initial_volume,
        metadata: Metadata::default(),
        stations,
        current_station_index: initial_station_index,
        current_station_uri: initial_current_station_uri,
        station_changed_tx,
    }));

    let media_player2 = MediaPlayer2 {
        quit_tx: quit_tx.clone(),
    };

    let station_tx_for_reload = station_tx.clone();
    let cmd_tx_for_reload = cmd_tx.clone();
    let station_tx_for_player = station_tx.clone();
    let media_player2_player = MediaPlayer2Player {
        state: state.clone(),
        cmd_tx,
        station_tx: station_tx_for_player,
    };

    let control = crate::control::Control {
        state: state.clone(),
        station_tx,
    };

    let conn = connection::Builder::session()?
        .name("org.mpris.MediaPlayer2.radio_devil")?
        .serve_at("/org/mpris/MediaPlayer2", media_player2)?
        .serve_at("/org/mpris/MediaPlayer2", media_player2_player)?
        .serve_at("/org/mpris/MediaPlayer2", control)?
        .build()
        .await?;

    let object_server = conn.object_server();

    let mut event_rx = event_tx.subscribe();
    let mut metadata_rx = metadata_tx.subscribe();

    loop {
        tokio::select! {
            _ = quit_rx.changed() => {
                tracing::info!("mpris quit requested");
                break;
            }

            event = event_rx.recv() => {
                match event {
                    Ok(PlayerEvent::StateChanged(new_state)) => {
                        state.write().await.playback_status = new_state;
                        emit_player_prop_changed(&object_server, "PlaybackStatus").await;
                    }
                    Ok(PlayerEvent::VolumeChanged(vol)) => {
                        state.write().await.volume = vol;
                        emit_player_prop_changed(&object_server, "Volume").await;
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("mpris event receiver lagged by {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }

            _ = metadata_rx.changed() => {
                let meta = metadata_rx.borrow_and_update().clone();
                state.write().await.metadata = meta;
                emit_player_prop_changed(&object_server, "Metadata").await;
            }

            event = station_changed_rx.recv() => {
                match event {
                    Ok(new_uri) => {
                        let _: Result<(), zbus::Error> = conn.emit_signal(
                            None::<&str>,
                            "/org/mpris/MediaPlayer2",
                            "org.mpris.MediaPlayer2.radio_devil.Control",
                            "StationChanged",
                            &(new_uri,),
                        ).await;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("station changed receiver lagged by {} messages", n);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }

            result = config_reload_rx.recv() => {
                match result {
                    Some(Err(e)) => {
                        tracing::error!("config reload failed: {:#}", e);
                    }
                    Some(Ok(new_config)) => {
                        let switch_to: Option<StationUri> = {
                            let mut state = state.write().await;
                            let old_uri = state.current_station_uri.clone();
                            state.stations = new_config.stations;
                            state.volume = new_config.daemon.volume;

                            let still_exists = state.stations.iter().any(|s| s.uri == old_uri);
                            if still_exists {
                                if let Some(idx) = state.stations.iter().position(|s| s.uri == old_uri) {
                                    state.current_station_index = idx;
                                }
                                None
                            } else if state.stations.is_empty() {
                                state.current_station_uri.clear();
                                state.current_station_index = 0;
                                drop(state);
                                let _ = cmd_tx_for_reload.send(PlayerCommand::Stop);
                                tracing::warn!(
                                    "config reload: current station removed, stopping playback"
                                );
                                None
                            } else {
                                let new_uri = state.stations[0].uri.clone();
                                state.current_station_index = 0;
                                state.current_station_uri.clone_from(&new_uri);
                                let _ = state.station_changed_tx.send(new_uri.clone());
                                StationUri::from_str(&new_uri).ok()
                            }
                        };

                        // Update volume regardless of station changes
                        let _ = cmd_tx_for_reload.send(PlayerCommand::SetVolume(
                            new_config.daemon.volume,
                        ));

                        // Switch station if needed (must happen outside lock for async .send)
                        if let Some(uri) = switch_to {
                            let _ = station_tx_for_reload.send(uri).await;
                        }

                        // Emit D-Bus PropertiesChanged for Stations
                        emit_control_prop_changed(&object_server, "Stations").await;

                        let count = state.read().await.stations.len();
                        tracing::info!("config reloaded successfully ({} stations)", count);
                    }
                    None => {
                        // Channel closed, watcher exited
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Emit a PropertiesChanged signal for the named property on the Player
/// interface. The property value is read via its getter.
async fn emit_player_prop_changed(
    object_server: &impl std::ops::Deref<Target = zbus::object_server::ObjectServer>,
    property: &str,
) {
    let Ok(iface_ref) = object_server
        .interface::<_, MediaPlayer2Player>("/org/mpris/MediaPlayer2")
        .await
    else {
        return;
    };
    let emitter = iface_ref.signal_context();

    let _ = match property {
        "PlaybackStatus" => iface_ref.get().await.playback_status_changed(emitter).await,
        "Volume" => iface_ref.get().await.volume_changed(emitter).await,
        "Metadata" => iface_ref.get_mut().await.metadata_changed(emitter).await,
        _ => Ok(()),
    };
}

/// Emit a PropertiesChanged signal for the named property on the Control
/// interface.
async fn emit_control_prop_changed(
    object_server: &impl std::ops::Deref<Target = zbus::object_server::ObjectServer>,
    property: &str,
) {
    let Ok(iface_ref) = object_server
        .interface::<_, crate::control::Control>("/org/mpris/MediaPlayer2")
        .await
    else {
        return;
    };
    let emitter = iface_ref.signal_context();
    let _ = match property {
        "Stations" => iface_ref.get().await.stations_changed(emitter).await,
        _ => Ok(()),
    };
}
