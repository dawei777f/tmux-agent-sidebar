use crate::cli::{sanitize_tmux_value, set_attention, set_status};
use crate::tmux;

use crate::time::now_epoch_secs;

use super::super::context::{
    AgentContext, clear_run_state, is_system_message, mark_task_reset, set_agent_meta,
};
use super::status_priority::resolve_stop_status;

pub(in crate::cli::hook) fn on_user_prompt_submit(
    pane: &str,
    ctx: &AgentContext<'_>,
    prompt: &str,
) -> i32 {
    set_agent_meta(pane, ctx);
    set_attention(pane, "clear");
    set_status(pane, "running");
    if !prompt.is_empty() && !is_system_message(prompt) {
        let p = sanitize_tmux_value(prompt);
        tmux::set_pane_option(pane, tmux::PANE_PROMPT, &p);
        tmux::set_pane_option(pane, tmux::PANE_PROMPT_SOURCE, "user");
    }
    tmux::set_pane_option(pane, tmux::PANE_STARTED_AT, &now_epoch_secs().to_string());
    tmux::unset_pane_option(pane, tmux::PANE_WAIT_REASON);
    0
}

pub(in crate::cli::hook) fn on_stop(
    pane: &str,
    ctx: &AgentContext<'_>,
    last_message: &str,
    response: Option<&str>,
) -> i32 {
    set_agent_meta(pane, ctx);
    set_attention(pane, "clear");
    if !last_message.is_empty() {
        let msg = sanitize_tmux_value(last_message);
        tmux::set_pane_option(pane, tmux::PANE_PROMPT, &msg);
        tmux::set_pane_option(pane, tmux::PANE_PROMPT_SOURCE, "response");
    }
    let bg_shell_live = !tmux::get_pane_option_value(pane, tmux::PANE_BG_CMD).is_empty();
    if bg_shell_live {
        tmux::unset_pane_option(pane, tmux::PANE_WAIT_REASON);
    } else {
        clear_run_state(pane);
    }
    mark_task_reset(pane);
    set_status(pane, resolve_stop_status(bg_shell_live));
    if let Some(resp) = response {
        println!("{resp}");
    }
    0
}

pub(in crate::cli::hook) fn on_stop_failure(
    pane: &str,
    ctx: &AgentContext<'_>,
    error: &str,
) -> i32 {
    set_agent_meta(pane, ctx);
    set_attention(pane, "clear");
    clear_run_state(pane);
    mark_task_reset(pane);
    if !error.is_empty() {
        tmux::set_pane_option(pane, tmux::PANE_WAIT_REASON, error);
    }
    set_status(pane, "error");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_user_prompt_submit_sets_running_and_stores_prompt() {
        let _guard = tmux::test_mock::install();
        let pane = "%PROMPT";
        let ctx = AgentContext {
            agent: "claude",
            cwd: "/repo",
            permission_mode: "default",
            session_id: &None,
        };
        let exit = on_user_prompt_submit(pane, &ctx, "fix the bug");
        assert_eq!(exit, 0);
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("running")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_PROMPT).as_deref(),
            Some("fix the bug")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_PROMPT_SOURCE).as_deref(),
            Some("user")
        );
        assert!(tmux::test_mock::contains(pane, tmux::PANE_STARTED_AT));
    }

    #[test]
    fn on_user_prompt_submit_ignores_system_messages() {
        let _guard = tmux::test_mock::install();
        let pane = "%SYS_PROMPT";
        let ctx = AgentContext {
            agent: "claude",
            cwd: "/repo",
            permission_mode: "default",
            session_id: &None,
        };
        on_user_prompt_submit(pane, &ctx, "<system-reminder>ignore me</system-reminder>");
        assert!(
            !tmux::test_mock::contains(pane, tmux::PANE_PROMPT),
            "system messages should not be stored as user prompt"
        );
        // But status should still advance to running.
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("running")
        );
    }

    #[test]
    fn on_user_prompt_submit_clears_stale_wait_reason_but_preserves_bg_cmd() {
        let _guard = tmux::test_mock::install();
        let pane = "%PROMPT_CLEAR_WAIT";
        tmux::test_mock::set(pane, tmux::PANE_WAIT_REASON, "permission");
        tmux::test_mock::set(pane, tmux::PANE_BG_CMD, "npm run dev");
        let ctx = AgentContext {
            agent: "claude",
            cwd: "/repo",
            permission_mode: "default",
            session_id: &None,
        };
        on_user_prompt_submit(pane, &ctx, "new prompt");
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_WAIT_REASON));
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_BG_CMD).as_deref(),
            Some("npm run dev"),
            "bg command must survive a new user turn — the shell is still running",
        );
    }

    #[test]
    fn on_stop_with_background_shell_sets_background_status() {
        let _guard = tmux::test_mock::install();
        let pane = "%STOP_BG";
        tmux::test_mock::set(pane, tmux::PANE_BG_CMD, "npm run dev");
        tmux::test_mock::set(pane, tmux::PANE_STARTED_AT, "123");
        let ctx = AgentContext {
            agent: "claude",
            cwd: "/repo",
            permission_mode: "default",
            session_id: &None,
        };

        let exit = on_stop(pane, &ctx, "", None);

        assert_eq!(exit, 0);
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("background")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STARTED_AT).as_deref(),
            Some("123")
        );
    }

    #[test]
    fn on_stop_without_background_shell_sets_idle_status() {
        let _guard = tmux::test_mock::install();
        let pane = "%STOP_IDLE";
        tmux::test_mock::set(pane, tmux::PANE_STARTED_AT, "123");
        let ctx = AgentContext {
            agent: "claude",
            cwd: "/repo",
            permission_mode: "default",
            session_id: &None,
        };

        on_stop(pane, &ctx, "", None);

        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("idle")
        );
        assert!(!tmux::test_mock::contains(pane, tmux::PANE_STARTED_AT));
    }

    #[test]
    fn on_stop_failure_records_error_wait_reason_and_error_status() {
        let _guard = tmux::test_mock::install();
        let pane = "%STOP_FAIL";
        let ctx = AgentContext {
            agent: "claude",
            cwd: "/repo",
            permission_mode: "default",
            session_id: &None,
        };
        let exit = on_stop_failure(pane, &ctx, "rate_limit");
        assert_eq!(exit, 0);
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_STATUS).as_deref(),
            Some("error")
        );
        assert_eq!(
            tmux::test_mock::get(pane, tmux::PANE_WAIT_REASON).as_deref(),
            Some("rate_limit")
        );
    }
}
