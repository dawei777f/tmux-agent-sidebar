use std::collections::HashMap;

use crate::activity::{self, TaskProgress};
use crate::tmux::{self, SessionInfo};

use super::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskProgressDecision {
    Clear,
    Show,
    Dismiss { total: usize },
    Skip,
}

/// A per-pane task-progress update computed in the first pass of
/// `refresh_task_progress`, applied back to `pane_states` in the second pass.
struct PaneTaskUpdate {
    pane_id: String,
    progress: Option<TaskProgress>,
    dismissed_total: Option<usize>,
    inactive_since: Option<u64>,
    log_mtime: Option<std::time::SystemTime>,
}

pub(crate) fn classify_task_progress(
    progress: &TaskProgress,
    dismissed_total: Option<usize>,
) -> TaskProgressDecision {
    if progress.is_empty() {
        return TaskProgressDecision::Clear;
    }
    if progress.all_completed() {
        if dismissed_total == Some(progress.total()) {
            TaskProgressDecision::Skip
        } else {
            TaskProgressDecision::Dismiss {
                total: progress.total(),
            }
        }
    } else {
        TaskProgressDecision::Show
    }
}

impl AppState {
    pub(crate) fn refresh_now(&mut self) {
        self.now = crate::time::now_epoch_secs();
    }

    pub(crate) fn apply_session_snapshot(
        &mut self,
        sidebar_focused: bool,
        sessions: Vec<SessionInfo>,
    ) {
        self.focus_state.sidebar_focused = sidebar_focused;
        // Capture the prior `pane_id → session_id` map so we can detect
        // anything that should re-trigger `refresh_session_names`:
        //   - a brand-new pane_id (first appearance)
        //   - an existing pane whose session_id changed (e.g. /clear or
        //     a Codex session swap reuses the same pane_id but binds a
        //     new session label)
        let prev_session_ids: HashMap<String, Option<String>> = self
            .repo_groups
            .iter()
            .flat_map(|g| {
                g.panes
                    .iter()
                    .map(|p| (p.pane_id.clone(), p.session_id.clone()))
            })
            .collect();
        self.repo_groups = crate::group::group_panes_by_repo(&sessions);
        if !self.sessions.dirty
            && self
                .repo_groups
                .iter()
                .flat_map(|g| g.panes.iter())
                .any(|p| match prev_session_ids.get(&p.pane_id) {
                    None => true,
                    Some(prev_sid) => *prev_sid != p.session_id,
                })
        {
            self.sessions.dirty = true;
        }
        self.prune_pane_states_to_current_panes();
        self.rebuild_row_targets();
        self.find_focused_pane();
    }

    fn refresh_activity_data(&mut self) {
        self.refresh_task_progress();
    }

    /// Fast refresh: tmux state + task progress (called every 1s while visible,
    /// slower while hidden). Returns whether the sidebar's window is visible in
    /// an attached tmux session.
    pub fn refresh(&mut self) -> bool {
        self.refresh_now();
        let (focused, window_active, _, _) = tmux::get_sidebar_pane_info(&self.tmux_pane);
        let sessions = tmux::query_sessions();
        self.apply_session_snapshot(focused, sessions);
        if self.sessions.dirty {
            self.refresh_session_names();
            self.sessions.dirty = false;
        }
        self.refresh_activity_data();
        window_active
    }

    pub fn refresh_visibility(&mut self) -> bool {
        self.refresh_now();
        let (focused, window_active, _, _) = tmux::get_sidebar_pane_info(&self.tmux_pane);
        self.focus_state.sidebar_focused = focused;
        window_active
    }

    /// Apply the current `session_id → name` map to each pane so the
    /// sidebar can render `/rename`-assigned labels. The map itself is
    /// refreshed off-thread by `session_poll_loop` in `main.rs`; this
    /// function only consumes the cached snapshot.
    fn refresh_session_names(&mut self) {
        for group in &mut self.repo_groups {
            for pane in &mut group.panes {
                if let Some(sid) = &pane.session_id
                    && let Some(name) = self.sessions.names.get(sid)
                {
                    pane.session_name.clone_from(name);
                } else {
                    pane.session_name.clear();
                }
            }
        }
    }

