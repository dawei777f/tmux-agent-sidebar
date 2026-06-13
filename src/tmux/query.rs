use super::commands::list_panes_formatted;
use super::options::{
    PANE_AGENT, PANE_ATTENTION, PANE_BG_CMD, PANE_CWD, PANE_NAME, PANE_PERMISSION_MODE,
    PANE_PROMPT, PANE_PROMPT_SOURCE, PANE_ROLE, PANE_SESSION_ID, PANE_STARTED_AT, PANE_STATUS,
    PANE_SUBAGENTS, PANE_WAIT_REASON,
};
use super::types::{AgentType, PaneInfo, PaneStatus, PermissionMode, SessionInfo, WindowInfo};

// Field indices in rmux list-panes formatted output. Keep in lock-step with
// the `pane_format()` field list. When adding a new field, update both
// this module and the format string together.
mod session_line_field {
    pub const SESSION_NAME: usize = 0;
    pub const WINDOW_ID: usize = 1;
    pub const WINDOW_NAME: usize = 3;
    pub const WINDOW_ACTIVE: usize = 4;
    pub const AUTOMATIC_RENAME: usize = 5;
    /// Index where the per-pane field suffix consumed by `parse_pane_line` begins.
    pub const PANE_LINE_OFFSET: usize = 6;
    /// Minimum number of fields a valid `pane_format()` line must contain.
    pub const MIN_FIELDS: usize = 24;
}

// Indices into the pane-line suffix that `parse_pane_line` operates on.
// Each value = (absolute index in the full format string) - 6, because
// `build_session_hierarchy` strips the leading 6 window-level fields
// before joining the remainder back into `pane_line`.
pub(super) mod pane_line_field {
    pub const PANE_ACTIVE: usize = 0; // absolute 6
    pub const PANE_STATUS: usize = 1; // absolute 7  (@pane_status)
    pub const PANE_ATTENTION: usize = 2; // absolute 8  (@pane_attention)
    pub const AGENT: usize = 3; // absolute 9  (@pane_agent)
    pub const PANE_CURRENT_PATH: usize = 5; // absolute 11 (pane_current_path)
    pub const PANE_ROLE: usize = 6; // absolute 12 (@pane_role)
    pub const PANE_ID: usize = 7; // absolute 13
    pub const PROMPT: usize = 8; // absolute 14 (@pane_prompt)
    pub const PROMPT_SOURCE: usize = 9; // absolute 15 (@pane_prompt_source)
    pub const STARTED_AT: usize = 10; // absolute 16 (@pane_started_at)
    pub const WAIT_REASON: usize = 11; // absolute 17 (@pane_wait_reason)
    pub const PANE_PID: usize = 12; // absolute 18
    pub const SUBAGENTS: usize = 13; // absolute 19 (@pane_subagents)
    pub const PANE_CWD: usize = 14; // absolute 20 (@pane_cwd)
    pub const PERMISSION_MODE: usize = 15; // absolute 21 (@pane_permission_mode)
    pub const SESSION_ID: usize = 16; // absolute 22 (@pane_session_id)
    pub const BG_CMD: usize = 17; // absolute 23 (@pane_bg_cmd)
    /// Minimum number of fields the pane-line suffix must contain.
    /// Equals `session_line_field::MIN_FIELDS - PANE_LINE_OFFSET`.
    pub const MIN_FIELDS: usize = 18;
}

/// Build the rmux `list-panes -F` format used by [`query_sessions`].
/// Every field is quoted with `#{q:...}` so embedded pipes in user content
/// survive the split.
fn pane_format() -> String {
    [
        q("session_name"),
        q("window_id"),
        q("window_index"),
        q("window_name"),
        q("window_active"),
        q("automatic-rename"),
        q("pane_active"),
        q(PANE_STATUS),
        q(PANE_ATTENTION),
        q(PANE_AGENT),
        q(PANE_NAME),
        q("pane_current_path"),
        q(PANE_ROLE),
        q("pane_id"),
        q(PANE_PROMPT),
        q(PANE_PROMPT_SOURCE),
        q(PANE_STARTED_AT),
        q(PANE_WAIT_REASON),
        q("pane_pid"),
        q(PANE_SUBAGENTS),
        q(PANE_CWD),
        q(PANE_PERMISSION_MODE),
        q(PANE_SESSION_ID),
        q(PANE_BG_CMD),
    ]
    .join("|")
}

