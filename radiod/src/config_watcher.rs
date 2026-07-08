use std::path::Path;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use tokio::sync::mpsc;

use crate::config::{self, Config};

/// Spawns a debounced file watcher for the config file.
///
/// Watches the **parent directory** (to handle atomic-save editors that write
/// to a temp file and rename) and debounces events with a 500ms window. On each
/// debounced change, attempts to reload the config and sends the result through
/// the returned channel.
///
/// If the watcher cannot be created, an error is logged and a closed channel is
/// returned (the daemon continues without auto-reload).
pub fn spawn(path: impl AsRef<Path>) -> mpsc::Receiver<anyhow::Result<Config>> {
    let path = path.as_ref().to_owned();
    let (tx, rx) = mpsc::channel::<anyhow::Result<Config>>(1);

    let parent = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| path.clone());
    let filename = path.file_name().map(|n| n.to_os_string());

    let (debounce_tx, debounce_rx) = std::sync::mpsc::channel();
    let mut debouncer = match new_debouncer(Duration::from_millis(200), debounce_tx) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("failed to create config watcher: {:#}", e);
            return rx;
        }
    };

    if let Err(e) = debouncer
        .watcher()
        .watch(&parent, RecursiveMode::NonRecursive)
    {
        tracing::error!(
            "failed to watch config directory {}: {:#}",
            parent.display(),
            e
        );
    }

    // Process debounced events on a dedicated thread.
    // The debouncer must stay alive to keep the watcher active, so we hold it
    // in this thread.
    std::thread::spawn(move || {
        let _debouncer = debouncer;

        let mut last_mtime = None;

        for res in debounce_rx {
            match res {
                Ok(events) => {
                    let relevant = filename.as_ref().is_none_or(|fname| {
                        events
                            .iter()
                            .any(|e| e.path.file_name() == Some(fname.as_os_str()))
                    });
                    if relevant {
                        if let Ok(meta) = std::fs::metadata(&path) {
                            if let Ok(mtime) = meta.modified() {
                                if last_mtime.is_some_and(|last| mtime <= last) {
                                    continue;
                                }
                                last_mtime = Some(mtime);
                            }
                        }
                        let _ = tx.blocking_send(config::load_config());
                    }
                }
                Err(e) => {
                    tracing::error!("config watcher error: {:#}", e);
                }
            }
        }
    });

    rx
}