    pub(crate) fn refresh_task_progress(&mut self) {
        let mut updates: Vec<PaneTaskUpdate> = Vec::new();
        for group in &self.repo_groups {
            for pane in &group.panes {
                let prior_state = self.pane_state(&pane.pane_id).cloned().unwrap_or_default();
                let current_mtime = activity::log_mtime(&pane.pane_id);
                // Skip the (full-file) re-parse when the activity log
                // hasn't been touched since the last tick AND the pane
                // is still active. We must still re-evaluate the
                // inactive-grace path while the agent is idle so that a
                // long-stalled progress bar gets dismissed even if the
                // log file itself stops changing.
                let agent_active = pane.status.is_active();
                let log_unchanged =
                    current_mtime.is_some() && current_mtime == prior_state.task_progress_log_mtime;
                if log_unchanged && agent_active {
                    // Just refresh the mtime bookkeeping so we don't
                    // accidentally drop the cache on a future iteration
                    // where current_mtime suddenly becomes None (e.g.
                    // /tmp clean-up). All other prior_state fields
                    // remain authoritative.
                    updates.push(PaneTaskUpdate {
                        pane_id: pane.pane_id.clone(),
                        progress: prior_state.task_progress.clone(),
                        dismissed_total: prior_state.task_dismissed_total,
                        inactive_since: None,
                        log_mtime: current_mtime,
                    });
                    continue;
                }
                // Read all entries for task progress (not limited to display max)
                // so that TaskCreate entries aren't lost when subagents flood the log
                let entries = activity::read_activity_log(&pane.pane_id, 0);
                let progress = activity::parse_task_progress(&entries);
                // Debounce inactive→dismiss transition to avoid flicker.
                //
                // The agent status can briefly drop to idle during normal operation
                // (e.g. when Claude Code processes a system prompt or between tool
                // calls). Without a grace period, the 1-second refresh cycle can
                // catch that transient idle state and immediately hide the task
                // progress bar, causing a visible flicker.
                //
                // We track when each pane first appeared inactive and only dismiss
                // after INACTIVE_GRACE_SECS have elapsed. If the agent returns to
                // Running/Waiting within that window, the timer is reset.
                const INACTIVE_GRACE_SECS: u64 = 3;

                let next_inactive_since = if !agent_active {
                    Some(prior_state.inactive_since.unwrap_or(self.now))
                } else {
                    None
                };
                let grace_expired = next_inactive_since
                    .is_some_and(|since| self.now.saturating_sub(since) >= INACTIVE_GRACE_SECS);

                let decision = if grace_expired && !progress.is_empty() && !progress.all_completed()
                {
                    TaskProgressDecision::Dismiss {
                        total: progress.total(),
                    }
                } else {
                    classify_task_progress(&progress, prior_state.task_dismissed_total)
                };
                let next_progress = match decision {
                    TaskProgressDecision::Clear => None,
                    TaskProgressDecision::Show => Some(progress),
                    TaskProgressDecision::Dismiss { .. } => None,
                    TaskProgressDecision::Skip => prior_state.task_progress.clone(),
                };
                let next_dismissed_total = match decision {
                    TaskProgressDecision::Clear | TaskProgressDecision::Show => None,
                    TaskProgressDecision::Dismiss { total } => Some(total),
                    TaskProgressDecision::Skip => prior_state.task_dismissed_total,
                };
                updates.push(PaneTaskUpdate {
                    pane_id: pane.pane_id.clone(),
                    progress: next_progress,
                    dismissed_total: next_dismissed_total,
                    inactive_since: next_inactive_since,
                    log_mtime: current_mtime,
                });
            }
        }
        for update in updates {
            let pane_state = self.pane_state_mut(&update.pane_id);
            pane_state.inactive_since = update.inactive_since;
            pane_state.task_dismissed_total = update.dismissed_total;
            pane_state.task_progress = update.progress;
            pane_state.task_progress_log_mtime = update.log_mtime;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::{AgentType, PaneInfo, PaneStatus, PermissionMode, SessionInfo, WindowInfo};

    fn test_pane(id: &str) -> PaneInfo {
        PaneInfo {
            pane_id: id.into(),
            pane_active: false,
            status: PaneStatus::Running,
            attention: false,
            agent: AgentType::Claude,
            path: "/tmp".into(),
            current_command: String::new(),
            prompt: String::new(),
            prompt_is_response: false,
            started_at: None,
            wait_reason: String::new(),
            permission_mode: PermissionMode::Default,
            subagents: vec![],
            pane_pid: None,
            session_id: None,
            session_name: String::new(),
            bg_shell_cmd: None,
        }
    }

    fn test_session(panes: Vec<PaneInfo>) -> Vec<SessionInfo> {
        vec![SessionInfo {
            session_name: "main".into(),
            windows: vec![WindowInfo {
                window_id: "@0".into(),
                window_name: "test".into(),
                window_active: true,
                auto_rename: false,
                panes,
            }],
        }]
    }

    // ─── refresh_session_names ──────────────────────────────────────
    //
    // refresh_session_names no longer scans the filesystem itself; it
    // only consumes the cached `session_names` map populated by the
    // dedicated polling thread in `main.rs`. These tests pin that
    // contract: the function must apply the cached snapshot to every
    // pane and clear stale labels for panes whose session_id is no
    // longer in the map.

    fn pane_with_session(id: &str, session_id: &str) -> PaneInfo {
        let mut p = test_pane(id);
        p.session_id = Some(session_id.to_string());
        p
    }

    fn state_with_panes(panes: Vec<PaneInfo>) -> AppState {
        let mut state = AppState::new("%99".into());
        state.repo_groups = vec![crate::group::RepoGroup {
            name: "test".into(),
            has_focus: true,
            panes: panes.into_iter().collect(),
        }];
        state
    }

    #[test]
    fn refresh_session_names_applies_cached_map_to_panes() {
        let mut state = state_with_panes(vec![
            pane_with_session("%1", "sess-a"),
            pane_with_session("%2", "sess-b"),
        ]);
        state.sessions.names.insert("sess-a".into(), "alpha".into());
        state.sessions.names.insert("sess-b".into(), "beta".into());

        state.refresh_session_names();

        let names: Vec<&str> = state.repo_groups[0]
            .panes
            .iter()
            .map(|p| p.session_name.as_str())
            .collect();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn refresh_session_names_clears_stale_label_when_session_id_missing() {
        // Pane already has a label from a previous tick, but its
        // session_id no longer appears in the cached map (e.g. the
        // session JSON file was deleted). The label must be cleared so
        // the UI does not show a name for a session that is gone.
        let mut state = state_with_panes(vec![pane_with_session("%1", "sess-gone")]);
        state.repo_groups[0].panes[0].session_name = "old-label".into();
        // session_names is empty — no entry for sess-gone.

        state.refresh_session_names();

        assert!(
            state.repo_groups[0].panes[0].session_name.is_empty(),
            "stale session_name must be cleared when the cache no longer has it"
        );
    }

    #[test]
    fn apply_session_snapshot_marks_dirty_when_existing_pane_swaps_session_id() {
        // Pane %1 keeps the same pane_id across snapshots but its
        // session_id changes (e.g. the agent restarted with a new
        // Claude session). Without dirty propagation,
        // refresh_session_names would be skipped and the UI would
        // keep showing the old session label forever.
        let mut state = state_with_panes(vec![pane_with_session("%1", "sess-old")]);
        state.sessions.dirty = false;

        let next_sessions = test_session(vec![pane_with_session("%1", "sess-new")]);
        state.apply_session_snapshot(false, next_sessions);

        assert!(
            state.sessions.dirty,
            "session_names_dirty must be set when an existing pane's session_id changes"
        );
    }

    #[test]
    fn apply_session_snapshot_does_not_mark_dirty_when_session_ids_unchanged() {
        // Same pane, same session_id across snapshots — no need to
        // re-walk every pane, dirty flag should stay clear.
        let mut state = state_with_panes(vec![pane_with_session("%1", "sess-a")]);
        state.sessions.dirty = false;

        let next_sessions = test_session(vec![pane_with_session("%1", "sess-a")]);
        state.apply_session_snapshot(false, next_sessions);

        assert!(
            !state.sessions.dirty,
            "session_names_dirty must remain clear when nothing changed"
        );
    }

    #[test]
    fn refresh_session_names_clears_label_for_pane_with_no_session_id() {
        // Pane has a session_name set but no session_id (e.g. a
        // non-Claude agent or a pane that has not reported one yet).
        // The function must not preserve a label that no longer ties
        // to a known session.
        let mut state = state_with_panes(vec![test_pane("%1")]);
        state.repo_groups[0].panes[0].session_name = "stray".into();
        state.sessions.names.insert("sess-a".into(), "alpha".into());

        state.refresh_session_names();

        assert!(
            state.repo_groups[0].panes[0].session_name.is_empty(),
            "pane without session_id must end up with an empty session_name"
        );
    }
}
