use std::str::FromStr;
use std::time::Duration;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU32, Ordering}};

use anyhow::Context;

use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

mod config;
mod config_watcher;
mod control;
mod metadata;
mod orbox;
mod player;
mod station;
mod mpris;

use player::{PlayerCommand, PlayerEvent, PlaybackState};
use station::StationUri;

async fn retry_playback(
    station: Option<String>,
    retry_cnt: Arc<AtomicU32>,
    retrying: Arc<AtomicBool>,
    cmd_tx: std::sync::mpsc::Sender<PlayerCommand>,
    reason: String,
) {
    let count = retry_cnt.fetch_add(1, Ordering::Relaxed);
    let max_retries: u32 = 10;
    if count >= max_retries {
        tracing::error!("{}: max retries ({}) reached, giving up", reason, max_retries);
        retrying.store(false, Ordering::Release);
        return;
    }
    let delay = std::cmp::min(
        Duration::from_secs(2_u64.saturating_pow(count)),
        Duration::from_secs(60),
    );
    tracing::info!("{}: retrying in {:?} (attempt {}/{})", reason, delay, count + 1, max_retries);
    tokio::time::sleep(delay).await;

    if let Some(ref uri_str) = station {
        if let Ok(station_uri) = StationUri::from_str(uri_str) {
            let stream_url = match &station_uri {
                StationUri::Orbox { country, alias } => {
                    match orbox::resolve_stream_url(country, alias).await {
                        Ok(url) => {
                            tracing::info!("re-resolved stream URL for {}/{}", country, alias);
                            Some(url)
                        }
                        Err(e) => {
                            tracing::error!("retry resolve failed for {}/{}: {}", country, alias, e);
                            None
                        }
                    }
                }
                StationUri::Direct { url } => Some(url.clone()),
            };
            if let Some(url) = stream_url {
                let _ = cmd_tx.send(PlayerCommand::SetUri(url));
            }
        }
    }
    retrying.store(false, Ordering::Release);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = config::load_config()?;

    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<PlayerCommand>();
    let (event_tx, mut event_rx) = tokio::sync::broadcast::channel::<PlayerEvent>(32);

    let (station_tx, mut station_rx) = tokio::sync::mpsc::channel::<StationUri>(8);

    let (metadata_tx, _metadata_rx) = watch::channel(metadata::Metadata::default());

    let poll_interval = Duration::from_secs(cfg.daemon.metadata_poll_interval_secs);

    let initial_uri_str = cfg
        .daemon
        .default_station
        .as_ref()
        .and_then(|uri_str| {
            let station = cfg.stations.iter().find(|s| s.uri == *uri_str)?;
            StationUri::from_str(&station.uri).ok().map(|u| u.to_string())
        });

    let mut poll_handle: Option<tokio::task::JoinHandle<()>> = None;

    let resolved_initial_uri = if let Some(ref uri_str) = initial_uri_str {
        let station_uri = StationUri::from_str(uri_str)?;
        match &station_uri {
            StationUri::Orbox { country, alias } => {
                tracing::info!(
                    "resolving stream URL for orbox:{}/{}",
                    country,
                    alias
                );
                match orbox::resolve_stream_url(country, alias).await {
                    Ok(stream_url) => {
                        tracing::info!(
                            "starting metadata poller for {}/{} (interval {:?})",
                            country,
                            alias,
                            poll_interval
                        );
                        let h = metadata::spawn_metadata_poller(
                            country.clone(),
                            alias.clone(),
                            poll_interval,
                            metadata_tx.clone(),
                        );
                        poll_handle = Some(h);
                        Some(stream_url)
                    }
                    Err(e) => {
                        tracing::error!(
                            "failed to resolve initial station orbox:{}/{}: {}",
                            country,
                            alias,
                            e
                        );
                        None
                    }
                }
            }
            StationUri::Direct { url } => {
                tracing::info!("playing direct stream: {}", url);
                Some(url.clone())
            }
        }
    } else {
        None
    };

    let event_tx_for_mpris = event_tx.clone();
    let current_station_str = Arc::new(Mutex::new(initial_uri_str.clone()));
    let retry_count = Arc::new(AtomicU32::new(0));
    let is_retrying = Arc::new(AtomicBool::new(false));

    let player_handle = player::run_player(
        cmd_rx,
        event_tx,
        resolved_initial_uri,
        cfg.daemon.volume,
    );

    let (quit_tx, mut quit_rx) = watch::channel(false);

    let config_reload_rx = {
        let path = config::config_path().context("could not determine config path")?;
        config_watcher::spawn(path)
    };

    let mut mpris_handle = {
        let cmd_tx = cmd_tx.clone();
        let event_tx = event_tx_for_mpris;
        let metadata_tx = metadata_tx.clone();
        let station_tx = station_tx;
        let stations = cfg.stations.clone();
        let initial_volume = cfg.daemon.volume;
        let initial_uri = initial_uri_str.clone();
        tokio::spawn(async move {
            if let Err(e) = mpris::run_mpris(
                cmd_tx,
                event_tx,
                metadata_tx,
                station_tx,
                stations,
                initial_volume,
                initial_uri,
                quit_tx,
                config_reload_rx,
            )
            .await
            {
                tracing::warn!("mpris error: {}", e);
            }
        })
    };

    loop {
        tokio::select! {
            Some(station_uri) = station_rx.recv() => {
                tracing::info!("station change requested: {}", station_uri);
                *current_station_str.lock().unwrap() = Some(station_uri.to_string());
                retry_count.store(0, Ordering::Relaxed);
                is_retrying.store(false, Ordering::Release);

                if let Some(handle) = poll_handle.take() {
                    handle.abort();
                }

                match &station_uri {
                    StationUri::Orbox { country, alias } => {
                        match orbox::resolve_stream_url(country, alias).await {
                            Ok(stream_url) => {
                                let _ = cmd_tx.send(PlayerCommand::SetUri(stream_url));
                                let h = metadata::spawn_metadata_poller(
                                    country.clone(),
                                    alias.clone(),
                                    poll_interval,
                                    metadata_tx.clone(),
                                );
                                poll_handle = Some(h);
                            }
                            Err(e) => {
                                tracing::error!(
                                    "failed to resolve station orbox:{}/{}: {}",
                                    country,
                                    alias,
                                    e
                                );
                                // Send stop to player so it doesn't keep playing old stream
                                let _ = cmd_tx.send(PlayerCommand::Stop);
                            }
                        }
                    }
                    StationUri::Direct { url } => {
                        let _ = cmd_tx.send(PlayerCommand::SetUri(url.clone()));
                    }
                }
            }
            event = event_rx.recv() => {
                match event {
                    Ok(PlayerEvent::StateChanged(state)) => {
                        tracing::info!("state: {:?}", state);
                        if state == PlaybackState::Playing {
                            retry_count.store(0, Ordering::Relaxed);
                            is_retrying.store(false, Ordering::Release);
                        }
                    }
                    Ok(PlayerEvent::Error(msg)) => {
                        tracing::error!("player error: {}", msg);
                        if !is_retrying.swap(true, Ordering::AcqRel) {
                            let station = current_station_str.lock().unwrap().clone();
                            let cmd_tx = cmd_tx.clone();
                            let retry_cnt = retry_count.clone();
                            let retrying = is_retrying.clone();
                            tokio::spawn(retry_playback(station, retry_cnt, retrying, cmd_tx, format!("error: {}", msg)));
                        }
                    }
                    Ok(PlayerEvent::EndOfStream) => {
                        tracing::info!("end of stream");
                        if !is_retrying.swap(true, Ordering::AcqRel) {
                            let station = current_station_str.lock().unwrap().clone();
                            let cmd_tx = cmd_tx.clone();
                            let retry_cnt = retry_count.clone();
                            let retrying = is_retrying.clone();
                            tokio::spawn(retry_playback(station, retry_cnt, retrying, cmd_tx, "end of stream".into()));
                        }
                    }
                    Ok(PlayerEvent::VolumeChanged(vol)) => {
                        tracing::info!("volume: {:.2}", vol);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("event receiver lagged by {} messages", n);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            _ = quit_rx.changed() => {
                tracing::info!("received quit from MPRIS");
                break;
            }
            _ = &mut mpris_handle => {
                tracing::debug!("mpris task exited");
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received Ctrl+C, shutting down...");
                break;
            }
        }
    }

    if let Some(handle) = poll_handle.take() {
        handle.abort();
    }
    let _ = cmd_tx.send(PlayerCommand::Quit);
    player_handle.join().ok();
    tracing::info!("shutdown complete");
    Ok(())
}
