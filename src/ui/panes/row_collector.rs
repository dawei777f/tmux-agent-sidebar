use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::row;
use crate::state::{AppState, Focus};

#[derive(Debug, Default)]
pub(super) struct CollectedRows {
    pub lines: Vec<Line<'static>>,
    pub line_to_row: Vec<Option<usize>>,
}

pub(super) fn collect(state: &AppState, width: u16) -> CollectedRows {
    let width = width as usize;
    let theme = &state.theme;

    let mut collected = CollectedRows::default();
    let filter = state.global.status_filter;
    let mut first_group = true;
    let mut row_index: usize = 0;

    for group in &state.repo_groups {
        if !state.global.repo_filter.matches_group(&group.name) {
            continue;
        }
        let filtered_panes: Vec<_> = group
            .panes
            .iter()
            .filter(|pane| filter.matches(&pane.status))
            .collect();
        if filtered_panes.is_empty() {
            continue;
        }

        if !first_group {
            // Separate repo groups, but do not add a leading blank before
            // the first repo so the list starts immediately below the header.
            collected.lines.push(Line::from(""));
            collected.line_to_row.push(None);
        }
        first_group = false;

        let group_has_focused_pane = state
            .focus_state
            .focused_pane_id
            .as_ref()
            .is_some_and(|fid| group.panes.iter().any(|p| p.pane_id == *fid));

        let title = &group.name;
        let title_color = if group_has_focused_pane {
            theme.accent
        } else {
            theme.text_active
        };
        let spans: Vec<Span<'static>> = vec![Span::styled(
            title.clone(),
            Style::default().fg(title_color),
        )];
        collected.lines.push(Line::from(spans));
        collected.line_to_row.push(None);

        for pane in filtered_panes.iter() {
            let is_selected = state.focus_state.sidebar_focused
                && state.focus_state.focus == Focus::Panes
                && row_index == state.global.selected_pane_row;

            let is_active = state.focus_state.focused_pane_id.as_ref() == Some(&pane.pane_id);

            let task_progress = state
                .pane_state(&pane.pane_id)
                .and_then(|s| s.task_progress.as_ref());
            let pane_lines = row::render_pane_lines(
                pane,
                task_progress,
                is_selected,
                is_active,
                width,
                &state.icons,
                theme,
                state.spinner_frame,
                state.now,
            );
            let pane_line_count = pane_lines.len();
            collected.lines.extend(pane_lines);
            for _ in 0..pane_line_count {
                collected.line_to_row.push(Some(row_index));
            }

            row_index += 1;
        }
    }

    collected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::RepoGroup;
    use crate::state::{AppState, StatusFilter};
    use crate::tmux::{AgentType, PaneInfo, PaneStatus, PermissionMode};

    fn make_pane(id: &str, status: PaneStatus) -> PaneInfo {
        PaneInfo {
            pane_id: id.into(),
            pane_active: false,
            status,
            attention: false,
            agent: AgentType::Claude,
            path: "/tmp/repo".into(),
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

    #[test]
    fn collect_empty_repo_groups_produces_no_lines() {
        let state = AppState::new("%0".into());
        let collected = collect(&state, 40);
        assert!(collected.lines.is_empty());
        assert!(collected.line_to_row.is_empty());
    }

    #[test]
    fn collect_skips_group_when_status_filter_excludes_all_panes() {
        let mut state = AppState::new("%0".into());
        // The group has only Running panes, so filter to Waiting to drop them all.
        state.global.status_filter = StatusFilter::Waiting;
        state.repo_groups = vec![RepoGroup {
            name: "repo".into(),
            has_focus: false,
            panes: vec![make_pane("%1", PaneStatus::Running)],
        }];
        let collected = collect(&state, 40);
        assert!(collected.lines.is_empty());
    }

    #[test]
    fn collect_records_line_to_row_for_rendered_panes() {
        let mut state = AppState::new("%0".into());
        state.repo_groups = vec![RepoGroup {
            name: "repo".into(),
            has_focus: false,
            panes: vec![make_pane("%1", PaneStatus::Running)],
        }];
        let collected = collect(&state, 40);
        assert!(!collected.lines.is_empty());
        assert!(collected.line_to_row.iter().any(|row| row == &Some(0)));
    }

    #[test]
    fn collect_keeps_group_header_unmapped() {
        let mut state = AppState::new("%0".into());
        state.repo_groups = vec![RepoGroup {
            name: "raw-path".into(),
            has_focus: false,
            panes: vec![make_pane("%1", PaneStatus::Running)],
        }];
        let collected = collect(&state, 40);
        assert_eq!(collected.line_to_row.first(), Some(&None));
    }

    #[test]
    fn collect_separates_groups_with_blank_unmapped_line() {
        let mut state = AppState::new("%0".into());
        let group = |name: &str, pane_id: &str| RepoGroup {
            name: name.into(),
            has_focus: false,
            panes: vec![make_pane(pane_id, PaneStatus::Running)],
        };
        state.repo_groups = vec![group("a", "%1"), group("b", "%2")];
        let collected = collect(&state, 40);
        assert!(
            collected
                .lines
                .iter()
                .zip(collected.line_to_row.iter())
                .any(|(line, row)| line.spans.is_empty() && row.is_none())
        );
    }
}
