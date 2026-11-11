#!/usr/bin/env bash
# Live harness for trying the wrapping window layout.
#
# Hyprview can only draw inside a real Hyprland/Sway session, so this script
# builds the worktree binary, runs it against an isolated config, and offers
# shortcuts for switching wrap/geometry modes and toggling the overlay.
#
# Typical use:
#   ./scripts/try-wrap.sh                 # start, spawn 5 windows, show overlay
#   ./scripts/try-wrap.sh geometry        # switch to the old geometry layout
#   ./scripts/try-wrap.sh wrap            # switch back
#   ./scripts/try-wrap.sh toggle          # open/close the overlay
#   ./scripts/try-wrap.sh clean           # stop the test daemon and clean up
#
# Run `./scripts/try-wrap.sh --help` for options.

set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
bin="$repo_root/target/release/hyprview"

state_root="${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}/hyprview-wrap-test"
config_home="$state_root/config"
config_file="$config_home/hyprview/config.toml"
pid_file="$state_root/daemon.pid"
mode_file="$state_root/mode"
log_file="$state_root/daemon.log"
terminal_pids="$state_root/terminal.pids"

usage() {
    cat <<'EOF'
Usage: ./scripts/try-wrap.sh [COMMAND] [options]

Commands:
  start       Build, stop any running daemon, spawn test windows, show overlay
              (default when no command is given)
  wrap        Switch the running test daemon to wrapping mode
  geometry    Switch the running test daemon to the old geometry layout
  toggle      Open or close the test daemon's overlay
  status      Show the test daemon state and current layout mode
  clean       Stop the test daemon, close test windows, remove the temp config

Options (start):
  -n, --windows N   Number of test windows to spawn (default: 5)
      --no-spawn    Do not spawn test windows
      --no-build    Skip `cargo build --release --locked`
      --no-open     Set everything up but do not show the overlay
      --no-preview  Start with placeholder rectangles instead of thumbnails
  -h, --help        Show this help

The test daemon uses an isolated config under:
  <XDG_RUNTIME_DIR>/hyprview-wrap-test/

Because hyprview allows one daemon per session, starting the harness stops any
installed hyprview daemon. Run `clean` when finished and restart your normal
daemon if needed.
EOF
}

die() {
    printf 'try-wrap: %s\n' "$*" >&2
    exit 1
}

command=start
if [ $# -gt 0 ] && [[ $1 != -* ]]; then
    command=$1
    shift
fi

windows=5
spawn=1
build=1
open=1
no_preview=0

while [ $# -gt 0 ]; do
    case "$1" in
        -n | --windows)
            [ $# -ge 2 ] || die "--windows needs a value"
            windows=$2
            shift 2
            ;;
        --no-spawn)
            spawn=0
            shift
            ;;
        --no-build)
            build=0
            shift
            ;;
        --no-open)
            open=0
            shift
            ;;
        --no-preview)
            no_preview=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            die "unknown option: $1 (try --help)"
            ;;
    esac
done

detect_compositor() {
    if [ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]; then
        printf 'hyprland\n'
    elif [ -n "${SWAYSOCK:-}" ] || [ -n "${I3SOCK:-}" ]; then
        printf 'sway\n'
    else
        return 1
    fi
}

daemon_pid() {
    [ -f "$pid_file" ] || return 1
    local pid
    pid=$(cat "$pid_file" 2>/dev/null || true)
    [ -n "$pid" ] || return 1
    kill -0 "$pid" 2>/dev/null || return 1
    printf '%s\n' "$pid"
}

switch_to_test_workspace() {
    case "$1" in
        hyprland) hyprctl dispatch 'workspace name:hyprview-test' >/dev/null 2>&1 || true ;;
        sway) swaymsg 'workspace hyprview-test' >/dev/null 2>&1 || true ;;
    esac
    sleep 0.3
}

detect_terminal() {
    local candidate
    for candidate in "${TERMINAL:-}" foot kitty alacritty wezterm xterm; do
        [ -n "$candidate" ] || continue
        if command -v "$candidate" >/dev/null 2>&1; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    return 1
}

