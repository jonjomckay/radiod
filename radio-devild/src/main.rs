use std::str::FromStr;

use tracing_subscriber::EnvFilter;

mod config;
mod player;
mod station;

use player::{PlayerCommand, PlayerEvent};

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

    let initial_uri = cfg
        .daemon
        .default_station
        .as_ref()
        .and_then(|uri_str| {
            let station = cfg.stations.iter().find(|s| s.uri == *uri_str)?;
            station::StationUri::from_str(&station.uri).ok().map(|u| u.to_string())
        });

    if let Some(ref uri) = initial_uri {
        tracing::info!("starting with default station: {}", uri);
    }

    let player_handle = player::run_player(cmd_rx, event_tx, initial_uri, cfg.daemon.volume);

    loop {
        tokio::select! {
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

    let _ = cmd_tx.send(PlayerCommand::Quit);
    player_handle.join().ok();
    tracing::info!("shutdown complete");
    Ok(())
}
