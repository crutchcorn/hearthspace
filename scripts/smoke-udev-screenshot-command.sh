#!/usr/bin/env bash
set -euo pipefail

COMPOSITOR_LOG="${COMPOSITOR_LOG:-/tmp/hearthspace-udev-prestep8-missed.log}"
COMMAND_LOG="${COMMAND_LOG:-/tmp/hearthspace-udev-screenshot-command.log}"
EXIT_AFTER_MS="${EXIT_AFTER_MS:-10000}"
COMMAND_DELAY_SECONDS="${COMMAND_DELAY_SECONDS:-3}"
COMMAND_SOCKET="${COMMAND_SOCKET:-${XDG_RUNTIME_DIR:-/tmp}/hearthspace-shell.sock}"

cargo build --no-default-features --features udev

target/debug/hearthspace --tty --no-shell --exit-after-ms "$EXIT_AFTER_MS" 2>&1 | tee "$COMPOSITOR_LOG" &
compositor_pid=$!

sleep "$COMMAND_DELAY_SECONDS"

printf 'screenshot\n' | socat - "UNIX-CONNECT:${COMMAND_SOCKET}" | tee "$COMMAND_LOG"

wait "$compositor_pid"

printf 'Compositor log: %s\n' "$COMPOSITOR_LOG"
printf 'Command log: %s\n' "$COMMAND_LOG"
