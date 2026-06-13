#[allow(dead_code, unused_imports)]
mod test_helpers;

use test_helpers::*;
use tmux_agent_sidebar::activity::{TaskProgress, TaskStatus};
use tmux_agent_sidebar::state::{RepoFilter, StatusFilter};
use tmux_agent_sidebar::tmux::{
    AgentType, PaneInfo, PaneStatus, PermissionMode, SessionInfo, WindowInfo,
};

// ─── UI Snapshot Tests ─────────────────────────────────────────────

#[test]
fn snapshot_single_agent_idle_ui() {
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

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○1  ✕0
    ⓘ                        — ▾
    project
    ┃ ○ claude
        Waiting for prompt…
    ");
}

// Locks down the secondary header layout when there are no notices —
// `make_state()` injects a Claude missing-hook notice as the shared
// baseline so the ⓘ badge is on every other snapshot, which means a
// regression in the no-notices path would slip past unnoticed without
// this dedicated coverage.
#[test]
fn snapshot_secondary_header_without_notices() {
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
    state.notices.missing_hook_groups.clear();
    state.rebuild_row_targets();

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○1  ✕0
                             — ▾
    project
    ┃ ○ claude
        Waiting for prompt…
    ");
}

#[test]
fn snapshot_secondary_header_long_repo_filter_truncated() {
    let pane = make_pane(AgentType::Claude, PaneStatus::Idle);
    let repo_name = "very-long-repository-name-that-exceeds-width";
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
    state.repo_groups = vec![make_repo_group(repo_name, vec![pane])];
    state.global.repo_filter = RepoFilter::Repo(repo_name.into());
    state.rebuild_row_targets();

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○1  ✕0
    ⓘ  very-long-repository-n… ▾
    very-long-repository-name-th
    ┃ ○ claude
        Waiting for prompt…
    ");
}

#[test]
fn snapshot_single_agent_running_with_elapsed() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Running);
    pane.started_at = Some(FIXED_NOW - 125); // 2m5s ago

    let mut state = make_state(vec![SessionInfo {
        session_name: "main".into(),
        windows: vec![WindowInfo {
            window_id: "@1".into(),
            window_name: "dotfiles".into(),
            window_active: true,
            auto_rename: false,
            panes: vec![pane.clone()],
        }],
    }]);
    state.repo_groups = vec![make_repo_group("dotfiles", vec![pane])];
    state.rebuild_row_targets();

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    dotfiles
    ┃ ● claude              2m5s
    ");
}

#[test]
fn snapshot_long_session_name_truncated_keeps_elapsed_visible() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Running);
    pane.session_name = "this-is-a-ridiculously-long-session-name-that-will-not-fit".into();
    pane.started_at = Some(FIXED_NOW - 125); // 2m5s ago

    let mut state = make_state(vec![SessionInfo {
        session_name: "main".into(),
        windows: vec![WindowInfo {
            window_id: "@1".into(),
            window_name: "dotfiles".into(),
            window_active: true,
            auto_rename: false,
            panes: vec![pane.clone()],
        }],
    }]);
    state.repo_groups = vec![make_repo_group("dotfiles", vec![pane])];
    state.rebuild_row_targets();

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    dotfiles
    ┃ ● this-is-a-ridiculo… 2m5s
    ");
}

#[test]
fn running_spinner_different_frame() {
    let pane = make_pane(AgentType::Claude, PaneStatus::Running);
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
    state.spinner_frame = 0;

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude
    ");
}

#[test]
fn snapshot_agent_with_prompt_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Idle);
    pane.prompt = "fix the bug".into();

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

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○1  ✕0
    ⓘ                        — ▾
    project
    ┃ ○ claude
        fix the bug
    ");
}

#[test]
fn snapshot_agent_with_japanese_prompt_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Running);
    pane.prompt = "これって今1時間経っているけど、起動して確認しても問題ない？".into();

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

    let output = render_to_string(&mut state, 28, 27);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude
        こ れ っ て 今 1時 間 経 っ て い
        る け ど 、 起 動 し て 確 認 し て
        も 問 題 な い ？
    ");
}