fn q(field: &str) -> String {
    format!("#{{q:{field}}}")
}

type SessionMap = indexmap::IndexMap<String, indexmap::IndexMap<String, WindowInfo>>;

/// Query all sessions, windows, and panes in a single rmux list-panes RPC
/// instead of N+1 multiplexer RPC requests.
pub fn query_sessions() -> Vec<SessionInfo> {
    let pane_format = pane_format();
    let all_panes_output = match list_panes_formatted(None, true, &pane_format) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let sessions_map = build_session_hierarchy(&all_panes_output);
    finalize_sessions(sessions_map)
}

/// Parse the raw rmux list-panes output into an indexed session→window→pane
/// hierarchy.
fn build_session_hierarchy(all_panes_output: &str) -> SessionMap {
    let mut sessions_map: SessionMap = indexmap::IndexMap::new();

    for line in all_panes_output.lines() {
        let parts = split_tmux_fields(line, '|');
        if parts.len() < session_line_field::MIN_FIELDS {
            continue;
        }

        let session_name = parts[session_line_field::SESSION_NAME].as_str();
        let window_id = parts[session_line_field::WINDOW_ID].as_str();
        // Pass the unescaped pane fields directly instead of re-joining
        // with `|` and re-splitting, which would turn any literal pipe
        // inside a pane field (cwd, prompt, background command) back into a field
        // separator and shift every downstream index.
        let pane_fields = &parts[session_line_field::PANE_LINE_OFFSET..];

        let sessions_entry = sessions_map.entry(session_name.to_string()).or_default();

        let window = sessions_entry
            .entry(window_id.to_string())
            .or_insert_with(|| WindowInfo {
                window_id: window_id.to_string(),
                window_name: parts[session_line_field::WINDOW_NAME].to_string(),
                window_active: parts[session_line_field::WINDOW_ACTIVE] == "1",
                auto_rename: parts[session_line_field::AUTOMATIC_RENAME] == "1",
                panes: Vec::new(),
            });

        if let Some(pane) = parse_pane_fields(pane_fields) {
            window.panes.push(pane);
        }
    }

    sessions_map
}

/// Flatten the session→window hierarchy into a `Vec<SessionInfo>`, dropping
/// any windows whose `parse_pane_line` filtering left them empty, and any
/// sessions whose windows are all empty as a result.
fn finalize_sessions(sessions_map: SessionMap) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    for (session_name, windows) in sessions_map {
        let windows: Vec<WindowInfo> = windows
            .into_values()
            .filter(|w| !w.panes.is_empty())
            .collect();
        if !windows.is_empty() {
            sessions.push(SessionInfo {
                session_name,
                windows,
            });
        }
    }
    sessions
}

/// Parse a single pane line from rmux formatted pane output.
/// Returns None if the line has too few fields, is a sidebar, or has no agent.
/// Thin wrapper used by the unit tests, which still construct a raw
/// `|`-joined fixture line. Production callers go through
/// `parse_pane_fields` directly to avoid re-joining and re-splitting fields
/// that may themselves contain literal `|` characters (cwd,
/// prompt, background command) — see `build_session_hierarchy`.
#[cfg(test)]
pub(crate) fn parse_pane_line(line: &str) -> Option<PaneInfo> {
    let parts = split_tmux_fields(line, '|');
    parse_pane_fields(&parts)
}

