use std::thread::JoinHandle;

use libmpv2::events::{Event, PropertyData};
use libmpv2::{Format, Mpv};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PlayerCommand {
    Play,
    Pause,
    Stop,
    SetVolume(f64),
    SetUri(String),
    Quit,
}

#[derive(Debug, Clone)]
pub enum PlayerEvent {
    StateChanged(PlaybackState),
    Error(String),
    EndOfStream,
    VolumeChanged(f64),
}

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) radiod/1.0";

pub fn run_player(
    cmd_rx: std::sync::mpsc::Receiver<PlayerCommand>,
    event_tx: tokio::sync::broadcast::Sender<PlayerEvent>,
    initial_uri: Option<String>,
    initial_volume: f64,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mpv = match Mpv::with_initializer(|init| {
            init.set_property("vo", "null")
                .inspect_err(|e| tracing::error!("mpv init: vo=null failed ({e})"))?;
            init.set_option("config", "no")
                .inspect_err(|e| tracing::error!("mpv init: config=no failed ({e})"))?;
            init.set_option("user-agent", USER_AGENT)
                .inspect_err(|e| tracing::error!("mpv init: user-agent failed ({e})"))?;
            init.set_option("quiet", true)
                .inspect_err(|e| tracing::error!("mpv init: quiet failed ({e})"))?;
            Ok(())
        }) {
            Ok(m) => m,
            Err(e) => {
                let _ = event_tx.send(PlayerEvent::Error(format!("failed to init mpv: {}", e)));
                return;
            }
        };

        let _ = mpv.set_property("volume", (initial_volume * 100.0) as i64);

        let client = match mpv.create_client(None) {
            Ok(c) => c,
            Err(e) => {
                let _ = event_tx.send(PlayerEvent::Error(format!(
                    "failed to create mpv client: {}",
                    e
                )));
                return;
            }
        };
        let _ = client.disable_deprecated_events();

        let _ = client.observe_property("volume", Format::Int64, 0);
        let _ = client.observe_property("pause", Format::Flag, 1);

        if let Some(ref uri) = initial_uri {
            tracing::info!("setting initial URI: {}", uri);
            if let Err(e) = mpv.command("loadfile", &[uri.as_str(), "replace"]) {
                let _ = event_tx.send(PlayerEvent::Error(format!("failed to load {}: {}", uri, e)));
            }
        }

        loop {
            let mut should_quit = false;
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    PlayerCommand::Play => {
                        let _ = mpv.set_property("pause", false);
                    }
                    PlayerCommand::Pause => {
                        let _ = mpv.set_property("pause", true);
                    }
                    PlayerCommand::Stop => {
                        let _ = mpv.command("stop", &[]);
                    }
                    PlayerCommand::SetVolume(vol) => {
                        let _ = mpv.set_property("volume", (vol * 100.0) as i64);
                    }
                    PlayerCommand::SetUri(uri) => {
                        tracing::info!("setting URI: {}", uri);
                        let _ = mpv.command("loadfile", &[uri.as_str(), "replace"]);
                    }
                    PlayerCommand::Quit => {
                        let _ = mpv.command("stop", &[]);
                        should_quit = true;
                    }
                }
            }
            if should_quit {
                break;
            }

            match client.wait_event(0.05) {
                Some(Ok(event)) => match event {
                    Event::StartFile => {
                        let _ = event_tx.send(PlayerEvent::StateChanged(PlaybackState::Playing));
                    }
                    Event::EndFile(0) => {
                        let _ = event_tx.send(PlayerEvent::EndOfStream);
                    }
                    Event::EndFile(1) => {
                        let _ = event_tx.send(PlayerEvent::StateChanged(PlaybackState::Stopped));
                    }
                    Event::EndFile(3) => {
                        let _ = event_tx.send(PlayerEvent::Error("playback error".into()));
                    }
                    Event::EndFile(_) => {}
                    Event::PropertyChange { name, change, .. } => match name {
                        "volume" => {
                            if let PropertyData::Int64(v) = change {
                                let _ = event_tx.send(PlayerEvent::VolumeChanged(v as f64 / 100.0));
                            }
                        }
                        "pause" => match change {
                            PropertyData::Flag(true) => {
                                let _ =
                                    event_tx.send(PlayerEvent::StateChanged(PlaybackState::Paused));
                            }
                            PropertyData::Flag(false) => {
                                let _ = event_tx
                                    .send(PlayerEvent::StateChanged(PlaybackState::Playing));
                            }
                            _ => {}
                        },
                        _ => {}
                    },
                    _ => {}
                },
                Some(Err(e)) => {
                    let _ = event_tx.send(PlayerEvent::Error(format!("mpv error: {}", e)));
                }
                None => {}
            }
        }
    })
}
