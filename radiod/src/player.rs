use std::thread::JoinHandle;

use gstreamer::prelude::*;

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

pub fn run_player(
    cmd_rx: std::sync::mpsc::Receiver<PlayerCommand>,
    event_tx: tokio::sync::broadcast::Sender<PlayerEvent>,
    initial_uri: Option<String>,
    initial_volume: f64,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        macro_rules! require_element {
            ($factory:expr, $name:expr) => {
                match gstreamer::ElementFactory::make($factory).name($name).build() {
                    Ok(elem) => elem,
                    Err(e) => {
                        let _ = event_tx.send(PlayerEvent::Error(format!(
                            "GStreamer element '{}' not found. Is the required plugin installed? ({})",
                            $factory, e
                        )));
                        return;
                    }
                }
            };
        }

        if let Err(e) = gstreamer::init() {
            let _ = event_tx.send(PlayerEvent::Error(format!(
                "failed to initialise GStreamer: {}",
                e
            )));
            return;
        }

        let pipeline = gstreamer::Pipeline::new();

        let uridecodebin = require_element!("uridecodebin", "uridecodebin");
        uridecodebin.connect("source-setup", false, move |args| {
            if args.len() > 1 {
                if let Ok(source) = args[1].get::<gstreamer::Element>() {
                    source.set_property("user-agent", "Mozilla/5.0 (X11; Linux x86_64) radiod/1.0");
                }
            }
            None
        });
        let audioconvert = require_element!("audioconvert", "audioconvert");
        let audioresample = require_element!("audioresample", "audioresample");
        let volume_elem = require_element!("volume", "volume");
        let audiosink = require_element!("autoaudiosink", "audiosink");

        macro_rules! require {
            ($result:expr, $msg:literal) => {
                match $result {
                    Ok(val) => val,
                    Err(e) => {
                        let _ = event_tx.send(PlayerEvent::Error(format!("{}: {}", $msg, e)));
                        return;
                    }
                }
            };
        }

        macro_rules! require_some {
            ($option:expr, $msg:literal) => {
                match $option {
                    Some(val) => val,
                    None => {
                        let _ = event_tx.send(PlayerEvent::Error(format!("{}", $msg)));
                        return;
                    }
                }
            };
        }

        require!(
            pipeline.add_many([
                &uridecodebin,
                &audioconvert,
                &audioresample,
                &volume_elem,
                &audiosink,
            ]),
            "failed to add elements to pipeline"
        );

        require!(
            gstreamer::Element::link_many([
                &audioconvert,
                &audioresample,
                &volume_elem,
                &audiosink
            ]),
            "failed to link audio chain"
        );

        let audioconvert_weak = audioconvert.downgrade();
        uridecodebin.connect_pad_added(move |_dbin, src_pad| {
            let caps = src_pad.current_caps();
            let Some(caps) = caps else { return };
            let Some(s) = caps.structure(0) else { return };
            if !s.name().starts_with("audio/") {
                return;
            }
            let Some(audioconvert) = audioconvert_weak.upgrade() else {
                return;
            };
            let sink_pad = audioconvert
                .static_pad("sink")
                .expect("audioconvert has no sink pad");
            if src_pad.link(&sink_pad).is_err() {
                tracing::warn!("failed to link uridecodebin pad");
            }
        });

        let bus = require_some!(pipeline.bus(), "failed to get pipeline bus");
        let event_tx_bus = event_tx.clone();
        let event_tx_bus_inner = event_tx_bus.clone();
        let pipeline_ptr = pipeline.as_ptr() as usize;
        let _bus_watch = match bus.add_watch(move |_, msg| {
            use gstreamer::MessageView;
            match msg.view() {
                MessageView::Eos(_) => {
                    let _ = event_tx_bus_inner.send(PlayerEvent::EndOfStream);
                }
                MessageView::Error(err) => {
                    let _ = event_tx_bus_inner.send(PlayerEvent::Error(format!(
                        "{}: {}",
                        err.error(),
                        err.debug().unwrap_or_default()
                    )));
                }
                MessageView::StateChanged(state) => {
                    if let Some(src) = state.src() {
                        if src.as_ptr() as usize == pipeline_ptr {
                            let state = match state.current() {
                                gstreamer::State::Playing => PlaybackState::Playing,
                                gstreamer::State::Paused => PlaybackState::Paused,
                                gstreamer::State::Null => PlaybackState::Stopped,
                                _ => return gstreamer::glib::ControlFlow::Continue,
                            };
                            let _ = event_tx_bus_inner.send(PlayerEvent::StateChanged(state));
                        }
                    }
                }
                _ => {}
            }
            gstreamer::glib::ControlFlow::Continue
        }) {
            Ok(watch) => watch,
            Err(e) => {
                let _ = event_tx_bus.send(PlayerEvent::Error(format!(
                    "failed to add bus watch: {}",
                    e
                )));
                return;
            }
        };

        let main_loop = gstreamer::glib::MainLoop::new(None, false);
        let main_context = main_loop.context();
        let main_context_cmd = main_context.clone();

        let pipeline_cmd = pipeline.clone();
        let uridecodebin_cmd = uridecodebin.clone();
        let volume_cmd = volume_elem.clone();
        let main_loop_cmd = main_loop.clone();
        let event_tx_cmd = event_tx.clone();

        let forward_handle = std::thread::spawn(move || {
            while let Ok(cmd) = cmd_rx.recv() {
                let is_quit = matches!(cmd, PlayerCommand::Quit);
                let pipeline = pipeline_cmd.clone();
                let uridecodebin = uridecodebin_cmd.clone();
                let volume = volume_cmd.clone();
                let event_tx = event_tx_cmd.clone();
                let main_loop = main_loop_cmd.clone();

                main_context_cmd.invoke(move || match cmd {
                    PlayerCommand::Play => {
                        let _ = pipeline.set_state(gstreamer::State::Playing);
                    }
                    PlayerCommand::Pause => {
                        let _ = pipeline.set_state(gstreamer::State::Paused);
                    }
                    PlayerCommand::Stop => {
                        let _ = pipeline.set_state(gstreamer::State::Null);
                    }
                    PlayerCommand::SetVolume(vol) => {
                        volume.set_property("volume", vol);
                        let _ = event_tx.send(PlayerEvent::VolumeChanged(vol));
                    }
                    PlayerCommand::SetUri(uri) => {
                        tracing::info!("setting URI: {}", uri);
                        let _ = pipeline.set_state(gstreamer::State::Null);
                        uridecodebin.set_property("uri", &uri);
                        let _ = pipeline.set_state(gstreamer::State::Playing);
                    }
                    PlayerCommand::Quit => {
                        let _ = pipeline.set_state(gstreamer::State::Null);
                        main_loop.quit();
                    }
                });

                if is_quit {
                    break;
                }
            }
        });

        main_context.invoke({
            let volume = volume_elem.clone();
            let event_tx = event_tx.clone();
            move || {
                volume.set_property("volume", initial_volume);
                let _ = event_tx.send(PlayerEvent::VolumeChanged(initial_volume));
            }
        });
        if let Some(uri) = initial_uri {
            main_context.invoke({
                let pipeline = pipeline.clone();
                let uridecodebin = uridecodebin.clone();
                move || {
                    tracing::info!("setting initial URI: {}", &uri);
                    let _ = pipeline.set_state(gstreamer::State::Null);
                    uridecodebin.set_property("uri", &uri);
                    let _ = pipeline.set_state(gstreamer::State::Playing);
                }
            });
        }

        main_loop.run();
        forward_handle.join().ok();
    })
}