#[test]
fn snapshot_two_agents_same_window_ui() {
    let pane1 = PaneInfo {
        pane_id: "%1".into(),
        pane_active: true,
        status: PaneStatus::Running,
        attention: false,
        agent: AgentType::Claude,
        path: "/home/user/project".into(),
        current_command: String::new(),
        prompt: "fix the bug".into(),
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

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡2  ●1  ◎0  ◐0  ○1  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude
        fix the bug
      ○ codex
        Waiting for prompt…
    ");
}

#[test]
fn snapshot_two_windows_ui() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let mut pane2 = make_pane(AgentType::Codex, PaneStatus::Idle);
    pane2.pane_id = "%2".into();
    pane2.pane_active = false;

    let mut state = make_state(vec![SessionInfo {
        session_name: "main".into(),
        windows: vec![
            WindowInfo {
                window_id: "@1".into(),
                window_name: "project-a".into(),
                window_active: true,
                auto_rename: false,
                panes: vec![pane1.clone()],
            },
            WindowInfo {
                window_id: "@2".into(),
                window_name: "project-b".into(),
                window_active: false,
                auto_rename: false,
                panes: vec![pane2.clone()],
            },
        ],
    }]);
    // Two different windows → two repo groups
    let mut group1 = make_repo_group("project-a", vec![pane1]);
    group1.has_focus = true;
    let mut group2 = make_repo_group("project-b", vec![pane2]);
    group2.has_focus = false;
    state.repo_groups = vec![group1, group2];
    state.rebuild_row_targets();

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡2  ●1  ◎0  ◐0  ○1  ✕0
    ⓘ                        — ▾
    project-a
    ┃ ● claude
    project-b
      ○ codex
        Waiting for prompt…
    ");
}

#[test]
fn snapshot_multi_session_ui() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let mut pane2 = make_pane(AgentType::Codex, PaneStatus::Idle);
    pane2.pane_id = "%2".into();
    pane2.pane_active = false;

    let mut state = make_state(vec![
        SessionInfo {
            session_name: "main".into(),
            windows: vec![WindowInfo {
                window_id: "@1".into(),
                window_name: "dotfiles".into(),
                window_active: true,
                auto_rename: false,
                panes: vec![pane1.clone()],
            }],
        },
        SessionInfo {
            session_name: "work".into(),
            windows: vec![WindowInfo {
                window_id: "@2".into(),
                window_name: "api".into(),
                window_active: false,
                auto_rename: false,
                panes: vec![pane2.clone()],
            }],
        },
    ]);
    // Multi-session → two repo groups (sessions don't matter for rendering)
    let mut group1 = make_repo_group("dotfiles", vec![pane1]);
    group1.has_focus = true;
    let mut group2 = make_repo_group("api", vec![pane2]);
    group2.has_focus = false;
    state.repo_groups = vec![group1, group2];
    state.rebuild_row_targets();

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡2  ●1  ◎0  ◐0  ○1  ✕0
    ⓘ                        — ▾
    dotfiles
    ┃ ● claude
    api
      ○ codex
        Waiting for prompt…
    ");
}

#[test]
fn snapshot_wait_reason_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Waiting);
    pane.wait_reason = "permission_prompt".into();

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

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐1  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ◐ claude
        permission required
    ");
}

#[test]
fn snapshot_auto_rename_window_title_ui() {
    let pane = make_pane(AgentType::Claude, PaneStatus::Idle);

    let mut state = make_state(vec![SessionInfo {
        session_name: "main".into(),
        windows: vec![WindowInfo {
            window_id: "@1".into(),
            window_name: "fish".into(),
            window_active: true,
            auto_rename: true,
            panes: vec![pane.clone()],
        }],
    }]);
    // auto_rename=true: box title comes from RepoGroup.name (path basename = "project")
    state.repo_groups = vec![make_repo_group("project", vec![pane])];
    state.rebuild_row_targets();

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○1  ✕0
    ⓘ                        — ▾
    project
    ┃ ○ claude
        Waiting for prompt…
    ");
}
#[test]
fn snapshot_prompt_wrapping_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Idle);
    pane.prompt =
        "Please fix the authentication bug in the login flow that causes users to be logged out"
            .into();

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

    let output = render_to_string(&mut state, 28, 27);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○1  ✕0
    ⓘ                        — ▾
    project
    ┃ ○ claude
        Please fix the
        authentication bug in
        the login flow that cau…
    ");
}