terminal_argv() {
    # Emit NUL-separated argv for the terminal emulator. The shell payload
    # receives the index and lifetime as $1/$2.
    local term=$1 title=$2 payload=$3 index=$4 lifetime=$5
    case "$(basename "$term")" in
        foot)
            printf '%s\0' "$term" --app-id=hyprview-test -T "$title" \
                -e sh -c "$payload" hyprview-test "$index" "$lifetime"
            ;;
        kitty)
            printf '%s\0' "$term" --class hyprview-test -T "$title" \
                sh -c "$payload" hyprview-test "$index" "$lifetime"
            ;;
        alacritty)
            printf '%s\0' "$term" --class hyprview-test -t "$title" \
                -e sh -c "$payload" hyprview-test "$index" "$lifetime"
            ;;
        wezterm)
            printf '%s\0' "$term" start --class hyprview-test -- \
                sh -c "$payload" hyprview-test "$index" "$lifetime"
            ;;
        xterm)
            printf '%s\0' "$term" -class HyprviewTest -T "$title" \
                -e sh -c "$payload" hyprview-test "$index" "$lifetime"
            ;;
        *)
            die "unsupported terminal: $term"
            ;;
    esac
}

spawn_windows() {
    local count=$1 term payload lifetime
    local -a argv
    lifetime=${HYPRVIEW_TEST_WINDOW_LIFETIME:-300}

    if ! term=$(detect_terminal); then
        printf 'try-wrap: no supported terminal found (tried foot, kitty, alacritty, wezterm, xterm)\n' >&2
        printf 'try-wrap: open %d windows manually on the test workspace\n' "$count" >&2
        return 0
    fi

    : >"$terminal_pids"
    # shellcheck disable=SC2016 # expanded by the spawned shell, not this one
    payload='printf "\033[2J\033[H"; printf "hyprview wrap test window %s\n" "$1"; exec sleep "$2"'
    printf 'try-wrap: spawning %d %s window(s) on workspace "hyprview-test"\n' "$count" "$(basename "$term")"
    for index in $(seq 1 "$count"); do
        mapfile -d '' -t argv < <(terminal_argv "$term" "hyprview test $index" "$payload" "$index" "$lifetime")
        nohup "${argv[@]}" >/dev/null 2>&1 &
        printf '%s\n' "$!" >>"$terminal_pids"
    done
    sleep 0.6
}

set_config_mode() {
    local mode=$1 value
    [ -f "$config_file" ] || die "missing test config: $config_file"
    case "$mode" in
        wrap) value=true ;;
        geometry) value=false ;;
        *) die "unknown mode: $mode" ;;
    esac

    if grep -qE '^[[:space:]]*wrap_windows[[:space:]]*=' "$config_file"; then
        sed -i -E "s/^[[:space:]]*wrap_windows[[:space:]]*=.*/wrap_windows = $value/" "$config_file"
    else
        sed -i "/^\[appearance\]/a wrap_windows = $value" "$config_file"
    fi
    printf '%s\n' "$mode" >"$mode_file"
}

current_mode() {
    [ -f "$mode_file" ] && cat "$mode_file" || printf 'wrap\n'
}

toggle_overlay() {
    XDG_CONFIG_HOME="$config_home" "$bin" --toggle
}

stop_pids() {
    local pid
    if [ -f "$terminal_pids" ]; then
        while read -r pid; do
            [ -n "$pid" ] || continue
            kill "$pid" 2>/dev/null || true
        done <"$terminal_pids"
        rm -f "$terminal_pids"
    fi
}

stop_test_daemon() {
    local pid
    if pid=$(daemon_pid); then
        kill -TERM "$pid" 2>/dev/null || true
        for _ in $(seq 1 20); do
            kill -0 "$pid" 2>/dev/null || break
            sleep 0.1
        done
        kill -KILL "$pid" 2>/dev/null || true
    fi
    rm -f "$pid_file"
}

stop_any_daemon() {
    if pgrep -x hyprview >/dev/null 2>&1; then
        printf 'try-wrap: stopping the running hyprview daemon (restart it after testing)\n'
        pkill -TERM -x hyprview 2>/dev/null || true
        for _ in $(seq 1 20); do
            pgrep -x hyprview >/dev/null 2>&1 || break
            sleep 0.1
        done
        pkill -KILL -x hyprview 2>/dev/null || true
    fi
}

