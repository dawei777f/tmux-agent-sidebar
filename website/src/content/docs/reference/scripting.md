---
title: Pane state API
description: Read the pane option fields written by the sidebar through rmux APIs.
---

The sidebar writes agent state into rmux pane options on every hook event. External integrations should read these fields through rmux APIs/RPC rather than spawning tmux or rmux commands.

## Reading state

Use the pane id reported by rmux and request the option key you need. Missing options should be treated as an empty value.

## Available pane options

| Key                        | Value                                                              |
| -------------------------- | ------------------------------------------------------------------ |
| `@pane_status`             | `running` / `background` / `waiting` / `idle` / `error` / empty    |
| `@pane_attention`          | `1` while the pane is flagged for attention, otherwise empty        |
| `@pane_agent`              | `claude` / `codex` / `opencode` / empty                             |
| `@pane_name`               | Friendly agent/session name (from `/rename` on Claude)              |
| `@pane_role`               | `sidebar` for the sidebar pane itself; empty for agent panes        |
| `@pane_prompt`             | Latest user prompt text or response preview                         |
| `@pane_prompt_source`      | `user` when the prompt field holds the user's prompt, `response` when it holds the agent's last reply |
| `@pane_started_at`         | Epoch seconds of the last `UserPromptSubmit`                        |
| `@pane_wait_reason`        | Wait-reason text (populated only when waiting)                      |
| `@pane_bg_cmd`             | Latest sanitized background Bash command reported by the agent; empty when no background shell is tracked. |
| `@pane_subagents`          | Comma-separated subagent labels (Claude only)                       |
| `@pane_cwd`                | Working directory reported by the agent (preferred over `pane_current_path`) |
| `@pane_permission_mode`    | Permission-mode string for the badge (`plan` / `edit` / `auto` / `!` / …) |
| `@pane_session_id`         | Agent session ID (opaque; useful for correlating logs)              |

## Use cases

- **Status dashboards** — surface `@pane_status` and `@pane_wait_reason` in a separate rmux-aware tool.
- **Automation guards** — gate side-effectful actions on agent state using the hook-reported fields.
