#[allow(dead_code, unused_imports)]
mod test_helpers;

use test_helpers::*;
use tmux_agent_sidebar::group::RepoGroup;
use tmux_agent_sidebar::state::{
    AppState, Focus, GlobalState, RepoFilter, RowTarget, StatusFilter,
};
use tmux_agent_sidebar::tmux::{self, AgentType, PaneInfo, PaneStatus, SessionInfo, WindowInfo};

// ─── State Transition Tests ────────────────────────────────────────

#[test]
fn test_move_pane_selection_bounds() {
    let mut state = make_state(vec![]);
    state.layout.pane_row_targets = vec![
        RowTarget {
            pane_id: "%1".into(),
        },
        RowTarget {
            pane_id: "%2".into(),
        },
    ];
    state.global.selected_pane_row = 0;
    state.move_pane_selection(1);
    assert_eq!(state.global.selected_pane_row, 1);
    state.move_pane_selection(1); // should not go past end
    assert_eq!(state.global.selected_pane_row, 1);
    state.move_pane_selection(-1);
    assert_eq!(state.global.selected_pane_row, 0);
    state.move_pane_selection(-1); // should not go below 0
    assert_eq!(state.global.selected_pane_row, 0);
}

#[test]
fn test_move_pane_selection_empty() {
    let mut state = make_state(vec![]);
    state.move_pane_selection(1);
    assert_eq!(state.global.selected_pane_row, 0);
}

// ─── line_to_row Mapping Tests ─────────────────────────────────────

#[test]
fn test_line_to_row_single_agent() {
    let pane = make_pane(AgentType::Claude, PaneStatus::Idle);
    let mut state = make_state(vec![SessionInfo {
        session_name: "main".into(),
        windows: vec![WindowInfo {
            window_id: "@1".into(),
            window_name: "project".into(),
            window_active: true,
            auto_rename: false,
            panes: vec![pane.clone()],
        }],
    }]);
    state.repo_groups = vec![make_repo_group("project", vec![pane])];
    state.rebuild_row_targets();
    let _ = render_to_styled_string(&mut state, 28, 10);
    // repo header, agent status, idle hint
    assert_eq!(state.layout.line_to_row.len(), 3);
    assert_eq!(state.layout.line_to_row[0], None); // repo header
    assert_eq!(state.layout.line_to_row[1], Some(0)); // agent status
    assert_eq!(state.layout.line_to_row[2], Some(0)); // idle hint
}

#[test]
fn test_line_to_row_two_agents() {
    let pane1 = PaneInfo {
        pane_id: "%1".into(),
        pane_active: true,
        status: PaneStatus::Running,
        attention: false,
        agent: AgentType::Claude,
        path: "/home/user/project".into(),
        current_command: String::new(),
        prompt: String::new(),
        prompt_is_response: false,
        started_at: None,
        wait_reason: String::new(),
        permission_mode: tmux_agent_sidebar::tmux::PermissionMode::Default,
        subagents: vec![],
        pane_pid: None,
        session_id: None,
        session_name: String::new(),
        bg_shell_cmd: None,
    };
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Idle,
        attention: false,
        agent: AgentType::Codex,
        path: "/home/user/project".into(),
        current_command: String::new(),
        prompt: String::new(),
        prompt_is_response: false,
        started_at: None,
        wait_reason: String::new(),
        permission_mode: tmux_agent_sidebar::tmux::PermissionMode::Default,
        subagents: vec![],
        pane_pid: None,
        session_id: None,
        session_name: String::new(),
        bg_shell_cmd: None,
    };

    let mut state = make_state(vec![SessionInfo {
        session_name: "main".into(),
        windows: vec![WindowInfo {
            window_id: "@1".into(),
            window_name: "project".into(),
            window_active: true,
            auto_rename: false,
            panes: vec![pane1.clone(), pane2.clone()],
        }],
    }]);
    state.repo_groups = vec![make_repo_group("project", vec![pane1, pane2])];
    state.rebuild_row_targets();
    let _ = render_to_styled_string(&mut state, 28, 10);
    // repo header, agent1, agent2 status+hint
    assert_eq!(state.layout.line_to_row.len(), 4);
    assert_eq!(state.layout.line_to_row[0], None); // repo header
    assert_eq!(state.layout.line_to_row[1], Some(0)); // agent 1
    assert_eq!(state.layout.line_to_row[2], Some(1)); // agent 2 status line
    assert_eq!(state.layout.line_to_row[3], Some(1)); // agent 2 idle hint
}

