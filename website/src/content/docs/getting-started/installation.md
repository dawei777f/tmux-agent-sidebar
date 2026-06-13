---
title: Installation
description: Install tmux-agent-sidebar for rmux.
---

## Requirements

- rmux 0.5+
- [Rust](https://rustup.rs/) (only if building from source)

## Option A — Plugin directory

Clone the plugin into rmux's plugin directory:

```sh
mkdir -p ~/.rmux/plugins
git clone https://github.com/hiroppy/tmux-agent-sidebar.git \
  ~/.rmux/plugins/tmux-agent-sidebar
```

Run the installer once:

```sh
~/.rmux/plugins/tmux-agent-sidebar/install-wizard.sh auto
```

The installer downloads a pre-built binary or builds from source, then the launcher registers key bindings and hooks through rmux APIs.

## Option B — Manual binary

1. Clone the repository:

   ```sh
   mkdir -p ~/.rmux/plugins
   git clone https://github.com/hiroppy/tmux-agent-sidebar.git \
     ~/.rmux/plugins/tmux-agent-sidebar
   ```

2. Install the binary — download a pre-built release, or build from source:

   ```sh
   # macOS (Apple Silicon)
   curl -fSL https://github.com/hiroppy/tmux-agent-sidebar/releases/latest/download/tmux-agent-sidebar-darwin-aarch64 \
     -o ~/.rmux/plugins/tmux-agent-sidebar/bin/tmux-agent-sidebar
   chmod +x ~/.rmux/plugins/tmux-agent-sidebar/bin/tmux-agent-sidebar
   ```

   Or build from source:

   ```sh
   cd ~/.rmux/plugins/tmux-agent-sidebar
   cargo build --release
   ```

3. Load the bundled launcher from your rmux config or plugin manager.

## Reload rmux config

After editing your config, reload it through rmux.

## Next steps

The sidebar receives status updates through agent hooks — continue with the agent you use:

- [Claude Code setup](/tmux-agent-sidebar/getting-started/claude-code/)
- [Codex setup](/tmux-agent-sidebar/getting-started/codex/)
- [OpenCode setup](/tmux-agent-sidebar/getting-started/opencode/)
