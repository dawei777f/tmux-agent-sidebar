use crate::cli::{set_attention, set_status};
use crate::tmux;

use super::super::context::{AgentContext, set_agent_meta};
use super::status_priority::resolve_notification_status;

pub(in crate::cli::hook) fn on_notification(
    pane: &str,
    ctx: &AgentContext<'_>,
    wait_reason: &str,
    meta_only: bool,
) -> i32 {
    set_agent_meta(pane, ctx);
    if meta_only {
        return 0;
    }
    let bg_shell_live = !tmux::get_pane_option_value(pane, tmux::PANE_BG_CMD).is_empty();
    set_status(
        pane,
        resolve_notification_status(wait_reason, bg_shell_live),
    );
    set_attention(pane, "notification");
    if wait_reason.is_empty() {
        tmux::unset_pane_option(pane, tmux::PANE_WAIT_REASON);
    } else {
        tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, wait_reason);
    }
    0
}

pub(in crate::cli::hook) fn on_permission_denied(pane: &str, ctx: &AgentContext<'_>) -> i32 {
    set_agent_meta(pane, ctx);
    set_status(pane, "waiting");
    set_attention(pane, "notification");
    tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, "permission_denied");
    0
}

pub(in crate::cli::hook) fn on_teammate_idle(
    pane: &str,
    teammate_name: &str,
    idle_reason: &str,
) -> i32 {
    set_attention(pane, "notification");
    let reason = if idle_reason.is_empty() {
        format!("teammate_idle:{teammate_name}")
    } else {
        format!("teammate_idle:{teammate_name}:{idle_reason}")
    };
    tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, &reason);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic_ctx() -> AgentContext<'static> {
        AgentContext {
            agent: "claude",
            cwd: "/repo",
            permission_mode: "default",
            session_id: &None,
        }
    }

    #[test]
    fn on_teammate_idle_sets_attention_and_reason() {
        let _guard = tmux::test_mock::install();
        let pane = "%TEAM";
        let exit = on_teammate_idle(pane, "alice", "");
        assert_eq!(exit, 0);
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_ATTENTION).as_deref(),
            Some("notification")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_WAIT_REASON).as_deref(),
            Some("teammate_idle:alice")
        );
    }

    #[test]
    fn on_teammate_idle_includes_idle_reason_when_present() {
        let _guard = tmux::test_mock::install();
        let pane = "%TEAM_REASON";
        on_teammate_idle(pane, "alice", "tokens_exhausted");
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_WAIT_REASON).as_deref(),
            Some("teammate_idle:alice:tokens_exhausted")
        );
    }

    #[test]
    fn on_notification_meta_only_skips_status_and_attention() {
        let _guard = tmux::test_mock::install();
        let pane = "%NOTIF_META";
        let ctx = basic_ctx();
        on_notification(pane, &ctx, "permission", true);
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_STATUS));
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_ATTENTION));
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_WAIT_REASON));
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_AGENT).as_deref(),
            Some("claude")
        );
    }

    #[test]
    fn on_notification_sets_waiting_status_and_reason() {
        let _guard = tmux::test_mock::install();
        let pane = "%NOTIF_WAIT";
        let ctx = basic_ctx();
        on_notification(pane, &ctx, "permission", false);
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("waiting")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_ATTENTION).as_deref(),
            Some("notification")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_WAIT_REASON).as_deref(),
            Some("permission")
        );
    }

    #[test]
    fn on_notification_keeps_background_when_softer_reason_and_bg_shell_live() {
        let _guard = tmux::test_mock::install();
        let pane = "%NOTIF_BG_PREEMPT";
        tmux::test_mock::set(pane, tmux::PANE_BG_CMD, "cargo test");
        let ctx = basic_ctx();
        on_notification(pane, &ctx, "auth_success", false);
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("background"),
        );
    }

    #[test]
    fn on_notification_permission_reason_preempts_background() {
        let _guard = tmux::test_mock::install();
        let pane = "%NOTIF_PERM_OVER_BG";
        tmux::test_mock::set(pane, tmux::PANE_BG_CMD, "cargo test");
        let ctx = basic_ctx();
        on_notification(pane, &ctx, "permission_prompt", false);
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("waiting"),
        );
    }

    #[test]
    fn on_notification_empty_wait_reason_clears_stale_value() {
        let _guard = tmux::test_mock::install();
        let pane = "%NOTIF_STALE";
        tmux::test_mock::set(pane, tmux::PANE_WAIT_REASON, "permission");
        let ctx = basic_ctx();
        on_notification(pane, &ctx, "", false);
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_WAIT_REASON));
    }

    #[test]
    fn on_permission_denied_records_permission_denied_wait_reason() {
        let _guard = tmux::test_mock::install();
        let pane = "%PD";
        let ctx = basic_ctx();
        on_permission_denied(pane, &ctx);
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_WAIT_REASON).as_deref(),
            Some("permission_denied")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("waiting")
        );
    }
}