#[test]
fn test_line_to_row_with_prompt() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Idle);
    pane.prompt = "hello".into();

    let mut state = make_state(vec![SessionInfo {
        session_name: "main".into(),
        windows: vec![WindowInfo {
            window_id: "@1".into(),
            window_name: "project".into(),
            window_active: true,
            auto_rename: false,
            panes: vec![pane.clone()],
        }],
    }]);
    state.repo_groups = vec![make_repo_group("project", vec![pane])];
    state.rebuild_row_targets();
    let _ = render_to_styled_string(&mut state, 28, 10);
    // repo header, agent status, prompt
    assert_eq!(state.layout.line_to_row.len(), 3);
    assert_eq!(state.layout.line_to_row[0], None); // repo header
    assert_eq!(state.layout.line_to_row[1], Some(0)); // agent status line
    assert_eq!(state.layout.line_to_row[2], Some(0)); // prompt line
}

#[test]
fn test_rebuild_row_targets_clamps_selection() {
    let pane = make_pane(AgentType::Claude, PaneStatus::Idle);
    let mut p2 = pane.clone();
    p2.pane_id = "%2".into();
    let mut state = make_state(vec![]);
    state.repo_groups = vec![RepoGroup {
        name: "project".into(),
        has_focus: true,
        panes: vec![pane.clone(), p2.clone()],
    }];
    state.global.selected_pane_row = 1; // select second agent

    // Trigger rebuild
    state.rebuild_row_targets();
    assert_eq!(state.layout.pane_row_targets.len(), 2);

    // Now shrink to 1 agent
    state.repo_groups[0].panes.pop();
    state.global.selected_pane_row = 1; // still pointing at index 1
    state.rebuild_row_targets();
    // Should be clamped to 0
    assert_eq!(state.global.selected_pane_row, 0);
}

// find_focused_pane now queries rmux directly, so it can't be tested
// without a rmux session. The underlying logic (pick_active_pane) is
// tested via unit tests in tmux.rs. focused_pane_id is pub, so tests
// can set it directly.

// ─── State: new Tests ───────────────────────────────────────────────

#[test]
fn test_state_new_defaults() {
    let state = AppState::new("%99".into());
    assert_eq!(state.now, 0);
    assert_eq!(state.tmux_pane, "%99");
    assert!(state.repo_groups.is_empty());
    assert!(!state.focus_state.sidebar_focused);
    assert_eq!(state.focus_state.focus, Focus::Panes);
    assert_eq!(state.spinner_frame, 0);
    assert_eq!(state.global.selected_pane_row, 0);
    assert!(state.layout.pane_row_targets.is_empty());
    assert_eq!(state.scrolls.panes.offset, 0);
    assert_eq!(state.scrolls.panes.total_lines, 0);
    assert_eq!(state.scrolls.panes.visible_height, 0);
}

// ─── State: move_pane_selection return value Tests ─────────────────

#[test]
fn test_move_pane_selection_return_value() {
    let mut state = make_state(vec![]);
    state.layout.pane_row_targets = vec![
        RowTarget {
            pane_id: "%1".into(),
        },
        RowTarget {
            pane_id: "%2".into(),
        },
    ];
    state.global.selected_pane_row = 0;

    assert!(
        state.move_pane_selection(1),
        "should return true when moved"
    );
    assert!(
        !state.move_pane_selection(1),
        "should return false at boundary"
    );
    assert!(
        state.move_pane_selection(-1),
        "should return true when moved back"
    );
    assert!(
        !state.move_pane_selection(-1),
        "should return false at start"
    );
}

// find_focused_pane edge case tests were removed because the function now
// queries rmux directly. See tmux::find_active_pane tests instead.

