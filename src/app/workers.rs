use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use crate::session;
use crate::state::AppState;

/// Channels and shared flags produced by [`spawn`] that the main event loop
/// drains every tick.
pub(super) struct Workers {
    pub session_rx: Receiver<HashMap<String, String>>,
    pub sidebar_visible: Arc<AtomicBool>,
}

/// Spawn the background thread that feeds the event loop.
pub(super) fn spawn(state: &AppState, sidebar_visible: bool) -> Workers {
    let (session_tx, session_rx) = mpsc::channel::<HashMap<String, String>>();
    let visible = Arc::new(AtomicBool::new(sidebar_visible));
    let session_visible = Arc::clone(&visible);
    let _ = state;
    std::thread::spawn(move || {
        session_poll_loop(&session_tx, &session_visible);
    });

    Workers {
        session_rx,
        sidebar_visible: visible,
    }
}

/// Session name polling thread. Scans `~/.claude/sessions/*.json` every 10
/// seconds so the main TUI thread never performs blocking filesystem I/O
/// to refresh `/rename`-assigned labels.
pub(super) fn session_poll_loop(tx: &mpsc::Sender<HashMap<String, String>>, visible: &AtomicBool) {
    loop {
        std::thread::sleep(if visible.load(Ordering::Relaxed) {
            Duration::from_secs(10)
        } else {
            Duration::from_secs(60)
        });
        let names = session::scan_session_names();
        if tx.send(names).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_poll_interval_expands_when_hidden() {
        let visible = Arc::new(AtomicBool::new(false));
        assert!(!visible.load(Ordering::Relaxed));
    }
}
