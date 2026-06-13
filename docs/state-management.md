# State Management Architecture

## State Scopes

Every piece of runtime state belongs to one of three scopes:

- **Global**: shared across sidebar instances through rmux global options, written and read through rmux RPC.
- **Per pane**: keyed by rmux pane ID, usually written by agent hooks into rmux pane options.
- **Local**: owned by a single sidebar TUI process.

## Global State

Stored in `GlobalState`. User input writes these values through rmux option APIs; startup and SIGUSR1 focus refreshes read them back.

| Field | Tmux Option | Trigger | Description |
|---|---|---|---|
| `status_filter` | `@sidebar_filter` | Left/right status filter input | Active status filter |
| `selected_pane_row` | `@sidebar_cursor` | j/k selection input, debounced write | Cursor position in the agent list |
| `repo_filter` | `@sidebar_repo_filter` | Repo popup confirmation | Active repo/path group filter |

Each persisted field tracks a matching `last_saved_*` value so a failed rmux write does not let the next read overwrite local UI state with stale data.

## Per-Pane State

Agent hooks call the `hook` subcommand. The adapter normalizes raw agent JSON into `AgentEvent`, and the hook handlers write rmux pane options and append activity entries.

| Tmux Option | Trigger | Description |
|---|---|---|
| `@pane_agent` | SessionStart | Agent type: `claude`, `codex`, or `opencode` |
| `@pane_status` | Agent lifecycle events | `running`, `background`, `waiting`, `idle`, or `error` |
| `@pane_attention` | Attention-producing events and clears | Row attention flag |
| `@pane_cwd` | SessionStart, CwdChanged | Hook-reported working directory |
| `@pane_permission_mode` | SessionStart and permission events | Agent permission mode |
| `@pane_prompt` | UserPromptSubmit, Stop | Latest prompt or response preview |
| `@pane_prompt_source` | UserPromptSubmit, Stop | Whether `@pane_prompt` is user input or a response |
| `@pane_started_at` | UserPromptSubmit | Unix epoch when the current run started |
| `@pane_wait_reason` | StopFailure, PermissionDenied, TeammateIdle | Human-readable wait/error reason |
| `@pane_bg_cmd` | Background Bash activity, SessionEnd | Latest background shell command reported by hooks |
| `@pane_subagents` | SubagentStart/Stop | Comma-separated active subagent display labels |
| `@pane_session_id` | Session and prompt events | Agent-reported session ID |

`PaneRuntimeState` holds local per-pane data that should vanish when the pane disappears:

| Field | Trigger | Description |
|---|---|---|
| `task_progress` | Refresh cycle | Parsed task list from `/tmp/tmux-agent-activity_{pane_id}.log` |
| `task_dismissed_total` | Task completion handling | Avoids re-showing already dismissed completed task groups |
| `inactive_since` | Refresh cycle | Debounces task-progress dismissal while agents briefly appear idle |
| `task_progress_log_mtime` | Refresh cycle | Skips re-parsing unchanged activity logs |

## Local State

| Field | Trigger | Description |
|---|---|---|
| `repo_groups` | Refresh cycle | Panes grouped by current path from `tmux::query_sessions()` |
| `focus_state` | Refresh cycle, user focus input | Sidebar focus and focused pane tracking |
| `scrolls.panes` | User input and render | Agent list scroll state |
| `layout` | Every render | Hit-test metadata: pane row targets, line-to-row mapping, repo button column, hyperlinks |
| `notices` | Startup, render, copy actions | Missing hook and Claude plugin notices plus copy targets |
| `popup` | User input and render | `None`, `Repo`, or `Notices`; enforces at most one popup |
| `sessions` | Background session-name thread | `session_id -> session name` labels from Claude session files |
| `theme` / `icons` | Startup | Loaded from rmux options through rmux RPC |
| `pet_*` / `spinner_frame` | Animation tick | Pet and running-status animation state |

## Refresh Cycle

```text
Agent hooks
  -> CLI hook subcommand
  -> adapter.parse()
  -> AgentEvent
  -> hook handlers write @pane_* options and /tmp activity logs

TUI startup
  -> load theme/icons/global options through rmux RPC
  -> read static plugin notice inputs
  -> scan session-name labels once
  -> initial refresh()

Every active refresh tick
  -> get sidebar visibility through rmux display-message RPC
  -> query_sessions() via one rmux list-panes formatted RPC
  -> group_panes_by_repo()
  -> prune PaneRuntimeState for vanished panes
  -> refresh focused pane and row targets
  -> refresh session labels when the background map is dirty
  -> parse changed activity logs for task progress
  -> render frame

Background worker
  -> periodically scans Claude session-name files
  -> sends a new session map to the TUI thread
```

## Key Invariants

1. `selected_pane_row` is clamped to `layout.pane_row_targets`.
2. `layout.line_to_row` is rebuilt every frame so mouse hit-testing matches the rendered buffer.
3. `PaneRuntimeState` is pruned whenever a pane disappears.
4. Task progress has an idle debounce before completed or stalled task groups are hidden.
5. Global state only clears pending writes after rmux accepts the option update.
6. At most one popup is open because `PopupState` is an enum, not parallel booleans.
7. Rmux/tmux interaction is centralized in `src/tmux/commands.rs`; callers use typed helper functions instead of spawning tmux/rmux commands.
