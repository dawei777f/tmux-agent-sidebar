//! Main application orchestration: prime the [`AppState`], spawn background
//! workers, and run the crossterm event loop. Split out from `src/main.rs` so
//! the binary entry point only handles CLI arg parsing, signal wiring, and
//! TUI session setup.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{self};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::SPINNER_PULSE;

mod input;
mod render;
mod setup;
mod workers;

const ACTIVE_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const HIDDEN_VISIBILITY_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const HIDDEN_FULL_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const SPINNER_INTERVAL: Duration = Duration::from_millis(200);

fn refresh_interval(window_active: bool) -> Duration {
    if window_active {
        ACTIVE_REFRESH_INTERVAL
    } else {
        HIDDEN_VISIBILITY_CHECK_INTERVAL
    }
}

fn spinner_interval(window_active: bool) -> Option<Duration> {
    window_active.then_some(SPINNER_INTERVAL)
}

fn needs_animation(state: &crate::state::AppState) -> bool {
    state.pet_enabled
        || state
            .repo_groups
            .iter()
            .flat_map(|group| group.panes.iter())
            .any(|(pane, _)| pane.status == crate::tmux::PaneStatus::Running)
}

fn next_poll_timeout(
    needs_refresh: bool,
    window_active: bool,
    animation_active: bool,
    last_refresh: std::time::Instant,
    last_spinner: std::time::Instant,
    now: std::time::Instant,
) -> Duration {
    if needs_refresh {
        return Duration::ZERO;
    }

    let refresh_timeout = refresh_interval(window_active).saturating_sub(now - last_refresh);
    if let Some(interval) = spinner_interval(window_active && animation_active) {
        refresh_timeout.min(interval.saturating_sub(now - last_spinner))
    } else {
        refresh_timeout
    }
}

fn should_run_full_refresh(
    sigusr1: bool,
    window_active: bool,
    last_full_refresh: std::time::Instant,
    now: std::time::Instant,
) -> bool {
    sigusr1
        || window_active
        || now.duration_since(last_full_refresh) >= HIDDEN_FULL_REFRESH_INTERVAL
}

/// Run the TUI event loop. Returns when the loop exits (currently only on
/// fatal I/O error, since the loop is `loop { ... }`).
///
/// `needs_refresh` is the process-wide SIGUSR1 flag owned by `main.rs` — the
/// signal handler must reference a static visible at signal-handler time,
/// so the static stays with the `extern "C"` handler in the binary crate and
/// we just borrow it here.
pub fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    tmux_pane: String,
    needs_refresh: &'static AtomicBool,
) -> io::Result<()> {
    let mut state = setup::init_state(tmux_pane);
    let mut window_inactive_count: u32 = 0;
    let initial_window_active = crate::tmux::get_sidebar_pane_info(&state.tmux_pane).1;

    let workers = workers::spawn(&state, initial_window_active);
    let workers::Workers {
        session_rx,
        version_rx,
        sidebar_visible,
    } = workers;

    let mut last_refresh = std::time::Instant::now();
    let mut last_full_refresh = last_refresh;
    let mut last_spinner = std::time::Instant::now();
    let mut window_active = initial_window_active;
    let mut needs_redraw = true;

    loop {
        if needs_redraw {
            render::render_frame(terminal, &mut state)?;
            needs_redraw = false;
        }

        let now = std::time::Instant::now();
        let timeout = next_poll_timeout(
            needs_refresh.load(Ordering::Relaxed),
            window_active,
            needs_animation(&state),
            last_refresh,
            last_spinner,
            now,
        );
        if event::poll(timeout)? {
            loop {
                let ev = event::read()?;
                if input::handle_event(ev, &mut state, terminal) {
                    needs_redraw = true;
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        if spinner_interval(window_active && needs_animation(&state))
            .is_some_and(|interval| last_spinner.elapsed() >= interval)
        {
            state.spinner_frame = (state.spinner_frame + 1) % SPINNER_PULSE.len();
            if state.pet_enabled {
                let term_width = terminal.size().map(|s| s.width).unwrap_or(60);
                state.tick_pet(term_width);
            }
            last_spinner = std::time::Instant::now();
            needs_redraw = true;
        }

        let sigusr1 = needs_refresh.swap(false, Ordering::Relaxed);
        if sigusr1 || last_refresh.elapsed() >= refresh_interval(window_active) {
            let now = std::time::Instant::now();
            let mut full_refresh =
                should_run_full_refresh(sigusr1, window_active, last_full_refresh, now);
            window_active = if full_refresh {
                let active = state.refresh();
                last_full_refresh = std::time::Instant::now();
                active
            } else {
                let active = state.refresh_visibility();
                if active {
                    full_refresh = true;
                    let active = state.refresh();
                    last_full_refresh = std::time::Instant::now();
                    active
                } else {
                    active
                }
            };
            needs_redraw = full_refresh;
            if window_active {
                if window_inactive_count >= 2 {
                    state.global.load_from_tmux();
                    state.rebuild_row_targets();
                }
                window_inactive_count = 0;
            } else {
                window_inactive_count = window_inactive_count.saturating_add(1);
            }
            sidebar_visible.store(window_active, Ordering::Relaxed);
            last_refresh = std::time::Instant::now();
        }

        if let Ok(names) = session_rx.try_recv() {
            state.sessions.names = names;
            state.sessions.dirty = true;
            needs_redraw = true;
        }

        if let Ok(notice) = version_rx.try_recv() {
            state.version_notice = Some(notice);
            needs_redraw = true;
        }

        state
            .global
            .flush_pending_cursor_save(std::time::Duration::from_millis(120));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_window_uses_interactive_cadence() {
        let now = std::time::Instant::now();

        assert_eq!(refresh_interval(true), Duration::from_secs(1));
        assert_eq!(spinner_interval(true), Some(Duration::from_millis(200)));
        assert_eq!(
            next_poll_timeout(false, true, true, now, now, now),
            Duration::from_millis(200),
            "active sidebars should sleep until the next spinner tick instead of busy-polling"
        );
    }

    #[test]
    fn active_window_without_animation_only_wakes_to_check_signals() {
        let now = std::time::Instant::now();

        assert_eq!(
            next_poll_timeout(false, true, false, now, now, now),
            Duration::from_secs(1),
            "idle active sidebars should wake on the refresh cadence"
        );
    }

    #[test]
    fn inactive_window_suppresses_animation_and_refreshes_slowly() {
        let now = std::time::Instant::now();

        assert_eq!(refresh_interval(false), Duration::from_secs(5));
        assert_eq!(spinner_interval(false), None);
        assert_eq!(
            next_poll_timeout(false, false, true, now, now, now),
            Duration::from_secs(5),
            "inactive sidebars should only wake for visibility checks"
        );
    }

    #[test]
    fn signal_refresh_wakes_immediately() {
        let now = std::time::Instant::now();

        assert_eq!(
            next_poll_timeout(true, false, false, now, now, now),
            Duration::ZERO
        );
    }

    #[test]
    fn hidden_sidebar_full_refreshes_periodically() {
        let now = std::time::Instant::now();

        assert!(!should_run_full_refresh(
            false,
            false,
            now - Duration::from_secs(29),
            now
        ));
        assert!(should_run_full_refresh(
            false,
            false,
            now - Duration::from_secs(30),
            now
        ));
    }

    #[test]
    fn visible_sidebar_full_refreshes_on_each_tick() {
        let now = std::time::Instant::now();

        assert!(should_run_full_refresh(false, true, now, now));
    }
}