#[test]
fn snapshot_selected_unfocused_ui() {
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
    state.focus_state.sidebar_focused = false;

    let output = render_to_string(&mut state, 28, 26);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○1  ✕0
    ⓘ                        — ▾
    project
    ┃ ○ claude
        Waiting for prompt…
    ");
}

#[test]
fn snapshot_error_state_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Error);
    pane.prompt = "something broke".into();

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

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○0  ✕1
    ⓘ                        — ▾
    project
    ┃ ✕ claude
        something broke
    ");
}

#[test]
fn snapshot_narrow_width_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Idle);
    pane.prompt = "hello world".into();

    let mut state = make_state(vec![SessionInfo {
        session_name: "main".into(),
        windows: vec![WindowInfo {
            window_id: "@1".into(),
            window_name: "p".into(),
            window_active: true,
            auto_rename: false,
            panes: vec![pane.clone()],
        }],
    }]);
    state.repo_groups = vec![make_repo_group("project", vec![pane])];
    state.rebuild_row_targets();

    let output = render_to_string(&mut state, 18, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○
    ⓘ              — ▾
    project
    ┃ ○ claude
        hello world
    ");
}

/// Create a state with a dummy session so draw() doesn't show "No agent panes found"
fn make_state_with_groups(
    groups: Vec<tmux_agent_sidebar::group::RepoGroup>,
) -> tmux_agent_sidebar::state::AppState {
    let pane = make_pane(AgentType::Claude, PaneStatus::Idle);
    let mut state = make_state(vec![SessionInfo {
        session_name: "main".into(),
        windows: vec![WindowInfo {
            window_id: "@1".into(),
            window_name: "dummy".into(),
            window_active: true,
            auto_rename: false,
            panes: vec![pane],
        }],
    }]);
    state.repo_groups = groups;
    state.rebuild_row_targets();
    state
}

// ─── Task Progress Variations ─────────────────────────────────────

#[test]
fn snapshot_task_progress_partial_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Running);
    pane.prompt = "working".into();
    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);
    state.set_pane_task_progress(
        "%1",
        Some(TaskProgress {
            tasks: vec![
                ("Task A".into(), TaskStatus::Completed),
                ("Task B".into(), TaskStatus::InProgress),
                ("Task C".into(), TaskStatus::Pending),
            ],
        }),
    );

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude
        ✔◼◻ 1/3
        working
    ");
}

#[test]
fn snapshot_task_progress_all_completed_ui() {
    let pane = make_pane(AgentType::Claude, PaneStatus::Running);
    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);
    state.set_pane_task_progress(
        "%1",
        Some(TaskProgress {
            tasks: vec![
                ("A".into(), TaskStatus::Completed),
                ("B".into(), TaskStatus::Completed),
            ],
        }),
    );

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude
        ✔✔ 2/2
    ");
}

#[test]
fn snapshot_task_progress_all_pending_ui() {
    let pane = make_pane(AgentType::Claude, PaneStatus::Running);
    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);
    state.set_pane_task_progress(
        "%1",
        Some(TaskProgress {
            tasks: vec![
                ("A".into(), TaskStatus::Pending),
                ("B".into(), TaskStatus::Pending),
                ("C".into(), TaskStatus::Pending),
            ],
        }),
    );

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude
        ◻◻◻ 0/3
    ");
}

// ─── Combined Elements ────────────────────────────────────────────

#[test]
fn snapshot_response_japanese_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Idle);
    pane.prompt = "修正が完了しました。テストも全て通っています。".into();
    pane.prompt_is_response = true;
    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);

    let output = render_to_string(&mut state, 30, 27);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐0  ○1  ✕0
    ⓘ                          — ▾
    project
    ┃ ○ claude
      ▷ 修 正 が 完 了 し ま し た 。 テ ス ト
        も 全 て 通 っ て い ま す 。
    ");
}

// ─── Three Groups with Focus ─────────────────────────────────────

