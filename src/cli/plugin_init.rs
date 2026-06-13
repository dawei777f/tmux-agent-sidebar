use rmux_proto::HookName;

use crate::tmux;

const AGENT_SIDEBAR_BIN: &str = "@agent_sidebar_bin";
const PLUGIN_HOOK_NEEDLE: &str = "tmux-agent-sidebar";
const LEGACY_HOOK_NEEDLES: &[&str] = &[PLUGIN_HOOK_NEEDLE, "@agent_sidebar_bin", "tmux list-panes"];

pub(crate) fn cmd_plugin_init(args: &[String]) -> i32 {
    let Some(binary) = args.first() else {
        eprintln!("plugin-init: missing binary path");
        return 2;
    };

    match install(binary) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("plugin-init: {err}");
            1
        }
    }
}

fn install(binary: &str) -> Result<(), String> {
    tmux::set_global_option(AGENT_SIDEBAR_BIN, binary)?;
    seed_defaults()?;
    bind_keys(binary)?;
    install_hooks(binary)
}

fn seed_defaults() -> Result<(), String> {
    for (key, value) in [
        (tmux::SIDEBAR_WIDTH, "15%"),
        (tmux::SIDEBAR_POSITION, "left"),
        (tmux::SIDEBAR_BOTTOM_HEIGHT, "20"),
        (tmux::SIDEBAR_AUTO_CREATE, "on"),
        (tmux::SIDEBAR_PET, "off"),
        (tmux::SIDEBAR_KEY, "e"),
        (tmux::SIDEBAR_KEY_ALL, "E"),
    ] {
        if tmux::get_global_option(key)?.is_none_or(|existing| existing.is_empty()) {
            tmux::set_global_option(key, value)?;
        }
    }
    Ok(())
}

fn bind_keys(binary: &str) -> Result<(), String> {
    let toggle_key = tmux::get_option(tmux::SIDEBAR_KEY).unwrap_or_else(|| "e".to_string());
    let toggle_all_key = tmux::get_option(tmux::SIDEBAR_KEY_ALL).unwrap_or_else(|| "E".to_string());

    tmux::bind_prefix_key(
        &toggle_key,
        vec![
            "run-shell".to_string(),
            shell_command(binary, &["toggle", "#{window_id}", "#{pane_current_path}"]),
        ],
    )?;
    tmux::bind_prefix_key(
        &toggle_all_key,
        vec![
            "run-shell".to_string(),
            shell_command(binary, &["toggle-all"]),
        ],
    )
}

fn install_hooks(binary: &str) -> Result<(), String> {
    let auto_create = tmux::get_option(tmux::SIDEBAR_AUTO_CREATE)
        .map(|value| !value.eq_ignore_ascii_case("off"))
        .unwrap_or(true);

    replace_global_hook_matching_any(
        HookName::AfterNewWindow,
        auto_create.then(|| {
            run_shell_command(&shell_command(
                binary,
                &[
                    "toggle",
                    "--create-only",
                    "#{window_id}",
                    "#{pane_current_path}",
                ],
            ))
        }),
    )?;
    replace_global_hook_matching_any(
        HookName::AfterSelectPane,
        Some(run_shell_command(&shell_command(
            binary,
            &["notify-focus", "#{window_id}"],
        ))),
    )?;
    replace_global_hook_matching_any(
        HookName::AfterSelectWindow,
        Some(run_shell_command(&shell_command(
            binary,
            &["notify-focus", "#{window_id}"],
        ))),
    )?;
    replace_global_hook_matching_any(
        HookName::PaneExited,
        Some(run_shell_command(&shell_command(
            binary,
            &["auto-close", "#{window_id}"],
        ))),
    )
}

fn replace_global_hook_matching_any(hook: HookName, command: Option<String>) -> Result<(), String> {
    for needle in LEGACY_HOOK_NEEDLES {
        tmux::replace_global_hook_matching(hook, needle, None)?;
    }
    if let Some(command) = command {
        tmux::replace_global_hook_matching(hook, PLUGIN_HOOK_NEEDLE, Some(command))?;
    }
    Ok(())
}

fn shell_command(binary: &str, args: &[&str]) -> String {
    let mut command = crate::cli::setup::shell_quote(binary);
    for arg in args {
        command.push(' ');
        if arg.starts_with("#{") {
            command.push('"');
            command.push_str(arg);
            command.push('"');
        } else {
            command.push_str(&crate::cli::setup::shell_quote(arg));
        }
    }
    command
}

fn run_shell_command(command: &str) -> String {
    format!("run-shell {}", rmux_command_quote(command))
}

fn rmux_command_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '\\' | '"' => {
                quoted.push('\\');
                quoted.push(ch);
            }
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}
