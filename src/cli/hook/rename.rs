use std::process::{Command, Stdio};

use crate::llm::{config::LlmConfig, store};
use crate::tmux;

use super::context::AgentContext;

/// Spawn a background `rename-session --auto` subprocess when the
/// local-LLM rename feature is enabled and this session does not yet
/// have a generated name. Fires once per session from the first `Stop`
/// hook, mirroring ChatGPT/Claude title-generation timing.
///
/// This is fire-and-forget so the hook process returns immediately and
/// does not block the agent. Errors are swallowed and recorded in the
/// subprocess's own log file.
pub(super) fn maybe_spawn_rename(pane: &str, ctx: &AgentContext<'_>, last_message: &str) {
    let Some(session_id) = ctx.session_id.as_deref().filter(|s| !s.is_empty()) else {
        return;
    };

    let Some(cfg) = LlmConfig::from_tmux_options(&tmux::get_all_global_options()) else {
        return;
    };
    if !cfg.auto_rename {
        return;
    }

    if store::read(session_id).is_some() {
        return;
    }

    let Ok(exe) = std::env::current_exe() else {
        return;
    };

    let mut cmd = Command::new(exe);
    cmd.arg("rename-session")
        .arg("--session")
        .arg(session_id)
        .arg("--pane")
        .arg(pane)
        .arg("--auto");
    if !last_message.is_empty() {
        cmd.arg("--last-message").arg(last_message);
    }
    let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}
