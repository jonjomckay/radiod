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
        if let Err(e) = gstreamer::init() {
            let _ = event_tx.send(PlayerEvent::Error(format!(
                "failed to initialise GStreamer: {}",
                e
            )));
            return;
        }

        let pipeline = gstreamer::Pipeline::new();

        let uridecodebin = gstreamer::ElementFactory::make("uridecodebin")
            .name("uridecodebin")
            .build()
            .expect("failed to create uridecodebin");
        let audioconvert = gstreamer::ElementFactory::make("audioconvert")
            .name("audioconvert")
            .build()
            .expect("failed to create audioconvert");
        let audioresample = gstreamer::ElementFactory::make("audioresample")
            .name("audioresample")
            .build()
            .expect("failed to create audioresample");
        let volume_elem = gstreamer::ElementFactory::make("volume")
            .name("volume")
            .build()
            .expect("failed to create volume");
        let audiosink = gstreamer::ElementFactory::make("autoaudiosink")
            .name("audiosink")
            .build()
            .expect("failed to create autoaudiosink");

        pipeline
            .add_many([
                &uridecodebin,
                &audioconvert,
                &audioresample,
                &volume_elem,
                &audiosink,
            ])
            .expect("failed to add elements to pipeline");

        gstreamer::Element::link_many([&audioconvert, &audioresample, &volume_elem, &audiosink])
            .expect("failed to link audio chain");

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

        let bus = pipeline.bus().expect("failed to get pipeline bus");
        let event_tx_bus = event_tx.clone();
        let pipeline_ptr = pipeline.as_ptr() as usize;
        let _bus_watch = bus
            .add_watch(move |_, msg| {
                use gstreamer::MessageView;
                match msg.view() {
                    MessageView::Eos(_) => {
                        let _ = event_tx_bus.send(PlayerEvent::EndOfStream);
                    }
                    MessageView::Error(err) => {
                        let _ = event_tx_bus.send(PlayerEvent::Error(format!(
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
                                let _ = event_tx_bus.send(PlayerEvent::StateChanged(state));
                            }
                        }
                    }
                    _ => {}
                }
                gstreamer::glib::ControlFlow::Continue
            })
            .expect("failed to add bus watch");

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
