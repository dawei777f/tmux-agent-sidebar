//! Read Claude Code's plugin install registry to detect whether
//! tmux-agent-sidebar has been installed as a Claude Code plugin.
//!
//! Claude Code maintains `~/.claude/plugins/installed_plugins.json`.
//! The sidebar reads it once at startup so the TUI can suppress the
//! manual "missing hooks" notice for Claude when the plugin owns the
//! hook wiring. No update or version checks are performed here.

use std::fs;
use std::path::{Path, PathBuf};

const PLUGIN_NAME: &str = "tmux-agent-sidebar";
const RESIDUAL_HOOK_NEEDLE: &str = "tmux-agent-sidebar/hook.sh";

/// Lifetime state of the Claude Code plugin install, resolved once at
/// sidebar startup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaudePluginStatus {
    /// Whether `tmux-agent-sidebar` is recorded in Claude Code's
    /// `installed_plugins.json`. Derived from the presence of a matching
    /// entry with a non-empty install path.
    pub installed: bool,
}

/// Resolve the Claude plugin install status from the user's
/// `~/.claude/plugins/installed_plugins.json`. Registry-level failure
/// paths (missing registry, unreadable file, malformed JSON, missing
/// install path) degrade to "plugin not installed".
pub fn installed_plugin_status() -> ClaudePluginStatus {
    let Some(registry) = claude_plugins_registry_path() else {
        return ClaudePluginStatus::default();
    };
    installed_plugin_status_from(&registry)
}

fn claude_plugins_registry_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".claude/plugins/installed_plugins.json"))
}

fn installed_plugin_status_from(registry_path: &Path) -> ClaudePluginStatus {
    ClaudePluginStatus {
        installed: installed_plugin_install_path_from(registry_path).is_some(),
    }
}

/// Extract the recorded `installPath` for the `tmux-agent-sidebar`
/// plugin, preferring the first entry that carries a non-empty path.
/// Returns `None` when the plugin is not installed or the registry is
/// malformed/unreadable.
fn installed_plugin_install_path_from(path: &Path) -> Option<PathBuf> {
    let raw = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let plugins = json.get("plugins")?.as_object()?;
    for (key, installs) in plugins {
        // Match by plugin name (the part before `@`), not by full key.
        let name = key.split('@').next().unwrap_or("");
        if name != PLUGIN_NAME {
            continue;
        }
        if let Some(install_path) = installs.as_array().and_then(|installs| {
            installs.iter().find_map(|install| {
                install
                    .get("installPath")
                    .and_then(|v| v.as_str())
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
            })
        }) {
            return Some(install_path);
        }
    }
    None
}

/// Whether the user's `~/.claude/settings.json` still contains residual
/// `tmux-agent-sidebar/hook.sh` entries from the legacy manual setup.
///
/// When this returns `true` AND the plugin is also installed, every hook
/// fires twice, so the notices popup asks the user to clean up the
/// duplicate manual entries. Resolved once at sidebar startup.
pub fn claude_settings_has_residual_hooks() -> bool {
    claude_settings_has_residual_hooks_at(&claude_settings_path())
}

fn claude_settings_path() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".claude/settings.json")
}