#[test]
fn snapshot_three_groups_middle_focused_ui() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let mut pane2 = make_pane(AgentType::Codex, PaneStatus::Idle);
    pane2.pane_id = "%2".into();
    pane2.pane_active = false;
    let mut pane3 = make_pane(AgentType::Claude, PaneStatus::Idle);
    pane3.pane_id = "%3".into();
    pane3.pane_active = false;

    let mut group1 = make_repo_group("repo-a", vec![pane1]);
    group1.has_focus = false;
    let mut group2 = make_repo_group("repo-b", vec![pane2]);
    group2.has_focus = false;
    let mut group3 = make_repo_group("repo-c", vec![pane3]);
    group3.has_focus = false;
    let mut state = make_state_with_groups(vec![group1, group2, group3]);
    state.focus_state.focused_pane_id = Some("%2".into());

    let output = render_to_string(&mut state, 28, 33);
    insta::assert_snapshot!(output, @r"
     ≡3  ●1  ◎0  ◐0  ○2  ✕0
    ⓘ                        — ▾
    repo-a
      ● claude
    repo-b
    ┃ ○ codex
        Waiting for prompt…
    repo-c
      ○ claude
        Waiting for prompt…
    ");
}

// ─── PermissionMode Badges ────────────────────────────────────────

#[test]
fn snapshot_bypass_all_badge_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Running);
    pane.permission_mode = PermissionMode::BypassPermissions;

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude !
    ");
}

#[test]
fn snapshot_full_auto_badge_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Running);
    pane.permission_mode = PermissionMode::Auto;

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude auto
    ");
}

#[test]
fn snapshot_plan_badge_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Running);
    pane.permission_mode = PermissionMode::Plan;

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude plan
    ");
}

#[test]
fn snapshot_accept_edits_badge_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Running);
    pane.permission_mode = PermissionMode::AcceptEdits;

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude edit
    ");
}

#[test]
fn snapshot_wait_reason_elicitation_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Waiting);
    pane.wait_reason = "elicitation_dialog".into();

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐1  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ◐ claude
        waiting for selection
    ");
}

#[test]
fn snapshot_wait_reason_unknown_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Waiting);
    pane.wait_reason = "some_future_reason".into();

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐1  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ◐ claude
        some_future_reason
    ");
}

// ─── Permission Denied ───────────────────────────────────────────

#[test]
fn snapshot_wait_reason_permission_denied_ui() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Waiting);
    pane.wait_reason = "permission_denied".into();

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);

    let output = render_to_string(&mut state, 28, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●0  ◎0  ◐1  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ◐ claude
        permission denied
    ");
}

#[test]
fn right_border_narrow_width_with_badge() {
    let mut pane = make_pane(AgentType::Claude, PaneStatus::Running);
    pane.started_at = Some(FIXED_NOW - 7200); // 2h ago
    pane.permission_mode = PermissionMode::BypassPermissions;
    pane.prompt = "fix the issue".into();

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);

    // Snapshot locks in the `!` badge visibility at narrow width plus a
    // fully-drawn right border.
    let output = render_to_string(&mut state, 22, 25);
    insta::assert_snapshot!(output, @r"
     ≡1  ●1  ◎0  ◐0  ○0  ✕
    ⓘ                  — ▾
    project
    ┃ ● claude !    2h0m0s
        fix the issue
    ");
    // Structural invariant (width-agnostic): every line that starts with a
    // border glyph must also end with one. Kept alongside the snapshot so
    // border regressions are caught even if someone regenerates the snapshot.
    assert_right_border_intact(&output);
}

