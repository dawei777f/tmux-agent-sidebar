use std::collections::HashMap;
use std::time::Instant;

use crate::tmux;

use super::filter::{RepoFilter, StatusFilter};

/// State shared across all sidebar instances via rmux global variables.
/// Synced from rmux at startup and on pane focus change (SIGUSR1).
pub struct GlobalState {
    pub status_filter: StatusFilter,
    pub selected_pane_row: usize,
    pub repo_filter: RepoFilter,
    /// Last filter value successfully written to rmux.
    last_saved_filter: StatusFilter,
    /// Last cursor value successfully written to rmux.
    last_saved_cursor: usize,
    /// Last repo filter value successfully written to rmux.
    last_saved_repo_filter: RepoFilter,
    /// When the selected cursor was last changed and still needs persisting.
    pending_cursor_save_since: Option<Instant>,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalState {
    pub fn new() -> Self {
        Self {
            status_filter: StatusFilter::All,
            selected_pane_row: 0,
            repo_filter: RepoFilter::All,
            last_saved_filter: StatusFilter::All,
            last_saved_cursor: 0,
            last_saved_repo_filter: RepoFilter::All,
            pending_cursor_save_since: None,
        }
    }

    /// Save filter to rmux global variable.
    /// Only updates `last_saved_filter` on success so that a failed write
    /// does not cause sync to overwrite the user's choice.
    pub fn save_filter(&mut self) {
        if tmux::set_global_option(tmux::SIDEBAR_FILTER, self.status_filter.as_str()).is_ok() {
            self.last_saved_filter = self.status_filter;
        }
    }

    /// Save cursor position to rmux global variable. Returns `true` when
    /// rmux accepted the write so callers can decide whether to clear a
    /// queued save or keep retrying.
    pub fn save_cursor(&mut self) -> bool {
        if tmux::set_global_option(tmux::SIDEBAR_CURSOR, &self.selected_pane_row.to_string())
            .is_ok()
        {
            self.last_saved_cursor = self.selected_pane_row;
            true
        } else {
            false
        }
    }

    /// Mark the cursor as dirty so the main loop can persist it once the
    /// user pauses navigation.
    pub fn queue_cursor_save(&mut self) {
        self.pending_cursor_save_since = Some(Instant::now());
    }

    /// Persist a queued cursor update after it has been idle for at least the
    /// requested debounce duration. Returns true when the queue was consumed.
    pub fn flush_pending_cursor_save(&mut self, debounce: std::time::Duration) -> bool {
        let Some(queued_at) = self.pending_cursor_save_since else {
            return false;
        };
        if queued_at.elapsed() < debounce {
            return false;
        }
        // Only clear the pending marker on successful rmux write — otherwise
        // a transient failure would silently drop the queued save instead of
        // retrying on the next flush tick.
        if self.save_cursor() {
            self.pending_cursor_save_since = None;
            true
        } else {
            false
        }
    }

    /// Save repo filter to rmux global variable.
    pub fn save_repo_filter(&mut self) {
        if tmux::set_global_option(tmux::SIDEBAR_REPO_FILTER, self.repo_filter.as_str()).is_ok() {
            self.last_saved_repo_filter = self.repo_filter.clone();
        }
    }

    /// Load all global state from rmux variables.
    /// Called at startup and on SIGUSR1 (pane focus change).
    pub fn load_from_tmux(&mut self) {
        let opts = tmux::get_all_global_options();
        self.apply_all(&opts);
    }

    /// Apply all global options from rmux (filter, cursor, repo filter).
    pub fn apply_all(&mut self, opts: &HashMap<String, String>) {
        if let Some(filter_str) = opts.get(tmux::SIDEBAR_FILTER) {
            let rmux_filter = StatusFilter::from_label(filter_str);
            if rmux_filter != self.last_saved_filter {
                self.status_filter = rmux_filter;
                self.last_saved_filter = rmux_filter;
            }
        }
        if let Some(cursor_str) = opts.get(tmux::SIDEBAR_CURSOR)
            && let Ok(n) = cursor_str.parse::<usize>()
            && n != self.last_saved_cursor
        {
            self.selected_pane_row = n;
            self.last_saved_cursor = n;
        }
        if let Some(repo_str) = opts.get(tmux::SIDEBAR_REPO_FILTER) {
            let rmux_repo = RepoFilter::from_label(repo_str);
            if rmux_repo != self.last_saved_repo_filter {
                self.repo_filter = rmux_repo.clone();
                self.last_saved_repo_filter = rmux_repo;
            }
        }
    }
}
