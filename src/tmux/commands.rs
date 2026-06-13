use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use rmux_client::{ClientError, Connection, connect, resolve_socket_path};
use rmux_proto::{
    BindKeyRequest, CAPABILITY_SDK_PANE_BROADCAST, CapturePaneRequest, CommandOutput,
    HookLifecycle, HookName, OptionScopeSelector, PaneId, PaneTarget, ResolveTargetType, Response,
    ScopeSelector, SessionName, SetOptionMode, SplitDirection, SplitWindowTarget, Target,
    WindowTarget,
};

pub fn display_message(target: &str, format: &str) -> String {
    display_message_result(target, format)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn display_message_result(target: &str, format: &str) -> Result<String, String> {
    let mut conn = rmux_connection()?;
    let target = resolve_any_target(&mut conn, target)?;
    display_message_for_target(&mut conn, target, format)
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

pub fn list_panes_formatted(
    target: Option<&str>,
    all_sessions: bool,
    format: &str,
) -> Result<String, String> {
    let mut conn = rmux_connection()?;
    let format = Some(format.to_string());
    if all_sessions {
        return list_panes_all_sessions(&mut conn, format);
    }
    match target {
        Some(target) => {
            let (session, window) = session_window_for_list_target(&mut conn, target)?;
            let response = conn
                .list_panes_in_window(session, window, format)
                .map_err(rmux_client_error)?;
            stdout_from_response(response)
        }
        None => {
            let session = active_session(&mut conn)?;
            let response = conn
                .list_panes_in_window(session, None, format)
                .map_err(rmux_client_error)?;
            stdout_from_response(response)
        }
    }
}

pub fn split_window_vertical(
    target: &str,
    before: bool,
    size: &str,
    start_directory: &str,
    command: &str,
    output_format: &str,
) -> Result<String, String> {
    let mut conn = rmux_connection()?;
    let target = match resolve_target(&mut conn, target, ResolveTargetType::Pane)? {
        Target::Session(session) => SplitWindowTarget::Session(session),
        Target::Window(window) => SplitWindowTarget::Session(window.session_name().clone()),
        Target::Pane(pane) => SplitWindowTarget::Pane(pane),
    };
    let response = conn
        .roundtrip(&rmux_proto::Request::SplitWindowExt(
            rmux_proto::SplitWindowExtRequest {
                target,
                direction: SplitDirection::Vertical,
                before,
                environment: None,
                command: Some(vec![command.to_string()]),
                process_command: None,
                start_directory: Some(PathBuf::from(start_directory)),
                keep_alive_on_exit: None,
                detached: false,
                size: Some(size.to_string()),
                preserve_zoom: false,
            },
        ))
        .map_err(rmux_client_error)?;
    match response {
        Response::SplitWindow(response) => {
            display_message_for_target(&mut conn, Target::Pane(response.pane), output_format)
        }
        Response::Error(error) => Err(error.error.to_string()),
        other => Err(format!(
            "rmux returned {} for split-window",
            other.command_name()
        )),
    }
}

/// Send a command line to `target` (a pane id) and press Enter so the shell
/// executes it. Input goes through rmux SDK pane broadcast so we do not shell
/// out to tmux/rmux.
pub fn send_command(target: &str, command: &str) -> Result<(), String> {
    send_command_with_broadcast(target, command)
}

/// Kill the tmux window identified by `window_id` (e.g. `@7`).
pub fn kill_window(window_id: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let window = resolve_window(&mut conn, window_id)?;
    let response = conn.kill_window(window, false).map_err(rmux_client_error)?;
    stdout_from_response(response).map(|_| ())
}

pub fn kill_pane(pane_id: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let pane = resolve_pane(&mut conn, pane_id)?;
    let response = conn.kill_pane(pane).map_err(rmux_client_error)?;
    stdout_from_response(response).map(|_| ())
}

/// Kill the tmux session identified by `session_id` (preferably `$N`).
pub fn kill_session(session_id: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let session = resolve_session(&mut conn, session_id)?;
    let response = conn
        .kill_session(rmux_proto::KillSessionRequest {
            target: session,
            kill_all_except_target: false,
            clear_alerts: false,
        })
        .map_err(rmux_client_error)?;
    stdout_from_response(response).map(|_| ())
}

pub fn select_pane(pane_id: &str) {
    // Find the session containing this pane and switch to it first.
    let session_id = display_message(pane_id, "#{session_id}");
    if !session_id.is_empty() {
        let _ = switch_client(&session_id);
    }
    // Then switch to the correct window.
    let window_id = display_message(pane_id, "#{window_id}");
    if !window_id.is_empty() {
        let _ = select_window(&window_id);
    }
    let _ = select_pane_by_id(pane_id);
}

pub fn select_pane_by_id(pane_id: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let pane = resolve_pane(&mut conn, pane_id)?;
    let response = conn.select_pane(pane).map_err(rmux_client_error)?;
    stdout_from_response(response).map(|_| ())
}

pub fn select_last_pane(window_id: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let window = resolve_window(&mut conn, window_id)?;
    let response = conn.last_pane(window).map_err(rmux_client_error)?;
    stdout_from_response(response).map(|_| ())
}

pub fn select_window(window_id: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let window = resolve_window(&mut conn, window_id)?;
    let response = conn.select_window(window).map_err(rmux_client_error)?;
    stdout_from_response(response).map(|_| ())
}

pub fn switch_client(session_id: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let session = resolve_session(&mut conn, session_id)?;
    let response = conn.switch_client(session).map_err(rmux_client_error)?;
    stdout_from_response(response).map(|_| ())
}

pub fn set_global_option(key: &str, value: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    set_option_by_name(
        &mut conn,
        global_scope_for_option_name(key),
        key,
        Some(value),
        false,
        false,
    )
}

pub fn get_global_option(key: &str) -> Result<Option<String>, String> {
    let mut conn = rmux_connection()?;
    let response = conn
        .show_options(
            global_scope_for_option_name(key),
            Some(key.to_string()),
            true,
            false,
        )
        .map_err(rmux_client_error)?;
    match stdout_from_response(response) {
        Ok(output) => Ok(Some(output.trim().to_string()).filter(|s| !s.is_empty())),
        Err(err) if err.starts_with("invalid option:") => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn get_all_global_options_formatted() -> Result<String, String> {
    let mut conn = rmux_connection()?;
    let response = conn
        .show_options(OptionScopeSelector::SessionGlobal, None, false, false)
        .map_err(rmux_client_error)?;
    stdout_from_response(response)
}

pub fn unset_global_option(key: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    set_option_by_name(
        &mut conn,
        global_scope_for_option_name(key),
        key,
        None,
        false,
        true,
    )
}

pub fn set_pane_option_by_id(pane: &str, key: &str, value: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let pane = resolve_pane(&mut conn, pane)?;
    set_option_by_name(
        &mut conn,
        OptionScopeSelector::Pane(pane),
        key,
        Some(value),
        false,
        false,
    )
}

pub fn get_pane_option_by_id(pane: &str, key: &str) -> Result<String, String> {
    let mut conn = rmux_connection()?;
    let pane = resolve_pane(&mut conn, pane)?;
    let response = conn
        .show_options(
            OptionScopeSelector::Pane(pane),
            Some(key.to_string()),
            true,
            false,
        )
        .map_err(rmux_client_error)?;
    Ok(stdout_from_response(response)?.trim().to_string())
}

pub fn unset_pane_option_by_id(pane: &str, key: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let pane = resolve_pane(&mut conn, pane)?;
    set_option_by_name(
        &mut conn,
        OptionScopeSelector::Pane(pane),
        key,
        None,
        false,
        true,
    )
}

pub fn set_global_option_if_empty(key: &str, value: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    set_option_by_name(
        &mut conn,
        global_scope_for_option_name(key),
        key,
        Some(value),
        true,
        false,
    )
}

pub fn bind_prefix_key(key: &str, command: Vec<String>) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let response = conn
        .bind_key(BindKeyRequest {
            table_name: "prefix".to_string(),
            key: key.to_string(),
            note: None,
            repeat: false,
            command: Some(command),
        })
        .map_err(rmux_client_error)?;
    stdout_from_response(response).map(|_| ())
}

pub fn replace_global_hook_matching(
    hook: HookName,
    needle: &str,
    command: Option<String>,
) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let response = conn
        .show_hooks(ScopeSelector::Global, false, false, Some(hook))
        .map_err(rmux_client_error)?;
    let output = stdout_from_response(response).unwrap_or_default();
    let mut indices = matching_hook_indices(&output, hook, needle);
    indices.sort_unstable_by(|a, b| b.cmp(a));
    for index in indices {
        let response = conn
            .set_hook_mutation(
                ScopeSelector::Global,
                hook,
                None,
                HookLifecycle::Persistent,
                false,
                true,
                false,
                Some(index),
            )
            .map_err(rmux_client_error)?;
        stdout_from_response(response)?;
    }
    if let Some(command) = command {
        let response = conn
            .set_hook_mutation(
                ScopeSelector::Global,
                hook,
                Some(command),
                HookLifecycle::Persistent,
                true,
                false,
                false,
                None,
            )
            .map_err(rmux_client_error)?;
        stdout_from_response(response)?;
    }
    Ok(())
}

pub fn set_buffer(content: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let response = conn
        .set_buffer(None, content.as_bytes().to_vec(), false, None, false)
        .map_err(rmux_client_error)?;
    stdout_from_response(response).map(|_| ())
}

pub fn capture_pane_ansi(pane_id: &str) -> Result<Vec<u8>, String> {
    let mut conn = rmux_connection()?;
    let target = resolve_pane(&mut conn, pane_id)?;
    let response = conn
        .capture_pane(CapturePaneRequest {
            target,
            start: None,
            end: None,
            print: true,
            buffer_name: None,
            alternate: false,
            escape_ansi: true,
            escape_sequences: false,
            join_wrapped: false,
            use_mode_screen: false,
            preserve_trailing_spaces: false,
            do_not_trim_spaces: false,
            pending_input: false,
            quiet: false,
            start_is_absolute: false,
            end_is_absolute: false,
        })
        .map_err(rmux_client_error)?;
    match response {
        Response::CapturePane(response) => Ok(response
            .command_output()
            .map(|output| output.stdout().to_vec())
            .unwrap_or_default()),
        Response::Error(error) => Err(error.error.to_string()),
        other => Err(format!(
            "rmux returned {} for capture-pane",
            other.command_name()
        )),
    }
}

fn rmux_connection() -> Result<Connection, String> {
    let socket_path = resolve_socket_path(None, None).map_err(rmux_client_error)?;
    connect(&socket_path).map_err(rmux_client_error)
}

fn list_panes_all_sessions(
    conn: &mut Connection,
    format: Option<String>,
) -> Result<String, String> {
    let mut output = String::new();
    let sessions = conn
        .list_sessions(rmux_proto::ListSessionsRequest {
            format: Some("#{session_name}".into()),
            filter: None,
            sort_order: Some("name".into()),
            reversed: false,
        })
        .map_err(rmux_client_error)?;
    let sessions = stdout_from_response(sessions)?;
    for session in sessions.lines() {
        let Some(session) = session_name(session) else {
            continue;
        };
        let response = conn
            .list_panes(session, format.clone())
            .map_err(rmux_client_error)?;
        output.push_str(&stdout_from_response(response)?);
    }
    Ok(output)
}

fn session_window_for_list_target(
    conn: &mut Connection,
    raw: &str,
) -> Result<(SessionName, Option<u32>), String> {
    let target = resolve_target(conn, raw, ResolveTargetType::Window)?;
    Ok(match target {
        Target::Session(session) => (session, None),
        Target::Window(window) => (window.session_name().clone(), Some(window.window_index())),
        Target::Pane(pane) => (pane.session_name().clone(), Some(pane.window_index())),
    })
}

fn send_command_with_broadcast(target: &str, command: &str) -> Result<(), String> {
    let mut conn = rmux_connection()?;
    let pane = resolve_pane(&mut conn, target)?;
    let session = pane.session_name().clone();
    let pane_id = pane_id_for_target(&mut conn, &pane)?;
    if !conn
        .supports_capability(CAPABILITY_SDK_PANE_BROADCAST)
        .map_err(rmux_client_error)?
    {
        return Err("rmux server does not support pane broadcast".into());
    }
    let runtime = tokio_runtime()?;
    runtime.block_on(async move {
        let rmux = rmux_sdk::Rmux::builder()
            .default_timeout(Duration::from_secs(5))
            .connect()
            .await
            .map_err(|err| err.to_string())?;
        let pane_id = PaneId::new(pane_id);
        let pane = rmux
            .pane_by_id(session, pane_id)
            .await
            .map_err(|err| err.to_string())?;
        rmux.broadcast(std::slice::from_ref(&pane), rmux_sdk::Input::Text(command))
            .await
            .map_err(|err| err.to_string())?;
        rmux.broadcast(std::slice::from_ref(&pane), rmux_sdk::Input::Key("Enter"))
            .await
            .map_err(|err| err.to_string())?;
        Ok::<(), String>(())
    })
}

fn resolve_target(
    conn: &mut Connection,
    target: &str,
    target_type: ResolveTargetType,
) -> Result<Target, String> {
    let response = conn
        .resolve_target(
            (!target.is_empty()).then(|| target.to_string()),
            target_type,
            false,
            false,
        )
        .map_err(rmux_client_error)?;
    match response {
        Response::ResolveTarget(response) => Ok(response.target),
        Response::Error(error) => Err(error.error.to_string()),
        other => Err(format!(
            "rmux returned {} for resolve-target",
            other.command_name()
        )),
    }
}

fn resolve_any_target(conn: &mut Connection, target: &str) -> Result<Target, String> {
    let mut last_error = None;
    for target_type in [
        ResolveTargetType::Pane,
        ResolveTargetType::Window,
        ResolveTargetType::Session,
    ] {
        match resolve_target(conn, target, target_type) {
            Ok(target) => return Ok(target),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| format!("could not resolve target {target}")))
}

fn resolve_session(conn: &mut Connection, target: &str) -> Result<SessionName, String> {
    match resolve_target(conn, target, ResolveTargetType::Session)? {
        Target::Session(session) => Ok(session),
        Target::Window(window) => Ok(window.session_name().clone()),
        Target::Pane(pane) => Ok(pane.session_name().clone()),
    }
}

fn resolve_window(conn: &mut Connection, target: &str) -> Result<WindowTarget, String> {
    match resolve_target(conn, target, ResolveTargetType::Window)? {
        Target::Window(window) => Ok(window),
        Target::Pane(pane) => Ok(WindowTarget::with_window(
            pane.session_name().clone(),
            pane.window_index(),
        )),
        Target::Session(session) => Ok(WindowTarget::new(session)),
    }
}

fn resolve_pane(conn: &mut Connection, target: &str) -> Result<PaneTarget, String> {
    match resolve_target(conn, target, ResolveTargetType::Pane)? {
        Target::Pane(pane) => Ok(pane),
        Target::Window(window) => Ok(PaneTarget::with_window(
            window.session_name().clone(),
            window.window_index(),
            0,
        )),
        Target::Session(session) => Ok(PaneTarget::new(session, 0)),
    }
}

fn active_session(conn: &mut Connection) -> Result<SessionName, String> {
    let response = conn
        .display_message(None, true, Some("#{session_name}".to_string()))
        .map_err(rmux_client_error)?;
    let name = stdout_from_response(response)?;
    session_name(name.trim()).ok_or_else(|| "no active session".into())
}

fn global_scope_for_option_name(name: &str) -> OptionScopeSelector {
    if is_user_option(name) {
        OptionScopeSelector::SessionGlobal
    } else {
        OptionScopeSelector::ServerGlobal
    }
}

fn is_user_option(name: &str) -> bool {
    name.split('[')
        .next()
        .is_some_and(|base| base.starts_with('@'))
}

fn session_name(raw: &str) -> Option<SessionName> {
    SessionName::new(raw).ok()
}

fn pane_id_for_target(conn: &mut Connection, target: &PaneTarget) -> Result<u32, String> {
    let format = "#{pane_id}";
    let response = conn
        .list_panes_in_window(
            target.session_name().clone(),
            Some(target.window_index()),
            Some(format.into()),
        )
        .map_err(rmux_client_error)?;
    let output = stdout_from_response(response)?;
    output
        .lines()
        .nth(target.pane_index() as usize)
        .and_then(|pane_id| pane_id.strip_prefix('%'))
        .and_then(|pane_id| pane_id.parse::<u32>().ok())
        .ok_or_else(|| format!("could not resolve pane id for {target}"))
}

fn stdout_from_response(response: Response) -> Result<String, String> {
    match response {
        Response::Error(error) => Err(error.error.to_string()),
        response => Ok(command_output_to_string(response.command_output())),
    }
}

fn display_message_for_target(
    conn: &mut Connection,
    target: Target,
    format: &str,
) -> Result<String, String> {
    let response = conn
        .display_message(Some(target), true, Some(format.to_string()))
        .map_err(rmux_client_error)?;
    stdout_from_response(response)
}

fn command_output_to_string(output: Option<&CommandOutput>) -> String {
    output
        .map(|output| String::from_utf8_lossy(output.stdout()).to_string())
        .unwrap_or_default()
}

fn matching_hook_indices(output: &str, hook: HookName, needle: &str) -> Vec<u32> {
    let hook = hook.as_str();
    output
        .lines()
        .filter(|line| line.contains(needle))
        .filter_map(|line| {
            let start = line.find(hook)? + hook.len();
            let rest = line.get(start..)?;
            let bracket_start = rest.find('[')? + 1;
            let bracket_end = rest.get(bracket_start..)?.find(']')? + bracket_start;
            rest.get(bracket_start..bracket_end)?.parse().ok()
        })
        .collect()
}

fn set_option_by_name(
    conn: &mut Connection,
    scope: OptionScopeSelector,
    name: &str,
    value: Option<&str>,
    only_if_unset: bool,
    unset: bool,
) -> Result<(), String> {
    set_option_by_name_with_mode(
        conn,
        scope,
        name.to_string(),
        value.map(str::to_string),
        SetOptionMode::Replace,
        only_if_unset,
        unset,
    )
    .map(|_| ())
}

fn set_option_by_name_with_mode(
    conn: &mut Connection,
    scope: OptionScopeSelector,
    name: String,
    value: Option<String>,
    mode: SetOptionMode,
    only_if_unset: bool,
    unset: bool,
) -> Result<String, String> {
    let response = conn
        .set_option_by_name(scope, name, value, mode, only_if_unset, unset, false)
        .map_err(rmux_client_error)?;
    stdout_from_response(response)
}

fn tokio_runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|err| err.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn rmux_client_error(error: ClientError) -> String {
    error.to_string()
}