#[test]
fn right_border_all_permission_modes_and_agents() {
    let modes: &[PermissionMode] = &[
        PermissionMode::Default,
        PermissionMode::Auto,
        PermissionMode::DontAsk,
        PermissionMode::Plan,
        PermissionMode::AcceptEdits,
        PermissionMode::BypassPermissions,
    ];
    let agents = [AgentType::Claude, AgentType::Codex];
    let now = FIXED_NOW;

    // Render every (agent, mode) combination into a single composite string
    // so one inline snapshot covers the full matrix. A regression in any
    // single cell surfaces as a diff that names the exact combo.
    // Each render is also passed through `assert_right_border_intact`, the
    // structural invariant that catches width-agnostic border breakage.
    let mut composite = String::new();
    for agent in &agents {
        for mode in modes {
            let mut pane = make_pane(agent.clone(), PaneStatus::Running);
            pane.permission_mode = mode.clone();
            pane.started_at = Some(now - 5432); // ~1h30m

            let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane])]);
            let rendered = render_to_string(&mut state, 28, 25);
            assert_right_border_intact(&rendered);
            composite.push_str(&format!("=== {:?} / {:?} ===\n", agent, mode));
            composite.push_str(&rendered);
            composite.push_str("\n\n");
        }
    }
    insta::assert_snapshot!(composite, @r"
    === Claude / Default ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude          1h30m32s

    === Claude / Auto ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude auto     1h30m32s

    === Claude / DontAsk ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude dontAsk  1h30m32s

    === Claude / Plan ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude plan     1h30m32s

    === Claude / AcceptEdits ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude edit     1h30m32s

    === Claude / BypassPermissions ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● claude !        1h30m32s

    === Codex / Default ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● codex           1h30m32s

    === Codex / Auto ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● codex auto      1h30m32s

    === Codex / DontAsk ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● codex dontAsk   1h30m32s

    === Codex / Plan ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● codex plan      1h30m32s

    === Codex / AcceptEdits ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● codex edit      1h30m32s

    === Codex / BypassPermissions ===
     ≡1  ●1  ◎0  ◐0  ○0  ✕0
    ⓘ                        — ▾
    project
    ┃ ● codex !         1h30m32s
    ");
}

// ─── Filter Bar Tests ────────────────────────────────────────────

#[test]
fn snapshot_filter_bar_shows_counts() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Idle,
        agent: AgentType::Codex,
        ..make_pane(AgentType::Codex, PaneStatus::Idle)
    };

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane1, pane2])]);
    let output = render_to_string(&mut state, 30, 25);
    insta::assert_snapshot!(output, @r"
     ≡2  ●1  ◎0  ◐0  ○1  ✕0
    ⓘ                          — ▾
    project
    ┃ ● claude
      ○ codex
        Waiting for prompt…
    ");
}

#[test]
fn snapshot_filter_running_hides_idle() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Idle,
        agent: AgentType::Codex,
        ..make_pane(AgentType::Codex, PaneStatus::Idle)
    };

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane1, pane2])]);
    state.global.status_filter = StatusFilter::Running;
    let output = render_to_string(&mut state, 30, 25);
    insta::assert_snapshot!(output, @r"
     ≡2  ●1  ◎0  ◐0  ○1  ✕0
    ⓘ                          — ▾
    project
    ┃ ● claude
    ");
}

#[test]
fn snapshot_filter_idle_hides_running() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Idle,
        agent: AgentType::Codex,
        ..make_pane(AgentType::Codex, PaneStatus::Idle)
    };

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane1, pane2])]);
    state.global.status_filter = StatusFilter::Idle;
    let output = render_to_string(&mut state, 30, 25);
    insta::assert_snapshot!(output, @r"
     ≡2  ●1  ◎0  ◐0  ○1  ✕0
    ⓘ                          — ▾
    project
      ○ codex
        Waiting for prompt…
    ");
}

#[test]
fn snapshot_filter_hides_empty_groups() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Idle,
        agent: AgentType::Codex,
        ..make_pane(AgentType::Codex, PaneStatus::Idle)
    };

    let mut state = make_state_with_groups(vec![
        make_repo_group("repo-a", vec![pane1]),
        make_repo_group("repo-b", vec![pane2]),
    ]);
    state.global.status_filter = StatusFilter::Running;
    let output = render_to_string(&mut state, 30, 25);
    insta::assert_snapshot!(output, @r"
     ≡2  ●1  ◎0  ◐0  ○1  ✕0
    ⓘ                          — ▾
    repo-a
    ┃ ● claude
    ");
}

#[test]
fn snapshot_filter_all_shows_everything() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Idle,
        agent: AgentType::Codex,
        ..make_pane(AgentType::Codex, PaneStatus::Idle)
    };

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane1, pane2])]);
    state.global.status_filter = StatusFilter::All;
    let output = render_to_string(&mut state, 30, 30);
    insta::assert_snapshot!(output, @r"
     ≡2  ●1  ◎0  ◐0  ○1  ✕0
    ⓘ                          — ▾
    project
    ┃ ● claude
      ○ codex
        Waiting for prompt…
    ");
}