fn parse_pane_fields(parts: &[String]) -> Option<PaneInfo> {
    if parts.len() < pane_line_field::MIN_FIELDS {
        return None;
    }

    if parts[pane_line_field::PANE_ROLE] == "sidebar" {
        return None;
    }

    let agent = AgentType::from_label(&parts[pane_line_field::AGENT])?;
    let pane_pid: Option<u32> = parts[pane_line_field::PANE_PID].parse().ok();

    // Prefer @pane_cwd (set by hook from agent's cwd) over pane_current_path
    let pane_cwd = &parts[pane_line_field::PANE_CWD];
    let path = if !pane_cwd.is_empty() {
        pane_cwd.to_string()
    } else {
        parts[pane_line_field::PANE_CURRENT_PATH].to_string()
    };

    // Claude: read permission_mode from hook-set tmux variable.
    // Codex / OpenCode: no permission_mode in hooks, keep the default.
    let permission_mode = if agent == AgentType::Claude {
        PermissionMode::from_label(&parts[pane_line_field::PERMISSION_MODE])
    } else {
        PermissionMode::Default
    };

    let prompt_source = &parts[pane_line_field::PROMPT_SOURCE];
    let prompt_is_response = prompt_source == "response";

    // Sanitize prompt: replace pipes/newlines, filter system-injected messages, truncate
    let prompt = sanitize_prompt(&parts[pane_line_field::PROMPT]);

    let session_id = if parts[pane_line_field::SESSION_ID].is_empty() {
        None
    } else {
        Some(parts[pane_line_field::SESSION_ID].to_string())
    };

    Some(PaneInfo {
        pane_active: parts[pane_line_field::PANE_ACTIVE] == "1",
        status: PaneStatus::from_label(&parts[pane_line_field::PANE_STATUS]),
        attention: !parts[pane_line_field::PANE_ATTENTION].is_empty(),
        agent,
        path,
        current_command: String::new(),
        pane_id: parts[pane_line_field::PANE_ID].to_string(),
        prompt,
        prompt_is_response,
        started_at: parts[pane_line_field::STARTED_AT].parse().ok(),
        wait_reason: parts[pane_line_field::WAIT_REASON].to_string(),
        permission_mode,
        subagents: parse_subagents(&parts[pane_line_field::SUBAGENTS]),
        pane_pid,
        session_id,
        session_name: String::new(),
        bg_shell_cmd: {
            let raw = &parts[pane_line_field::BG_CMD];
            if raw.is_empty() {
                None
            } else {
                Some(raw.to_string())
            }
        },
    })
}

/// Sanitize prompt text from tmux variable so it's safe for display.
fn sanitize_prompt(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    // Filter known system-injected messages. Avoid the old broad angle-bracket
    // check so legitimate prompts containing comparisons or code snippets
    // still render.
    if raw.contains("<task-notification>")
        || raw.contains("<system-reminder>")
        || raw.contains("<task-status>")
    {
        return String::new();
    }
    if raw.chars().count() > 200 {
        raw.chars().take(200).collect()
    } else {
        raw.to_string()
    }
}

/// Parse subagent list from tmux variable.
/// Format: comma-separated "type" entries, e.g. "Explore,Explore,Plan"
/// Parse the comma-separated `@pane_subagents` value into display strings.
///
/// Each entry is either `agent_type` (legacy) or `agent_type:agent_id`
/// (current). When an `agent_id` is present, the entry is rendered as
/// `"agent_type #<id-prefix>"` where `<id-prefix>` is the first 4 characters
/// of the id — stable per instance, so the UI label does not shift when
/// sibling subagents stop. The `#` embedding is recognized by the `#`-based
/// numbering branch in `subagent_rows`, which keeps it verbatim.
fn parse_subagents(raw: &str) -> Vec<String> {
    const ID_PREFIX_LEN: usize = 4;
    if raw.is_empty() {
        return vec![];
    }
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|entry| match entry.split_once(':') {
            Some((ty, id)) if !id.is_empty() => {
                let prefix: String = id.chars().take(ID_PREFIX_LEN).collect();
                format!("{} #{}", ty, prefix)
            }
            _ => entry.to_string(),
        })
        .collect()
}