fn claude_settings_has_residual_hooks_at(path: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    let Some(hooks) = json.get("hooks").and_then(|v| v.as_object()) else {
        return false;
    };
    hooks
        .values()
        .filter_map(|v| v.as_array())
        .flatten()
        .filter_map(|matcher_obj| matcher_obj.get("hooks").and_then(|h| h.as_array()))
        .flatten()
        .filter_map(|action| action.get("command").and_then(|c| c.as_str()))
        .any(|cmd| cmd.contains(RESIDUAL_HOOK_NEEDLE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_registry(label: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("tmux-as-installed-plugins-{label}-{id}.json"));
        let _ = fs::remove_file(&path);
        path
    }

    fn unique_settings(label: &str) -> PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("tmux-as-claude-settings-{label}-{id}.json"));
        let _ = fs::remove_file(&path);
        path
    }

    fn write_registry(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
    }

    #[test]
    fn returns_install_path_when_plugin_is_installed() {
        let path = unique_registry("installed");
        write_registry(
            &path,
            r#"{
                "plugins": {
                    "tmux-agent-sidebar@hiroppy": [
                        {"scope":"user","installPath":"/opt/claude-cache/tmux-agent-sidebar"}
                    ]
                }
            }"#,
        );
        assert_eq!(
            installed_plugin_install_path_from(&path),
            Some(PathBuf::from("/opt/claude-cache/tmux-agent-sidebar"))
        );
    }

    #[test]
    fn installed_status_is_true_when_install_path_exists() {
        let path = unique_registry("status-installed");
        write_registry(
            &path,
            r#"{
                "plugins": {
                    "tmux-agent-sidebar@hiroppy": [
                        {"scope":"user","installPath":"/cache/tmux-agent-sidebar"}
                    ]
                }
            }"#,
        );
        assert_eq!(
            installed_plugin_status_from(&path),
            ClaudePluginStatus { installed: true }
        );
    }

    #[test]
    fn returns_none_when_plugin_not_in_registry() {
        let path = unique_registry("not-installed");
        write_registry(
            &path,
            r#"{
                "plugins": {
                    "code-review@anthropic": [
                        {"scope":"user","installPath":"/x"}
                    ]
                }
            }"#,
        );
        assert_eq!(installed_plugin_install_path_from(&path), None);
    }

    #[test]
    fn installed_status_is_false_when_registry_missing() {
        let path = unique_registry("missing");
        assert_eq!(
            installed_plugin_status_from(&path),
            ClaudePluginStatus::default()
        );
    }

    #[test]
    fn returns_none_when_registry_is_garbage() {
        let path = unique_registry("garbage");
        write_registry(&path, "not-json");
        assert_eq!(installed_plugin_install_path_from(&path), None);
    }

    #[test]
    fn matches_plugin_regardless_of_marketplace_suffix() {
        let path = unique_registry("different-marketplace");
        write_registry(
            &path,
            r#"{
                "plugins": {
                    "tmux-agent-sidebar@somewhere-else": [
                        {"scope":"user","installPath":"/tmp/elsewhere"}
                    ]
                }
            }"#,
        );
        assert_eq!(
            installed_plugin_install_path_from(&path),
            Some(PathBuf::from("/tmp/elsewhere"))
        );
    }

    #[test]
    fn returns_none_when_install_path_is_empty_string() {
        let path = unique_registry("empty-install-path");
        write_registry(
            &path,
            r#"{
                "plugins": {
                    "tmux-agent-sidebar@hiroppy": [
                        {"scope":"user","installPath":""}
                    ]
                }
            }"#,
        );
        assert_eq!(installed_plugin_install_path_from(&path), None);
    }

    #[test]
    fn returns_first_non_empty_install_path_across_multiple_installs() {
        let path = unique_registry("multiple-installs");
        write_registry(
            &path,
            r#"{
                "plugins": {
                    "tmux-agent-sidebar@hiroppy": [
                        {"scope":"user","installPath":""},
                        {"scope":"project","installPath":"/project/tmux-agent-sidebar"}
                    ]
                }
            }"#,
        );
        assert_eq!(
            installed_plugin_install_path_from(&path),
            Some(PathBuf::from("/project/tmux-agent-sidebar"))
        );
    }

    #[test]
    fn residual_hooks_false_when_settings_file_missing() {
        let path = unique_settings("missing");
        assert!(!claude_settings_has_residual_hooks_at(&path));
    }

    #[test]
    fn residual_hooks_false_when_settings_file_has_no_hooks_object() {
        let path = unique_settings("no-hooks-object");
        fs::write(&path, r#"{"theme":"dark"}"#).unwrap();
        assert!(!claude_settings_has_residual_hooks_at(&path));
    }

    #[test]
    fn residual_hooks_false_when_no_command_mentions_tmux_agent_sidebar() {
        let path = unique_settings("clean");
        fs::write(
            &path,
            r#"{
                "hooks": {
                    "SessionStart": [
                        {"matcher":"","hooks":[{"type":"command","command":"echo hi"}]}
                    ]
                }
            }"#,
        )
        .unwrap();
        assert!(!claude_settings_has_residual_hooks_at(&path));
    }

    #[test]
    fn residual_hooks_true_when_legacy_command_present() {
        let path = unique_settings("residual");
        fs::write(
            &path,
            r#"{
                "hooks": {
                    "SessionStart": [
                        {"matcher":"","hooks":[{"type":"command","command":"bash ~/.rmux/plugins/tmux-agent-sidebar/hook.sh claude session-start"}]}
                    ],
                    "PostToolUse": [
                        {"matcher":"","hooks":[{"type":"command","command":"bash ~/.rmux/plugins/tmux-agent-sidebar/hook.sh claude activity-log"}]}
                    ]
                }
            }"#,
        )
        .unwrap();
        assert!(claude_settings_has_residual_hooks_at(&path));
    }

    #[test]
    fn residual_hooks_true_when_only_one_legacy_command_present() {
        let path = unique_settings("residual-one");
        fs::write(
            &path,
            r#"{
                "hooks": {
                    "Stop": [
                        {"matcher":"","hooks":[{"type":"command","command":"bash /custom/path/tmux-agent-sidebar/hook.sh claude stop"}]}
                    ]
                }
            }"#,
        )
        .unwrap();
        assert!(claude_settings_has_residual_hooks_at(&path));
    }

    #[test]
    fn residual_hooks_false_when_settings_is_garbage() {
        let path = unique_settings("garbage");
        fs::write(&path, "not-json").unwrap();
        assert!(!claude_settings_has_residual_hooks_at(&path));
    }
}
