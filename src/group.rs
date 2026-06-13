use indexmap::IndexMap;

use crate::tmux::PaneInfo;

/// A group of panes sharing the same working directory.
#[derive(Debug, Clone)]
pub struct RepoGroup {
    /// Display name: directory basename, or a fallback for empty paths.
    pub name: String,
    /// Whether any pane in the group belongs to the focused active window.
    pub has_focus: bool,
    /// Panes in this group.
    pub panes: Vec<PaneInfo>,
}

/// Group all panes across all sessions by the working directory returned by
/// rmux. No Git repository discovery is performed.
pub fn group_panes_by_repo(sessions: &[crate::tmux::SessionInfo]) -> Vec<RepoGroup> {
    let mut groups: IndexMap<String, RepoGroup> = IndexMap::new();

    for session in sessions {
        for window in &session.windows {
            for pane in &window.panes {
                let group_key = pane.path.clone();
                let display_name = display_name_for_path(&group_key);
                let has_focus = window.window_active && pane.pane_active;

                let group = groups.entry(group_key).or_insert_with(|| RepoGroup {
                    name: display_name,
                    has_focus: false,
                    panes: Vec::new(),
                });

                if has_focus {
                    group.has_focus = true;
                }

                group.panes.push(pane.clone());
            }
        }
    }

    let mut result: Vec<RepoGroup> = groups.into_values().collect();
    result.sort_by_key(|a| a.name.to_lowercase());
    result
}

fn display_name_for_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "(unknown)".to_string();
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pane(id: &str, path: &str) -> PaneInfo {
        PaneInfo {
            pane_id: id.into(),
            pane_active: false,
            status: crate::tmux::PaneStatus::Running,
            attention: false,
            agent: crate::tmux::AgentType::Claude,
            path: path.into(),
            current_command: String::new(),
            prompt: String::new(),
            prompt_is_response: false,
            started_at: None,
            wait_reason: String::new(),
            permission_mode: crate::tmux::PermissionMode::Default,
            subagents: vec![],
            pane_pid: None,
            session_id: None,
            session_name: String::new(),
            bg_shell_cmd: None,
        }
    }

    fn test_window(panes: Vec<PaneInfo>, active: bool) -> crate::tmux::WindowInfo {
        crate::tmux::WindowInfo {
            window_id: "@0".into(),
            window_name: "test".into(),
            window_active: active,
            auto_rename: false,
            panes,
        }
    }

    fn test_session(windows: Vec<crate::tmux::WindowInfo>) -> crate::tmux::SessionInfo {
        crate::tmux::SessionInfo {
            session_name: "main".into(),
            windows,
        }
    }

    #[test]
    fn group_panes_empty_sessions() {
        let groups = group_panes_by_repo(&[]);
        assert!(groups.is_empty());
    }

    #[test]
    fn group_panes_same_path() {
        let pane1 = test_pane("%1", "/tmp/project");
        let pane2 = test_pane("%2", "/tmp/project");

        let sessions = vec![test_session(vec![test_window(vec![pane1, pane2], true)])];
        let groups = group_panes_by_repo(&sessions);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].panes.len(), 2);
        assert_eq!(groups[0].panes[0].pane_id, "%1");
        assert_eq!(groups[0].panes[1].pane_id, "%2");
    }

    #[test]
    fn group_panes_display_name_is_basename() {
        let pane = test_pane("%1", "/home/user/project");

        let sessions = vec![test_session(vec![test_window(vec![pane], true)])];
        let groups = group_panes_by_repo(&sessions);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "project");
    }

    #[test]
    fn group_panes_has_focus_from_active_window_and_pane() {
        let mut pane = test_pane("%1", "/tmp/project");
        pane.pane_active = true;

        let sessions = vec![test_session(vec![test_window(vec![pane], true)])];
        let groups = group_panes_by_repo(&sessions);

        assert!(groups[0].has_focus);
    }
}