/// Split a tmux format line while honoring tmux `#{q:...}` backslash escapes.
fn split_tmux_fields(line: &str, delimiter: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if ch == delimiter {
            fields.push(current);
            current = String::new();
            continue;
        }

        current.push(ch);
    }

    if escaped {
        current.push('\\');
    }

    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── sanitize_prompt tests ──────────────────────────────────────

    #[test]
    fn sanitize_prompt_filters_system_injected() {
        assert_eq!(
            sanitize_prompt("<system-reminder>noise</system-reminder>"),
            ""
        );
        assert_eq!(
            sanitize_prompt("hello <task-notification>abc</task-notification> world"),
            ""
        );
    }

    #[test]
    fn sanitize_prompt_passes_normal_text() {
        assert_eq!(sanitize_prompt("fix the bug"), "fix the bug");
    }

    #[test]
    fn sanitize_prompt_keeps_legitimate_angle_brackets() {
        assert_eq!(sanitize_prompt("1 < 2 and 3 > 1"), "1 < 2 and 3 > 1");
    }

    #[test]
    fn sanitize_prompt_truncates_long_text() {
        let long = "a".repeat(300);
        let result = sanitize_prompt(&long);
        assert_eq!(result.chars().count(), 200);
    }

    #[test]
    fn sanitize_prompt_empty() {
        assert_eq!(sanitize_prompt(""), "");
    }

    // ─── parse_subagents tests ──────────────────────────────────────

    #[test]
    fn parse_subagents_empty() {
        assert_eq!(parse_subagents(""), Vec::<String>::new());
    }

    #[test]
    fn parse_subagents_single() {
        assert_eq!(parse_subagents("Explore"), vec!["Explore"]);
    }

    #[test]
    fn parse_subagents_multiple() {
        assert_eq!(
            parse_subagents("Explore,Plan,Bash"),
            vec!["Explore", "Plan", "Bash"]
        );
    }

    #[test]
    fn parse_subagents_duplicates() {
        assert_eq!(
            parse_subagents("Explore,Explore,Plan"),
            vec!["Explore", "Explore", "Plan"]
        );
    }

    #[test]
    fn parse_subagents_renders_id_prefix() {
        // Current format: `type:id`. The id prefix is used as a stable
        // `#<prefix>` label so surviving siblings do not renumber when
        // another subagent stops.
        assert_eq!(
            parse_subagents("Explore:sub123456,Plan:abc987654"),
            vec!["Explore #sub1", "Plan #abc9"]
        );
    }

    #[test]
    fn parse_subagents_id_prefix_distinguishes_parallel_same_type() {
        // Two subagents of the same type get distinct labels from their ids,
        // which is the whole point of id-based tagging.
        assert_eq!(
            parse_subagents("Explore:aaaa1111,Explore:bbbb2222"),
            vec!["Explore #aaaa", "Explore #bbbb"]
        );
    }

    #[test]
    fn parse_subagents_id_shorter_than_prefix_len_uses_full_id() {
        // Short ids (e.g. test fixtures like "s1") render in full rather
        // than being padded or truncated to nothing.
        assert_eq!(parse_subagents("Plan:s1"), vec!["Plan #s1"]);
    }

    #[test]
    fn parse_subagents_legacy_without_id_renders_type_only() {
        // Stale entry written before id tracking (or by an older build)
        // falls back to the bare type name.
        assert_eq!(
            parse_subagents("Explore,Plan:sub-999"),
            vec!["Explore", "Plan #sub-"]
        );
    }

    // ─── parse_pane_line tests ──────────────────────────────────────

    fn make_pane_line(fields: &[&str]) -> String {
        fields.join("|")
    }

    fn full_fields() -> Vec<&'static str> {
        vec![
            "1",                  // 0: pane_active
            "running",            // 1: @pane_status
            "",                   // 2: @pane_attention
            "claude",             // 3: @pane_agent
            "my-agent",           // 4: @pane_name
            "/home/user/project", // 5: pane_current_path
            "",                   // 6: @pane_role
            "%1",                 // 7: pane_id
            "fix the bug",        // 8: @pane_prompt
            "user",               // 9: @pane_prompt_source
            "1700000000",         // 10: @pane_started_at
            "",                   // 11: @pane_wait_reason
            "12345",              // 12: pane_pid
            "Explore,Plan",       // 13: @pane_subagents
            "/custom/cwd",        // 14: @pane_cwd
            "auto",               // 15: @pane_permission_mode
            "",                   // 16: @pane_session_id
            "",                   // 17: @pane_bg_cmd
        ]
    }

    #[test]
    fn parse_pane_line_full_fields() {
        let line = make_pane_line(&full_fields());
        let pane = parse_pane_line(&line).expect("should parse 22 fields");
        assert!(pane.pane_active);
        assert_eq!(pane.status, PaneStatus::Running);
        assert_eq!(pane.agent, AgentType::Claude);
        assert_eq!(pane.path, "/custom/cwd"); // pane_cwd preferred
        assert_eq!(pane.current_command, "");
        assert_eq!(pane.pane_id, "%1");
        assert_eq!(pane.prompt, "fix the bug");
        assert!(!pane.prompt_is_response);
        assert_eq!(pane.started_at, Some(1700000000));
        assert_eq!(pane.pane_pid, Some(12345));
        assert_eq!(pane.subagents, vec!["Explore", "Plan"]);
        assert_eq!(pane.permission_mode, PermissionMode::Auto);
    }

    #[test]
    fn parse_pane_line_response_prompt_source() {
        let mut fields = full_fields();
        fields[9] = "response"; // @pane_prompt_source
        let line = make_pane_line(&fields);
        let pane = parse_pane_line(&line).unwrap();
        assert!(pane.prompt_is_response);
    }

    #[test]
    fn parse_pane_line_rejects_fewer_than_min_fields() {
        // Only 15 fields — should be rejected
        let fields_15 =
            "1|running||claude|name|/path|fish||%1|prompt|1700000000||12345|Explore|/cwd";
        assert!(
            parse_pane_line(fields_15).is_none(),
            "15 fields should be rejected"
        );

        // 17 fields — still rejected (need 18 including @pane_bg_cmd).
        let fields_17 =
            "1|running||claude|name|/path||%1|prompt|user|1700000000||12345|Explore|/cwd|auto|";
        assert!(
            parse_pane_line(fields_17).is_none(),
            "17 fields should be rejected"
        );
    }

    #[test]
    fn parse_pane_line_reads_bg_cmd_field() {
        let mut fields = full_fields();
        fields[pane_line_field::BG_CMD] = "cargo build --release";
        let pane = parse_pane_line(&make_pane_line(&fields)).unwrap();
        assert_eq!(
            pane.bg_shell_cmd.as_deref(),
            Some("cargo build --release"),
            "bg_shell_cmd should surface the @pane_bg_cmd value"
        );

        fields[pane_line_field::BG_CMD] = "";
        let pane = parse_pane_line(&make_pane_line(&fields)).unwrap();
        assert!(
            pane.bg_shell_cmd.is_none(),
            "empty @pane_bg_cmd should parse as None"
        );
    }

    #[test]
    fn parse_pane_line_rejects_sidebar_role() {
        let mut fields = full_fields();
        fields[pane_line_field::PANE_ROLE] = "sidebar";
        let line = make_pane_line(&fields);
        assert!(
            parse_pane_line(&line).is_none(),
            "sidebar role should be filtered out"
        );
    }

    #[test]
    fn parse_pane_line_rejects_unknown_agent() {
        let mut fields = full_fields();
        fields[3] = ""; // no agent type
        let line = make_pane_line(&fields);
        assert!(
            parse_pane_line(&line).is_none(),
            "empty agent should be rejected"
        );
    }

    #[test]
    fn parse_pane_line_falls_back_to_pane_current_path() {
        let mut fields = full_fields();
        fields[pane_line_field::PANE_CWD] = ""; // empty pane_cwd
        let line = make_pane_line(&fields);
        let pane = parse_pane_line(&line).unwrap();
        assert_eq!(
            pane.path, "/home/user/project",
            "should fall back to pane_current_path when pane_cwd is empty"
        );
    }

    #[test]
    fn parse_pane_line_preserves_pipe_in_path() {
        let mut fields = full_fields();
        fields[pane_line_field::PANE_CURRENT_PATH] = "/home/user/a\\|b";
        fields[pane_line_field::PANE_CWD] = "";
        let line = make_pane_line(&fields);
        let pane = parse_pane_line(&line).unwrap();
        assert_eq!(pane.path, "/home/user/a|b");
    }

    #[test]
    fn split_tmux_fields_unescapes_delimiter() {
        let fields = split_tmux_fields("one|two\\|still-two|three", '|');
        assert_eq!(fields, vec!["one", "two|still-two", "three"]);
    }

    #[test]
    fn parse_pane_line_codex_ignores_permission_mode_field() {
        let mut fields = full_fields();
        fields[pane_line_field::AGENT] = "codex";
        fields[pane_line_field::PERMISSION_MODE] = "auto"; // should be ignored for codex
        let line = make_pane_line(&fields);
        let pane = parse_pane_line(&line).unwrap();
        assert_eq!(
            pane.permission_mode,
            PermissionMode::Default,
            "codex should not read permission_mode from tmux variable"
        );
    }

    // ─── finalize_sessions ─────────────────────────────────────────

    #[test]
    fn finalize_sessions_drops_windows_with_no_panes() {
        // Regression: build_session_hierarchy() creates a WindowInfo as
        // soon as it sees a tmux row, but parse_pane_line() may then
        // reject every pane in that window (sidebar / shell / unknown).
        // finalize_sessions must filter out the resulting empty windows
        // so downstream code never has to special-case them.
        let mut sessions_map: SessionMap = indexmap::IndexMap::new();
        let entry = sessions_map.entry("main".to_string()).or_default();
        entry.insert(
            "@1".to_string(),
            WindowInfo {
                window_id: "@1".into(),
                window_name: "with-pane".into(),
                window_active: true,
                auto_rename: false,
                panes: vec![PaneInfo {
                    pane_id: "%1".into(),
                    pane_active: true,
                    status: PaneStatus::Running,
                    attention: false,
                    agent: AgentType::Claude,
                    path: "/repo".into(),
                    current_command: String::new(),
                    prompt: String::new(),
                    prompt_is_response: false,
                    started_at: None,
                    wait_reason: String::new(),
                    permission_mode: PermissionMode::Default,
                    subagents: vec![],
                    pane_pid: None,
                    session_id: None,
                    session_name: String::new(),
                    bg_shell_cmd: None,
                }],
            },
        );
        entry.insert(
            "@2".to_string(),
            WindowInfo {
                window_id: "@2".into(),
                window_name: "empty".into(),
                window_active: false,
                auto_rename: false,
                panes: vec![],
            },
        );

        let sessions = finalize_sessions(sessions_map);

        assert_eq!(sessions.len(), 1, "session should survive");
        assert_eq!(
            sessions[0].windows.len(),
            1,
            "empty window must be filtered out"
        );
        assert_eq!(sessions[0].windows[0].window_id, "@1");
    }

    #[test]
    fn finalize_sessions_drops_session_when_all_windows_are_empty() {
        let mut sessions_map: SessionMap = indexmap::IndexMap::new();
        let entry = sessions_map.entry("dead".to_string()).or_default();
        entry.insert(
            "@9".to_string(),
            WindowInfo {
                window_id: "@9".into(),
                window_name: "ghost".into(),
                window_active: false,
                auto_rename: false,
                panes: vec![],
            },
        );

        let sessions = finalize_sessions(sessions_map);

        assert!(sessions.is_empty(), "session with no panes must be dropped");
    }
}