#[test]
fn snapshot_filter_bar_icons_use_selected_and_inactive_colors() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Idle,
        agent: AgentType::Codex,
        ..make_pane(AgentType::Codex, PaneStatus::Idle)
    };
    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane1, pane2])]);

    let styled = render_to_styled_string(&mut state, 30, 25);
    let line = styled.lines().next().unwrap();
    insta::assert_snapshot!(line, @" ≡[fg:111]2[fg:255]  ●[fg:245]1[fg:255]  ◎[fg:245]0[fg:245]  ◐[fg:245]0[fg:245]  ○[fg:245]1[fg:255]  ✕[fg:245]0[fg:245]");
}

#[test]
fn snapshot_filter_bar_stays_fixed_on_scroll() {
    // Many agents to force scrolling, verify filter bar always present
    let panes: Vec<_> = (0..6)
        .map(|i| {
            let mut p = make_pane(AgentType::Claude, PaneStatus::Running);
            p.pane_id = format!("%{i}");
            p.pane_active = i == 0;
            p
        })
        .collect();
    let mut state = make_state_with_groups(vec![make_repo_group("project", panes)]);
    state.scrolls.panes.offset = 3; // scroll down

    let output = render_to_string(&mut state, 30, 15);
    insta::assert_snapshot!(output, @r"
     ≡6  ●6  ◎0  ◐0  ○0  ✕0
    ⓘ                          — ▾
    project
      ● claude
    ┃ ● claude
      ● claude
      ● claude
      ● claude
      ● claude
    ");
}

#[test]
fn snapshot_filter_selected_icon_has_color_without_underline() {
    let pane1 = make_pane(AgentType::Claude, PaneStatus::Running);
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Idle,
        agent: AgentType::Codex,
        ..make_pane(AgentType::Codex, PaneStatus::Idle)
    };
    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane1, pane2])]);
    state.global.status_filter = StatusFilter::Running;

    // The inline snapshot captures the styled filter bar; any underline
    // modifier on the selected filter would surface in the snapshot diff.
    let styled = render_to_styled_string(&mut state, 30, 25);
    let line = styled.lines().next().unwrap();
    insta::assert_snapshot!(line, @" ≡[fg:245]2[fg:255]  ●[fg:114]1[fg:255]  ◎[fg:245]0[fg:245]  ◐[fg:245]0[fg:245]  ○[fg:245]1[fg:255]  ✕[fg:245]0[fg:245]");
}

#[test]
fn snapshot_filter_error_shows_agents() {
    let mut pane1 = make_pane(AgentType::Claude, PaneStatus::Error);
    pane1.prompt = "something broke".into();
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Running,
        agent: AgentType::Codex,
        ..make_pane(AgentType::Codex, PaneStatus::Running)
    };

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane1, pane2])]);
    state.global.status_filter = StatusFilter::Error;
    let output = render_to_string(&mut state, 30, 25);
    insta::assert_snapshot!(output, @r"
     ≡2  ●1  ◎0  ◐0  ○0  ✕1
    ⓘ                          — ▾
    project
    ┃ ✕ claude
        something broke
    ");
}

#[test]
fn snapshot_filter_waiting_shows_only_waiting() {
    let mut pane1 = make_pane(AgentType::Claude, PaneStatus::Waiting);
    pane1.wait_reason = "permission_prompt".into();
    let pane2 = PaneInfo {
        pane_id: "%2".into(),
        pane_active: false,
        status: PaneStatus::Idle,
        agent: AgentType::Codex,
        ..make_pane(AgentType::Codex, PaneStatus::Idle)
    };

    let mut state = make_state_with_groups(vec![make_repo_group("project", vec![pane1, pane2])]);
    state.global.status_filter = StatusFilter::Waiting;
    let output = render_to_string(&mut state, 30, 25);
    insta::assert_snapshot!(output, @r"
     ≡2  ●0  ◎0  ◐1  ○1  ✕0
    ⓘ                          — ▾
    project
    ┃ ◐ claude
        permission required
    ");
}
