use crate::cli::{set_attention, set_status};
use crate::tmux;

use super::super::context::{
    AgentContext, PENDING_SESSION_END, clear_run_state, pane_writes_allowed,
    run_session_end_teardown, set_agent_meta,
};

pub(in crate::cli::hook) fn on_session_start(
    pane: &str,
    ctx: &AgentContext<'_>,
    source: &str,
) -> i32 {
    set_agent_meta(pane, ctx);
    set_attention(pane, "clear");
    clear_run_state(pane);
    tmux::unset_pane_option(pane, tmux::PANE_PROMPT);
    tmux::unset_pane_option(pane, tmux::PANE_PROMPT_SOURCE);
    // `@pane_subagents` is deliberately preserved across SessionStart.
    // Subagents share the parent's `$RMUX_PANE`, so when a subagent
    // fires its own SessionStart after SubagentStart has populated the
    // list, clearing it here would drop the marker that
    // `should_update_cwd` and `drain_pending_teardowns` rely on. The
    // normal teardown paths (`run_session_end_teardown` via
    // `clear_all_meta`) already clear the list when a real session
    // ends, so the only state this would skip clearing is a subagent
    // list stranded by a hard crash — acceptable vs. racing against
    // legitimate subagent activity.
    // A fresh session overrides any deferred teardown that was waiting
    // for the previous run's subagents to drain.
    tmux::unset_pane_option(pane, PENDING_SESSION_END);
    match source {
        "resume" => tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, "session_resumed"),
        "compact" => tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, "session_resumed_compact"),
        _ => tmux::unset_pane_option(pane, tmux::PANE_WAIT_REASON),
    }
    set_status(pane, "idle");
    0
}

pub(in crate::cli::hook) fn on_session_end(pane: &str, agent_name: &str, end_reason: &str) -> i32 {
    let _ = (agent_name, end_reason);
    // Subagents share the parent's `$RMUX_PANE`, so a SessionEnd fired
    // while `@pane_subagents` is populated is almost certainly a child's
    // (we have no way to distinguish parent vs. child events otherwise).
    // Bail out early before:
    //
    //   1. the notification path consumes the run-scoped fingerprint,
    //      which would silently deduplicate the parent's real SessionEnd
    //      notification when it eventually arrives, and
    //   2. we set PENDING_SESSION_END, which `drain_pending_teardowns`
    //      would later turn into `run_session_end_teardown` — wiping a
    //      still-running parent pane the moment the last subagent stops.
    //
    // The tradeoff is that a parent SessionEnd that genuinely races
    // ahead of every SubagentStop will be ignored too, leaving stale
    // metadata until the next SessionStart clears it. Compared to
    // clobbering a live parent, the stale-metadata failure mode is
    // far safer and the one the user can recover from.
    if !pane_writes_allowed(pane) {
        return 0;
    }

    run_session_end_teardown(pane);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn basic_ctx() -> AgentContext<'static> {
        AgentContext {
            agent: "claude",
            cwd: "/repo",
            permission_mode: "default",
            session_id: &None,
        }
    }

    #[test]
    fn on_session_end_preserves_parent_state_when_subagents_active() {
        let _guard = tmux::test_mock::install();
        let pane = "%PARENT_END";
        tmux::test_mock::set(pane, tmux::PANE_SUBAGENTS, "Explore:sub-1");
        tmux::test_mock::set(pane, tmux::PANE_AGENT, "claude");
        tmux::test_mock::set(pane, tmux::PANE_CWD, "/repo/parent");
        tmux::test_mock::set(pane, tmux::PANE_SESSION_ID, "parent-session");
        tmux::test_mock::set(pane, tmux::PANE_STATUS, "running");
        let log_path = crate::activity::log_file_path(pane);
        let _ = fs::create_dir_all(log_path.parent().unwrap());
        fs::write(&log_path, "1234567890|Read|main.rs\n").unwrap();

        let exit = on_session_end(pane, "claude", "");

        assert_eq!(exit, 0);
        assert!(tmux::test_mock::contains(pane, tmux::PANE_AGENT));
        assert!(tmux::test_mock::contains(pane, tmux::PANE_CWD));
        assert!(tmux::test_mock::contains(pane, tmux::PANE_SESSION_ID));
        assert!(tmux::test_mock::contains(pane, tmux::PANE_SUBAGENTS));
        assert!(log_path.exists());
        assert!(!tmux::test_mock::contains(pane, PENDING_SESSION_END));
        fs::remove_file(&log_path).ok();
    }

    #[test]
    fn on_session_end_clears_state_when_no_subagents() {
        let _guard = tmux::test_mock::install();
        let pane = "%LONE_END";
        tmux::test_mock::set(pane, tmux::PANE_AGENT, "claude");
        tmux::test_mock::set(pane, tmux::PANE_CWD, "/repo");
        tmux::test_mock::set(pane, tmux::PANE_STATUS, "running");

        let exit = on_session_end(pane, "claude", "");

        assert_eq!(exit, 0);
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_AGENT));
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_CWD));
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_STATUS));
    }

    #[test]
    fn on_session_start_sets_agent_and_idle_status() {
        let _guard = tmux::test_mock::install();
        let pane = "%NEW_SESSION";
        let ctx = AgentContext {
            agent: "claude",
            cwd: "/repo",
            permission_mode: "default",
            session_id: &Some("sess-123".into()),
        };

        let exit = on_session_start(pane, &ctx, "");
        assert_eq!(exit, 0);
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_AGENT).as_deref(),
            Some("claude")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("idle")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_SESSION_ID).as_deref(),
            Some("sess-123")
        );
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_PROMPT));
    }

    #[test]
    fn on_session_start_preserves_subagents_list() {
        let _guard = tmux::test_mock::install();
        let pane = "%SUBAGENT_LIVE";
        tmux::test_mock::set(pane, tmux::PANE_SUBAGENTS, "Explore:sub-1");
        on_session_start(pane, &basic_ctx(), "");
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_SUBAGENTS).as_deref(),
            Some("Explore:sub-1")
        );
    }

    #[test]
    fn fresh_session_start_clears_pending_session_marker() {
        let _guard = tmux::test_mock::install();
        let pane = "%PARENT_RESTART";
        tmux::test_mock::set(pane, PENDING_SESSION_END, "1");
        on_session_start(pane, &basic_ctx(), "");
        assert!(!tmux::test_mock::contains(pane, PENDING_SESSION_END));
    }

    #[test]
    fn on_session_start_resume_writes_wait_reason() {
        let _guard = tmux::test_mock::install();
        let pane = "%RESUME";
        on_session_start(pane, &basic_ctx(), "resume");
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_WAIT_REASON).as_deref(),
            Some("session_resumed"),
        );
    }

    #[test]
    fn on_session_start_compact_writes_compact_wait_reason() {
        let _guard = tmux::test_mock::install();
        let pane = "%COMPACT";
        on_session_start(pane, &basic_ctx(), "compact");
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_WAIT_REASON).as_deref(),
            Some("session_resumed_compact"),
        );
    }

    #[test]
    fn on_session_start_startup_clears_stale_wait_reason() {
        let _guard = tmux::test_mock::install();
        let pane = "%FRESH";
        tmux::test_mock::set(pane, tmux::PANE_WAIT_REASON, "session_resumed");
        on_session_start(pane, &basic_ctx(), "startup");
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_WAIT_REASON));
    }
}