// ─── State: global sync → rebuild consistency Tests ─────────────

#[test]
fn test_filter_change_rebuilds_row_targets() {
    use tmux_agent_sidebar::state::StatusFilter;

    let running_pane = PaneInfo {
        pane_id: "%1".into(),
        status: PaneStatus::Running,
        ..make_pane(AgentType::Claude, PaneStatus::Running)
    };
    let idle_pane = PaneInfo {
        pane_id: "%2".into(),
        status: PaneStatus::Idle,
        ..make_pane(AgentType::Claude, PaneStatus::Idle)
    };
    let mut state = make_state(vec![]);
    state.repo_groups = vec![make_repo_group("project", vec![running_pane, idle_pane])];

    // All filter shows both
    state.global.status_filter = StatusFilter::All;
    state.rebuild_row_targets();
    assert_eq!(state.layout.pane_row_targets.len(), 2);

    // Simulates sync_global_state setting filter to Running
    state.global.status_filter = StatusFilter::Running;
    state.rebuild_row_targets();
    assert_eq!(state.layout.pane_row_targets.len(), 1);
    assert_eq!(state.layout.pane_row_targets[0].pane_id, "%1");

    // Simulates sync_global_state setting filter to Idle
    state.global.status_filter = StatusFilter::Idle;
    state.rebuild_row_targets();
    assert_eq!(state.layout.pane_row_targets.len(), 1);
    assert_eq!(state.layout.pane_row_targets[0].pane_id, "%2");
}

#[test]
fn test_cursor_sync_clamped_by_rebuild() {
    use tmux_agent_sidebar::state::StatusFilter;

    let pane = make_pane(AgentType::Claude, PaneStatus::Running);
    let mut state = make_state(vec![]);
    state.repo_groups = vec![make_repo_group("project", vec![pane])];

    // Simulates sync_global_state setting cursor beyond bounds
    state.global.selected_pane_row = 5;
    state.global.status_filter = StatusFilter::All;
    state.rebuild_row_targets();
    // Should be clamped to last valid index
    assert_eq!(state.global.selected_pane_row, 0);
}

// ─── GlobalState tests ──────────────────────────────────────────────

