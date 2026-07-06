use std::thread::JoinHandle;

use gstreamer::prelude::*;

/// Returns a flags value for playbin's `flags` property set to audio-only.
///
/// `set_property` with a plain integer creates a `guint` GLib value, but
/// playbin's `flags` property expects the `GstPlayFlags` GLib flags type.
/// `glib::FlagsValue` provides the correct type ID so GObject property
/// validation passes.
fn playbin_flags_audio_only() -> gstreamer::glib::Value {
    use gstreamer::glib::{self, translate::ToGlibPtrMut};

    static FLAGS_TYPE: std::sync::OnceLock<glib::Type> = std::sync::OnceLock::new();
    let flags_type = *FLAGS_TYPE.get_or_init(|| {
        glib::Type::from_name("GstPlayFlags")
            .expect("GstPlayFlags type not registered; is gst-plugins-base installed?")
    });

    let mut value = glib::Value::from_type(flags_type);
    unsafe {
        glib::gobject_ffi::g_value_set_flags(value.to_glib_none_mut().0, 0x02);
    }
    value
}

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

        if let Err(e) = gstreamer::init() {
            let _ = event_tx.send(PlayerEvent::Error(format!(
                "failed to initialise GStreamer: {}",
                e
            )));
            return;
        }

        let pipeline = require_element!("playbin", "playbin");

        pipeline.set_property("flags", playbin_flags_audio_only());

        pipeline.connect("source-setup", false, move |args| {
            if args.len() > 1 {
                if let Ok(source) = args[1].get::<gstreamer::Element>() {
                    source.set_property("user-agent", "Mozilla/5.0 (X11; Linux x86_64) radiod/1.0");
                }
            }
            None
        });

        let bus = require_some!(pipeline.bus(), "failed to get pipeline bus");
        let event_tx_bus = event_tx.clone();
        let event_tx_bus_inner = event_tx_bus.clone();
        let pipeline_ptr = pipeline.as_ptr() as usize;
        let pipeline_for_clock = pipeline.clone();
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
                MessageView::Buffering(buffering) => {
                    let percent = buffering.percent();
                    if percent < 100 {
                        tracing::debug!("buffering: {}%", percent);
                    }
                }
                MessageView::ClockLost(_) => {
                    tracing::debug!("clock lost, re-syncing...");
                    let _ = pipeline_for_clock.set_state(gstreamer::State::Paused);
                    let _ = pipeline_for_clock.set_state(gstreamer::State::Playing);
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
        let main_loop_cmd = main_loop.clone();
        let event_tx_cmd = event_tx.clone();

        let forward_handle = std::thread::spawn(move || {
            while let Ok(cmd) = cmd_rx.recv() {
                let is_quit = matches!(cmd, PlayerCommand::Quit);
                let pipeline = pipeline_cmd.clone();
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
                        pipeline.set_property("volume", vol);
                        let _ = event_tx.send(PlayerEvent::VolumeChanged(vol));
                    }
                    PlayerCommand::SetUri(uri) => {
                        tracing::info!("setting URI: {}", uri);
                        let _ = pipeline.set_state(gstreamer::State::Null);
                        pipeline.set_property("uri", &uri);
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
            let pipeline = pipeline.clone();
            let event_tx = event_tx.clone();
            move || {
                pipeline.set_property("volume", initial_volume);
                let _ = event_tx.send(PlayerEvent::VolumeChanged(initial_volume));
            }
        });

        if let Some(uri) = initial_uri {
            main_context.invoke({
                let pipeline = pipeline.clone();
                move || {
                    tracing::info!("setting initial URI: {}", &uri);
                    let _ = pipeline.set_state(gstreamer::State::Null);
                    pipeline.set_property("uri", &uri);
                    let _ = pipeline.set_state(gstreamer::State::Playing);
                }
            });
        }

        main_loop.run();
        forward_handle.join().ok();
    })
}
