use std::process::Command;

pub fn run_tmux(args: &[&str]) -> Option<String> {
    #[cfg(test)]
    if let Some(output) = test_mock::intercept_run_tmux(args) {
        return output;
    }

    let output = Command::new("tmux").args(args).output().ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        None
    }
}

/// Run a tmux command, returning trimmed stdout on success and stderr on failure.
/// Used by the spawn/remove flow so the UI can surface a meaningful error message
/// instead of a silent fallthrough.
pub fn run_tmux_capture(args: &[&str]) -> Result<String, String> {
    #[cfg(test)]
    if let Some(result) = test_mock::intercept_run_tmux_capture(args) {
        return result;
    }

    let output = Command::new("tmux")
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn tmux: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("tmux exited with status {}", output.status)
        } else {
            stderr
        })
    }
}

pub fn display_message(target: &str, format: &str) -> String {
    run_tmux(&["display-message", "-t", target, "-p", format])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Resolve the tmux session id containing `pane_id` (e.g. `$3`).
/// Session ids are stable even when users rename sessions.
pub fn pane_tmux_session_id(pane_id: &str) -> Option<String> {
    Some(display_message(pane_id, "#{session_id}")).filter(|s| !s.is_empty())
}

/// Resolve the session name containing `pane_id`. Returns `None` when tmux
/// can't find the pane (e.g. it has just been closed).
pub fn pane_session_name(pane_id: &str) -> Option<String> {
    Some(display_message(pane_id, "#{session_name}")).filter(|s| !s.is_empty())
}

/// Create a new tmux window in `session` whose initial cwd is `cwd` and whose
/// title is `name`. Returns `(pane_id, window_id)` on success — the window id
/// is used by the spawn flow to set markers at window scope so split panes
/// (e.g. Claude Code subagents) inherit them.
pub fn new_window(session: &str, cwd: &str, name: &str) -> Result<(String, String), String> {
    let out = run_tmux_capture(&[
        "new-window",
        "-t",
        session,
        "-c",
        cwd,
        "-n",
        name,
        "-P",
        "-F",
        "#{pane_id} #{window_id}",
    ])?;
    let mut parts = out.split_whitespace();
    let pane = parts
        .next()
        .ok_or_else(|| "new-window returned no pane id".to_string())?
        .to_string();
    let window = parts
        .next()
        .ok_or_else(|| "new-window returned no window id".to_string())?
        .to_string();
    Ok((pane, window))
}

/// Set a user option at window scope. Needed so markers survive through
/// split panes that inherit from the window. Returns an error so the
/// spawn flow can roll back when a marker the remove path relies on
/// cannot be written — silently dropping the failure would leave an
/// un-removable pane.
pub fn set_window_option(window: &str, key: &str, value: &str) -> Result<(), String> {
    run_tmux_capture(&["set", "-w", "-t", window, key, value]).map(|_| ())
}

/// Send a command line to `target` (a pane id) and press Enter so the shell
/// executes it. Used to launch the agent binary right after window creation.
/// The text is sent with `-l` (literal) so nothing in `command` can collide
/// with tmux key names (e.g. `Tab`, `BSpace`); Enter is issued as a
/// separate invocation so it's interpreted as the Return key.
pub fn send_command(target: &str, command: &str) -> Result<(), String> {
    run_tmux_capture(&["send-keys", "-t", target, "-l", command])?;
    run_tmux_capture(&["send-keys", "-t", target, "Enter"]).map(|_| ())
}

/// Kill the tmux window identified by `window_id` (e.g. `@7`).
pub fn kill_window(window_id: &str) -> Result<(), String> {
    run_tmux_capture(&["kill-window", "-t", window_id]).map(|_| ())
}

/// Kill the tmux session identified by `session_id` (preferably `$N`).
pub fn kill_session(session_id: &str) -> Result<(), String> {
    run_tmux_capture(&["kill-session", "-t", session_id]).map(|_| ())
}

pub fn select_pane(pane_id: &str) {
    // Find the session containing this pane and switch to it first
    let session_id = display_message(pane_id, "#{session_id}");
    if !session_id.is_empty() {
        let _ = run_tmux(&["switch-client", "-t", &session_id]);
    }
    // Then switch to the correct window
    let window_id = display_message(pane_id, "#{window_id}");
    if !window_id.is_empty() {
        let _ = run_tmux(&["select-window", "-t", &window_id]);
    }
    let _ = run_tmux(&["select-pane", "-t", pane_id]);
}

#[cfg(test)]
pub mod test_mock {
    use std::cell::RefCell;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct TmuxCommand {
        pub args: Vec<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum Response {
        Success(String),
        Failure(String),
    }

    #[derive(Debug, Default)]
    struct MockState {
        commands: Vec<TmuxCommand>,
        responses: Vec<(Vec<String>, Response)>,
    }

    thread_local! {
        static MOCK: RefCell<Option<MockState>> = const { RefCell::new(None) };
    }

    pub fn install() -> MockGuard {
        MOCK.with(|m| *m.borrow_mut() = Some(MockState::default()));
        MockGuard
    }

    pub struct MockGuard;

    impl Drop for MockGuard {
        fn drop(&mut self) {
            MOCK.with(|m| *m.borrow_mut() = None);
        }
    }

    pub fn add_success(args: &[&str], stdout: &str) {
        add_response(args, Response::Success(stdout.to_string()));
    }

    pub fn add_failure(args: &[&str], stderr: &str) {
        add_response(args, Response::Failure(stderr.to_string()));
    }

    fn add_response(args: &[&str], response: Response) {
        MOCK.with(|m| {
            if let Some(state) = m.borrow_mut().as_mut() {
                state.responses.push((
                    args.iter().map(|arg| (*arg).to_string()).collect(),
                    response,
                ));
            }
        });
    }

    pub fn commands() -> Vec<TmuxCommand> {
        MOCK.with(|m| {
            m.borrow()
                .as_ref()
                .map(|state| state.commands.clone())
                .unwrap_or_default()
        })
    }

    pub(super) fn intercept_run_tmux(args: &[&str]) -> Option<Option<String>> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        MOCK.with(|m| {
            let mut mock = m.borrow_mut();
            let state = mock.as_mut()?;
            state.commands.push(TmuxCommand { args: args.clone() });
            Some(match take_response(state, &args) {
                Some(Response::Success(stdout)) => Some(stdout),
                Some(Response::Failure(_)) => None,
                None => None,
            })
        })
    }

    pub(super) fn intercept_run_tmux_capture(args: &[&str]) -> Option<Result<String, String>> {
        let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        MOCK.with(|m| {
            let mut mock = m.borrow_mut();
            let state = mock.as_mut()?;
            state.commands.push(TmuxCommand { args: args.clone() });
            Some(match take_response(state, &args) {
                Some(Response::Success(stdout)) => Ok(stdout.trim().to_string()),
                Some(Response::Failure(stderr)) => Err(stderr),
                None => Err("tmux mock has no response for command".into()),
            })
        })
    }

    fn take_response(state: &mut MockState, args: &[String]) -> Option<Response> {
        let idx = state
            .responses
            .iter()
            .position(|(candidate, _)| candidate == args)?;
        Some(state.responses.remove(idx).1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_mock_intercepts_display_message() {
        let _guard = test_mock::install();
        test_mock::add_success(
            &["display-message", "-t", "%1", "-p", "#{session_id}"],
            "$2\n",
        );

        assert_eq!(pane_tmux_session_id("%1").as_deref(), Some("$2"));

        assert_eq!(
            test_mock::commands(),
            vec![test_mock::TmuxCommand {
                args: vec![
                    "display-message".into(),
                    "-t".into(),
                    "%1".into(),
                    "-p".into(),
                    "#{session_id}".into(),
                ],
            }]
        );
    }

    #[test]
    fn kill_session_uses_kill_session_target() {
        let _guard = test_mock::install();
        test_mock::add_success(&["kill-session", "-t", "$7"], "");

        kill_session("$7").expect("kill session should succeed");

        assert_eq!(
            test_mock::commands(),
            vec![test_mock::TmuxCommand {
                args: vec!["kill-session".into(), "-t".into(), "$7".into()],
            }]
        );
    }
}
