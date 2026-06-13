#!/usr/bin/env bash

PLUGIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ -x "$PLUGIN_DIR/bin/tmux-agent-sidebar" ]]; then
    SIDEBAR_BINARY="$PLUGIN_DIR/bin/tmux-agent-sidebar"
elif [[ -x "$PLUGIN_DIR/target/release/tmux-agent-sidebar" ]]; then
    SIDEBAR_BINARY="$PLUGIN_DIR/target/release/tmux-agent-sidebar"
fi

if [[ -z "$SIDEBAR_BINARY" ]]; then
    "$PLUGIN_DIR/install-wizard.sh" auto >/tmp/tmux-agent-sidebar-install.log 2>&1 || exit 0
    if [[ -x "$PLUGIN_DIR/bin/tmux-agent-sidebar" ]]; then
        SIDEBAR_BINARY="$PLUGIN_DIR/bin/tmux-agent-sidebar"
    elif [[ -x "$PLUGIN_DIR/target/release/tmux-agent-sidebar" ]]; then
        SIDEBAR_BINARY="$PLUGIN_DIR/target/release/tmux-agent-sidebar"
    else
        exit 0
    fi
fi

"$SIDEBAR_BINARY" plugin-init "$SIDEBAR_BINARY"
