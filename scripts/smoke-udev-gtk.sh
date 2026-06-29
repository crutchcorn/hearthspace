#!/usr/bin/env bash
set -euo pipefail

COMPOSITOR_LOG="${COMPOSITOR_LOG:-/tmp/hearthspace-udev-step7-gtk.log}"
CLIENT_LOG="${CLIENT_LOG:-/tmp/hearthspace-udev-step7-gtk-client.log}"
EXIT_AFTER_MS="${EXIT_AFTER_MS:-15000}"
CLIENT_DELAY_SECONDS="${CLIENT_DELAY_SECONDS:-3}"

cargo build --no-default-features --features udev,test-apps

client_pid=""
cleanup() {
    if [[ -n "$client_pid" ]]; then
        kill "$client_pid" 2>/dev/null || true
        wait "$client_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT

target/debug/hearthspace --tty --no-shell --exit-after-ms "$EXIT_AFTER_MS" 2>&1 | tee "$COMPOSITOR_LOG" &
compositor_pid=$!

sleep "$CLIENT_DELAY_SECONDS"

WAYLAND_DISPLAY=wayland-99 \
GDK_BACKEND=wayland \
target/debug/hearthspace --gtk-test-app 2>&1 | tee "$CLIENT_LOG" &
client_pid=$!

wait "$compositor_pid"
cleanup
trap - EXIT

printf 'Compositor log: %s\n' "$COMPOSITOR_LOG"
printf 'GTK client log: %s\n' "$CLIENT_LOG"