print_checklist() {
    cat <<'EOF'

While the overlay is open, check:
  - 4 landscape windows form a 2x2 grid (not one thin row)
  - an odd last window is centered on its row
  - mixed window shapes keep their aspect ratio
  - hovering each thumbnail highlights the correct window; click focuses it
  - Tab / Shift+Tab follows the visual reading order
  - Delete closes the hovered window and the remaining windows reflow live
  - placeholders wrap too: ./scripts/try-wrap.sh start --no-preview

Next:
  ./scripts/try-wrap.sh geometry   # compare with the old compositor layout
  ./scripts/try-wrap.sh wrap       # switch back
  ./scripts/try-wrap.sh toggle     # reopen the overlay
  ./scripts/try-wrap.sh clean      # stop the test daemon and clean up

EOF
    printf 'Config: %s\n' "$config_file"
    printf 'Mode:   %s\n' "$(current_mode)"
}

start() {
    local compositor pid
    local -a daemon_args=(--allow-mouse)
    compositor=$(detect_compositor) || die "run this inside Hyprland or Sway (no compositor env detected)"
    [[ "$windows" =~ ^[0-9]+$ ]] || die "--windows must be a number"
    if [ "$no_preview" = 1 ]; then
        daemon_args+=(--no-preview)
    fi

    if [ "$build" = 1 ]; then
        printf 'try-wrap: building %s\n' "$bin"
        (cd "$repo_root" && cargo build --release --locked)
    fi
    [ -x "$bin" ] || die "missing binary: $bin (run without --no-build)"

    stop_test_daemon
    stop_any_daemon
    stop_pids

    mkdir -p "$config_home/hyprview" "$state_root"
    cp "$repo_root/config.example.toml" "$config_file"
    set_config_mode wrap

    printf 'try-wrap: starting test daemon with config %s\n' "$config_file"
    XDG_CONFIG_HOME="$config_home" nohup "$bin" "${daemon_args[@]}" >"$log_file" 2>&1 &
    pid=$!
    printf '%s\n' "$pid" >"$pid_file"

    for _ in $(seq 1 50); do
        if ! kill -0 "$pid" 2>/dev/null; then
            cat "$log_file" >&2 || true
            die "test daemon exited during startup"
        fi
        grep -q 'daemon running' "$log_file" 2>/dev/null && break
        sleep 0.1
    done
    kill -0 "$pid" 2>/dev/null || die "test daemon failed to start (see $log_file)"

    switch_to_test_workspace "$compositor"
    if [ "$spawn" = 1 ]; then
        spawn_windows "$windows"
    fi

    if [ "$open" = 1 ]; then
        toggle_overlay
    fi
    print_checklist
}

require_running_daemon() {
    daemon_pid >/dev/null || die "test daemon is not running; run ./scripts/try-wrap.sh first"
}

case "$command" in
    start)
        start
        ;;
    wrap | geometry)
        [ -f "$config_file" ] || die "no test config yet; run ./scripts/try-wrap.sh first"
        require_running_daemon
        set_config_mode "$command"
        kill -HUP "$(daemon_pid)"
        sleep 0.3
        printf 'try-wrap: switched to %s mode\n' "$command"
        printf 'The overlay updates live if it is open; otherwise run ./scripts/try-wrap.sh toggle\n'
        ;;
    toggle)
        require_running_daemon
        toggle_overlay
        ;;
    status)
        if pid=$(daemon_pid); then
            printf 'test daemon: running (pid %s)\n' "$pid"
            printf 'mode:        %s\n' "$(current_mode)"
            printf 'config:      %s\n' "$config_file"
            printf 'log:         %s\n' "$log_file"
        else
            printf 'test daemon: not running\n'
        fi
        ;;
    clean)
        stop_test_daemon
        stop_pids
        rm -rf "$state_root"
        printf 'try-wrap: stopped the test daemon and removed the temporary config\n'
        printf 'try-wrap: restart your normal hyprview daemon if you stopped it\n'
        ;;
    -h | --help)
        usage
        ;;
    *)
        die "unknown command: $command (try --help)"
        ;;
esac
