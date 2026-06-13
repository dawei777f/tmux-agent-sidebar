use crate::event::{AgentEvent, resolve_adapter};

use super::{read_stdin_json, tmux_pane};

mod activity;
mod context;
mod handlers;

use context::sync_pane_location;

// ─── hook subcommand ────────────────────────────────────────────────────────

pub(crate) fn cmd_hook(args: &[String]) -> i32 {
    let agent_name = args.first().map(|s| s.as_str()).unwrap_or("");
    let event_name = args.get(1).map(|s| s.as_str()).unwrap_or("");

    if agent_name.is_empty() || event_name.is_empty() {
        return 0;
    }

    let Some(adapter) = resolve_adapter(agent_name) else {
        return 0;
    };

    let pane = tmux_pane();
    if pane.is_empty() {
        return 0;
    }

    let input = read_stdin_json();
    let Some(event) = adapter.parse(event_name, &input) else {
        return 0;
    };

    handle_event(&pane, agent_name, event)
}

// ─── event handler ──────────────────────────────────────────────────────────

fn handle_event(pane: &str, agent_name: &str, event: AgentEvent) -> i32 {
    match event {
        AgentEvent::SessionStart {
            agent,
            cwd,
            permission_mode,
            source,
            session_id,
            ..
        } => handlers::on_session_start(
            pane,
            &context::make_ctx(&agent, &cwd, &permission_mode, &session_id),
            &source,
        ),
        AgentEvent::SessionEnd { end_reason } => {
            handlers::on_session_end(pane, agent_name, &end_reason)
        }
        AgentEvent::UserPromptSubmit {
            agent,
            cwd,
            permission_mode,
            prompt,
            session_id,
            ..
        } => handlers::on_user_prompt_submit(
            pane,
            &context::make_ctx(&agent, &cwd, &permission_mode, &session_id),
            &prompt,
        ),
        AgentEvent::Notification {
            agent,
            cwd,
            permission_mode,
            wait_reason,
            meta_only,
            session_id,
            ..
        } => handlers::on_notification(
            pane,
            &context::make_ctx(&agent, &cwd, &permission_mode, &session_id),
            &wait_reason,
            meta_only,
        ),
        AgentEvent::Stop {
            agent,
            cwd,
            permission_mode,
            last_message,
            response,
            session_id,
            ..
        } => handlers::on_stop(
            pane,
            &context::make_ctx(&agent, &cwd, &permission_mode, &session_id),
            &last_message,
            response.as_deref(),
        ),
        AgentEvent::StopFailure {
            agent,
            cwd,
            permission_mode,
            error,
            session_id,
            ..
        } => handlers::on_stop_failure(
            pane,
            &context::make_ctx(&agent, &cwd, &permission_mode, &session_id),
            &error,
        ),
        AgentEvent::SubagentStart {
            agent_type,
            agent_id,
        } => handlers::on_subagent_start(pane, &agent_type, agent_id.as_deref()),
        AgentEvent::SubagentStop { agent_id, .. } => {
            handlers::on_subagent_stop(pane, agent_id.as_deref())
        }
        AgentEvent::ActivityLog {
            tool_name,
            tool_input,
            tool_response,
        } => activity::handle_activity_log(pane, &tool_name, &tool_input, &tool_response),
        AgentEvent::PermissionDenied {
            agent,
            cwd,
            permission_mode,
            session_id,
            ..
        } => handlers::on_permission_denied(
            pane,
            &context::make_ctx(&agent, &cwd, &permission_mode, &session_id),
        ),
        AgentEvent::CwdChanged {
            cwd, session_id, ..
        } => {
            sync_pane_location(pane, &cwd, &session_id);
            0
        }
        AgentEvent::TaskCreated { .. } => 0,
        AgentEvent::TaskCompleted {
            task_id,
            task_subject,
        } => {
            let _ = (agent_name, task_id, task_subject);
            super::set_attention(pane, "notification");
            0
        }
        AgentEvent::TeammateIdle {
            teammate_name,
            idle_reason,
            ..
        } => handlers::on_teammate_idle(pane, &teammate_name, &idle_reason),
    }
}