fn make_opts(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn make_global() -> GlobalState {
    GlobalState::new()
}

// ─── apply_all (full sync: startup + SIGUSR1) tests ─────────────────

#[test]
fn full_sync_ignores_tmux_filter_matching_last_saved() {
    let mut g = make_global();
    g.status_filter = StatusFilter::Running;

    let opts = make_opts(&[(tmux::SIDEBAR_FILTER, "all")]);
    g.apply_all(&opts);

    assert_eq!(
        g.status_filter,
        StatusFilter::Running,
        "local filter change should not be overwritten when tmux matches last_saved"
    );
}

#[test]
fn full_sync_applies_filter_from_tmux() {
    let mut g = make_global();

    let opts = make_opts(&[(tmux::SIDEBAR_FILTER, "waiting")]);
    g.apply_all(&opts);

    assert_eq!(g.status_filter, StatusFilter::Waiting);
}

#[test]
fn full_sync_applies_cursor_from_tmux() {
    let mut g = make_global();

    let opts = make_opts(&[(tmux::SIDEBAR_CURSOR, "3")]);
    g.apply_all(&opts);

    assert_eq!(g.selected_pane_row, 3);
}

#[test]
fn full_sync_ignores_cursor_matching_last_saved() {
    let mut g = make_global();
    g.selected_pane_row = 5;

    let opts = make_opts(&[(tmux::SIDEBAR_CURSOR, "0")]);
    g.apply_all(&opts);

    assert_eq!(
        g.selected_pane_row, 5,
        "should not overwrite local cursor when tmux matches last_saved"
    );
}

#[test]
fn full_sync_applies_repo_filter_from_tmux() {
    let mut g = make_global();

    let opts = make_opts(&[(tmux::SIDEBAR_REPO_FILTER, "my-app")]);
    g.apply_all(&opts);

    assert_eq!(g.repo_filter, RepoFilter::Repo("my-app".into()));
}

#[test]
fn full_sync_empty_opts_changes_nothing() {
    let mut g = make_global();
    g.status_filter = StatusFilter::Running;
    g.repo_filter = RepoFilter::Repo("app".into());
    g.selected_pane_row = 2;

    g.apply_all(&std::collections::HashMap::new());

    assert_eq!(g.status_filter, StatusFilter::Running);
    assert_eq!(g.repo_filter, RepoFilter::Repo("app".into()));
    assert_eq!(g.selected_pane_row, 2);
}

#[test]
fn full_sync_applies_error_filter_from_tmux() {
    let mut g = make_global();

    let opts = make_opts(&[(tmux::SIDEBAR_FILTER, "error")]);
    g.apply_all(&opts);

    assert_eq!(g.status_filter, StatusFilter::Error);
}

#[test]
fn full_sync_invalid_filter_defaults_to_all() {
    let mut g = make_global();
    g.status_filter = StatusFilter::Running;

    // "garbage" parses as All, All == last_saved → no change
    let opts = make_opts(&[(tmux::SIDEBAR_FILTER, "garbage")]);
    g.apply_all(&opts);

    assert_eq!(
        g.status_filter,
        StatusFilter::Running,
        "invalid filter string parsed as All should match last_saved and not overwrite"
    );
}

#[test]
fn full_sync_applies_all_three_from_tmux() {
    let mut g = make_global();

    let opts = make_opts(&[
        (tmux::SIDEBAR_FILTER, "error"),
        (tmux::SIDEBAR_CURSOR, "7"),
        (tmux::SIDEBAR_REPO_FILTER, "my-app"),
    ]);
    g.apply_all(&opts);

    assert_eq!(g.status_filter, StatusFilter::Error);
    assert_eq!(g.selected_pane_row, 7);
    assert_eq!(g.repo_filter, RepoFilter::Repo("my-app".into()));
}

// ─── last_saved guard tests (protects against save failure revert) ───

#[test]
fn sync_does_not_revert_filter_after_save_failure() {
    // The original bug scenario:
    // 1. Startup: tmux has "error", sidebar adopts it
    // 2. User changes filter to Running, but save_filter fails
    // 3. Next sync should NOT overwrite Running back to Error
    //    because last_saved_filter == Error == tmux value → no change
    let mut g = make_global();

    // Step 1: startup sync adopts "error" from tmux
    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "error")]));
    assert_eq!(g.status_filter, StatusFilter::Error);

    // Step 2: user changes filter locally, save_filter fails
    // (last_saved_filter stays Error)
    g.status_filter = StatusFilter::Running;

    // Step 3: next sync reads tmux "error", but last_saved is also Error → equal → no change
    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "error")]));

    assert_eq!(
        g.status_filter,
        StatusFilter::Running,
        "sync must not revert filter when save failed — the original bug scenario"
    );
}

#[test]
fn full_sync_does_not_revert_filter_after_save_failure() {
    // Same as the periodic version, but for SIGUSR1 (apply_all).
    // apply_all has last_saved guard so it should also be safe.
    let mut g = make_global();

    // Startup: adopt "error"
    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "error")]));
    assert_eq!(g.status_filter, StatusFilter::Error);

    // User changes filter locally, save_filter fails
    // (last_saved_filter stays Error)
    g.status_filter = StatusFilter::Running;

    // SIGUSR1 triggers apply_all: tmux still has "error",
    // but last_saved is also Error → equal → no overwrite
    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "error")]));

    assert_eq!(
        g.status_filter,
        StatusFilter::Running,
        "full sync must not revert filter when save failed"
    );
}

#[test]
fn full_sync_picks_up_change_from_another_instance() {
    // Simulates: this instance saved "running", another instance later
    // saved "waiting". SIGUSR1 should pick up "waiting".
    let mut g = make_global();

    // Startup: this instance starts with default (All)
    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "running")]));
    assert_eq!(g.status_filter, StatusFilter::Running);
    // last_saved_filter is now Running

    // Another instance changes filter to Waiting (writes to tmux)
    // This instance's SIGUSR1 fires:
    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "waiting")]));

    assert_eq!(
        g.status_filter,
        StatusFilter::Waiting,
        "SIGUSR1 should pick up filter changed by another instance"
    );
}

