use std::str::FromStr;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing;
use zbus::interface;

use crate::config;
use crate::mpris::MprisState;
use crate::station::StationUri;

pub struct Control {
    pub state: Arc<RwLock<MprisState>>,
    pub station_tx: tokio::sync::mpsc::Sender<StationUri>,
}

#[interface(name = "org.mpris.MediaPlayer2.radio_devil.Control")]
impl Control {
    #[zbus(signal)]
    async fn station_changed(
        _signal_ctxt: &zbus::SignalContext<'_>,
        new_uri: &str,
    ) -> zbus::Result<()>;

    async fn set_station(&self, uri: String) {
        let (idx, changed_tx) = {
            let state = self.state.read().await;
            let idx = state.stations.iter().position(|s| s.uri == uri);
            let changed_tx = state.station_changed_tx.clone();
            (idx, changed_tx)
        };

        match idx {
            Some(idx) => {
                {
                    let mut state = self.state.write().await;
                    state.current_station_index = idx;
                    state.current_station_uri.clone_from(&uri);
                }
                let _ = changed_tx.send(uri.clone());

                match StationUri::from_str(&uri) {
                    Ok(station_uri) => {
                        tracing::info!("control set_station: {}", station_uri);
                        let _ = self.station_tx.send(station_uri).await;
                    }
                    Err(e) => {
                        tracing::error!(
                            "control set_station: failed to parse URI '{}': {}",
                            uri,
                            e
                        );
                    }
                }
            }
            None => {
                tracing::warn!("control set_station: station not found for '{}'", uri);
            }
        }
    }

    async fn get_station(&self) -> (String, String) {
        let state = self.state.read().await;
        if state.stations.is_empty() || state.current_station_index >= state.stations.len() {
            return (String::new(), String::new());
        }
        let station = &state.stations[state.current_station_index];
        (station.name.clone(), station.uri.clone())
    }

    async fn list_stations(&self) -> Vec<(String, String)> {
        self.state
            .read()
            .await
            .stations
            .iter()
            .map(|s| (s.name.clone(), s.uri.clone()))
            .collect()
    }

    async fn reload_config(&self) {
        let cfg = match config::load_config() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("control reload_config: {}", e);
                return;
            }
        };

        let (old_uri, changed_tx) = {
            let state = self.state.read().await;
            (state.current_station_uri.clone(), state.station_changed_tx.clone())
        };

        let (_new_idx_or_none, needs_switch) = {
            let mut state = self.state.write().await;
            state.stations = cfg.stations;
            let new_idx = state.stations.iter().position(|s| s.uri == old_uri);
            match new_idx {
                Some(idx) => {
                    state.current_station_index = idx;
                    (Some(idx), false)
                }
                None => {
                    if state.stations.is_empty() {
                        state.current_station_index = 0;
                        state.current_station_uri.clear();
                        (None, false)
                    } else {
                        state.current_station_index = 0;
                        let new_uri = state.stations[0].uri.clone();
                    state.current_station_uri = new_uri;
                        (None, true)
                    }
                }
            }
        };

        if needs_switch {
            let new_uri = {
                let state = self.state.read().await;
                state.current_station_uri.clone()
            };
            let _ = changed_tx.send(new_uri.clone());

            match StationUri::from_str(&new_uri) {
                Ok(station_uri) => {
                    tracing::info!("control reload: switching to {}", station_uri);
                    let _ = self.station_tx.send(station_uri).await;
                }
                Err(e) => {
                    tracing::error!(
                        "control reload: failed to parse URI '{}': {}",
                        new_uri,
                        e
                    );
                }
            }
        } else {
            tracing::info!("control reload_config: config reloaded (station unchanged)");
        }
    }

    #[zbus(property)]
    async fn current_station(&self) -> (String, String) {
        self.get_station().await
    }

    #[zbus(property)]
    async fn stations(&self) -> Vec<(String, String)> {
        self.list_stations().await
    }
}
