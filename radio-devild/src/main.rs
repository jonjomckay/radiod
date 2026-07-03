use std::str::FromStr;
use std::time::Duration;

use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

mod config;
mod metadata;
mod orbox;
mod player;
mod station;

use player::{PlayerCommand, PlayerEvent};
use station::StationUri;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = config::load_config()?;
    let data_dir = config::ensure_data_dir()?;
    tracing::info!("data directory: {}", data_dir.display());

    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<PlayerCommand>();
    let (event_tx, mut event_rx) = tokio::sync::broadcast::channel::<PlayerEvent>(32);

    let (_station_tx, mut station_rx) = tokio::sync::mpsc::channel::<StationUri>(8);

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

    let player_handle = player::run_player(
        cmd_rx,
        event_tx,
        resolved_initial_uri,
        cfg.daemon.volume,
    );

    loop {
        tokio::select! {
            Some(station_uri) = station_rx.recv() => {
                tracing::info!("station change requested: {}", station_uri);

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
                    }
                    Ok(PlayerEvent::Error(msg)) => {
                        tracing::error!("player error: {}", msg);
                    }
                    Ok(PlayerEvent::EndOfStream) => {
                        tracing::info!("end of stream");
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