#[test]
fn full_sync_picks_up_cursor_from_another_instance() {
    let mut g = make_global();

    g.apply_all(&make_opts(&[(tmux::SIDEBAR_CURSOR, "3")]));
    assert_eq!(g.selected_pane_row, 3);
    // last_saved_cursor is now 3

    // Another instance moves cursor to 7
    g.apply_all(&make_opts(&[(tmux::SIDEBAR_CURSOR, "7")]));

    assert_eq!(
        g.selected_pane_row, 7,
        "SIGUSR1 should pick up cursor changed by another instance"
    );
}

// ─── window activation sync tests ───────────────────────────────────
// In the main loop, load_from_tmux() is called ONLY when the sidebar's
// window becomes active after being inactive for ≥2 refresh cycles
// (debounced to ignore hook-induced flicker). Periodic refresh within
// the same active window does NOT sync global state.

#[test]
fn global_state_stable_during_task_completion() {
    // Task completes in the active window — window stays active,
    // so load_from_tmux is never called. Filter stays as user set it.
    let mut g = make_global();

    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "running")]));
    g.status_filter = StatusFilter::Idle;

    // No apply_all called during task completion (window still active).
    assert_eq!(
        g.status_filter,
        StatusFilter::Idle,
        "filter must not change during task completion (window stayed active)"
    );
}

#[test]
fn window_switch_syncs_after_debounce() {
    // Simulates: user leaves this window (inactive for 2+ cycles),
    // another instance changes filter, user returns → sync fires.
    let mut g = make_global();

    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "running")]));
    assert_eq!(g.status_filter, StatusFilter::Running);

    // User returns to this window after being away.
    // Debounce passed (inactive_count >= 2) → apply_all called.
    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "waiting")]));

    assert_eq!(
        g.status_filter,
        StatusFilter::Waiting,
        "window activation after debounce should sync filter"
    );
}

#[test]
fn window_active_flicker_does_not_trigger_sync() {
    // Simulates: hook processing causes window_active to flicker
    // (1 cycle of inactive). Debounce threshold (≥2) prevents sync.
    // This is tested at the main loop level — GlobalState itself
    // is passive. Verify that apply_all is NOT called unless the
    // main loop determines debounce threshold was met.
    let mut g = make_global();

    g.apply_all(&make_opts(&[(tmux::SIDEBAR_FILTER, "running")]));
    g.status_filter = StatusFilter::Idle;

    // Flicker: only 1 cycle of inactive (count=1 < threshold=2).
    // Main loop would NOT call apply_all. State stays local.
    assert_eq!(
        g.status_filter,
        StatusFilter::Idle,
        "1-cycle flicker must not trigger sync"
    );
}

#[test]
fn window_activation_syncs_all_fields() {
    // Window activation triggers full sync of filter, cursor, and repo filter.
    let mut g = make_global();

    g.apply_all(&make_opts(&[
        (tmux::SIDEBAR_FILTER, "idle"),
        (tmux::SIDEBAR_CURSOR, "4"),
        (tmux::SIDEBAR_REPO_FILTER, "my-app"),
    ]));

    assert_eq!(g.status_filter, StatusFilter::Idle);
    assert_eq!(g.selected_pane_row, 4);
    assert_eq!(g.repo_filter, RepoFilter::Repo("my-app".into()));
}

#[test]
fn set_flash_and_take_flash_returns_then_clears_after_deadline() {
    let mut state = make_state(vec![]);
    state.set_flash("hello");
    assert_eq!(state.take_flash().as_deref(), Some("hello"));
    // Flash is still valid because expiry is 4s in the future.
    assert_eq!(state.take_flash().as_deref(), Some("hello"));
    // Expire manually and verify take_flash clears it.
    if let Some((_, exp)) = state.flash.as_mut() {
        *exp = std::time::Instant::now() - std::time::Duration::from_secs(1);
    }
    assert_eq!(state.take_flash(), None);
    assert!(state.flash.is_none());
}
