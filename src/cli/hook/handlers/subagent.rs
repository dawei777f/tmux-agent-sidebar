use crate::tmux;

use super::super::context::{append_subagent, remove_subagent};

pub(in crate::cli::hook) fn on_subagent_start(
    pane: &str,
    agent_type: &str,
    agent_id: Option<&str>,
) -> i32 {
    // Claude Code always sends agent_id per the hooks spec; drop the
    // event silently if it's missing so the tree never gains an
    // untrackable entry.
    let Some(id) = agent_id.filter(|s| !s.is_empty()) else {
        return 0;
    };
    let current = tmux::get_pane_option_value(pane, tmux::PANE_SUBAGENTS);
    let new_val = append_subagent(&current, agent_type, id);
    tmux::set_pane_option(pane, tmux::PANE_SUBAGENTS, &new_val);
    0
}

pub(in crate::cli::hook) fn on_subagent_stop(pane: &str, agent_id: Option<&str>) -> i32 {
    let Some(id) = agent_id.filter(|s| !s.is_empty()) else {
        return 0;
    };
    let current = tmux::get_pane_option_value(pane, tmux::PANE_SUBAGENTS);
    match remove_subagent(&current, id) {
        None => {}
        Some(new_val) if new_val.is_empty() => {
            tmux::unset_pane_option(pane, tmux::PANE_SUBAGENTS);
        }
        Some(new_val) => {
            tmux::set_pane_option(pane, tmux::PANE_SUBAGENTS, &new_val);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_subagent_start_appends_to_list() {
        let _guard = tmux::test_mock::install();
        let pane = "%SUB_START";
        on_subagent_start(pane, "Explore", Some("sub-1"));
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_SUBAGENTS).as_deref(),
            Some("Explore:sub-1")
        );
        on_subagent_start(pane, "Plan", Some("sub-2"));
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_SUBAGENTS).as_deref(),
            Some("Explore:sub-1,Plan:sub-2")
        );
    }

    #[test]
    fn on_subagent_start_drops_event_without_id() {
        let _guard = tmux::test_mock::install();
        let pane = "%SUB_NO_ID";
        on_subagent_start(pane, "Explore", None);
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_SUBAGENTS));
        on_subagent_start(pane, "Explore", Some(""));
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_SUBAGENTS));
    }

    #[test]
    fn on_subagent_stop_removes_matching_id() {
        let _guard = tmux::test_mock::install();
        let pane = "%SUB_STOP";
        tmux::test_mock::set(pane, tmux::PANE_SUBAGENTS, "Explore:sub-1,Plan:sub-2");
        on_subagent_stop(pane, Some("sub-1"));
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_SUBAGENTS).as_deref(),
            Some("Plan:sub-2")
        );
    }

    #[test]
    fn on_subagent_stop_clears_option_when_last_subagent_stops() {
        let _guard = tmux::test_mock::install();
        let pane = "%SUB_STOP_LAST";
        tmux::test_mock::set(pane, tmux::PANE_SUBAGENTS, "Explore:sub-1");
        on_subagent_stop(pane, Some("sub-1"));
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_SUBAGENTS));
    }
}
