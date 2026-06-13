use crate::tmux;

/// Returns whether the pane's cwd should be updated.
/// When subagents are active, events may come from a child agent, so we
/// should not overwrite the parent agent's cwd/session metadata.
pub(in crate::cli::hook) fn should_update_cwd(current_subagents: &str) -> bool {
    current_subagents.is_empty()
}

pub(in crate::cli::hook) fn sync_pane_location(pane: &str, cwd: &str, session_id: &Option<String>) {
    // Subagents share the parent's $RMUX_PANE and can fire their own hook
    // events with a different session_id or cwd. While children are active,
    // every pane-scoped write must be skipped so the parent's identity is
    // preserved.
    let current_subagents = tmux::get_pane_option_value(pane, tmux::PANE_SUBAGENTS);
    if !should_update_cwd(&current_subagents) {
        return;
    }
    match session_id.as_deref() {
        Some(sid) if !sid.is_empty() => tmux::set_pane_option(pane, tmux::PANE_SESSION_ID, sid),
        _ => tmux::unset_pane_option(pane, tmux::PANE_SESSION_ID),
    }
    if !cwd.is_empty() {
        tmux::set_pane_option(pane, tmux::PANE_CWD, cwd);
    }
}

/// Returns true if pane-scoped writes from this hook event are safe to
/// apply to the pane's metadata. False while subagents are active so a
/// child hook cannot clobber the parent pane's identity.
pub(in crate::cli::hook) fn pane_writes_allowed(pane: &str) -> bool {
    let current_subagents = tmux::get_pane_option_value(pane, tmux::PANE_SUBAGENTS);
    should_update_cwd(&current_subagents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_update_cwd_when_no_subagents() {
        assert!(should_update_cwd(""));
    }

    #[test]
    fn should_not_update_cwd_when_subagent_active() {
        assert!(!should_update_cwd("Explore:sub-1"));
    }

    #[test]
    fn should_not_update_cwd_when_multiple_subagents_active() {
        assert!(!should_update_cwd("Explore:sub-1,Plan:sub-2"));
    }

    #[test]
    fn sync_pane_location_skips_writes_while_subagents_active() {
        let _guard = tmux::test_mock::install();
        let pane = "%PARENT";
        tmux::test_mock::set(pane, tmux::PANE_SUBAGENTS, "Explore:sub-1");
        tmux::test_mock::set(pane, tmux::PANE_CWD, "/repo/parent");
        tmux::test_mock::set(pane, tmux::PANE_SESSION_ID, "parent-session");

        sync_pane_location(pane, "/repo/child", &Some("child-session".into()));

        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_CWD).as_deref(),
            Some("/repo/parent")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_SESSION_ID).as_deref(),
            Some("parent-session")
        );
    }

    #[test]
    fn sync_pane_location_writes_cwd_and_session_when_no_subagents() {
        let _guard = tmux::test_mock::install();
        let pane = "%LONE";

        sync_pane_location(pane, "/repo", &Some("sess-1".into()));

        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_CWD).as_deref(),
            Some("/repo")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_SESSION_ID).as_deref(),
            Some("sess-1")
        );
    }

    #[test]
    fn pane_writes_allowed_tracks_subagent_presence() {
        let _guard = tmux::test_mock::install();
        let pane = "%ALLOWED";
        assert!(pane_writes_allowed(pane));
        tmux::test_mock::set(pane, tmux::PANE_SUBAGENTS, "Explore:sub-1");
        assert!(!pane_writes_allowed(pane));
    }
}
