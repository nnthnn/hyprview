# AGENTS.md

Guidance for coding agents working in the hyprview repository. User-facing
documentation is in [`README.md`](README.md); the full configuration reference
is [`config.example.toml`](config.example.toml).

## What this is

hyprview is a Linux/Wayland daemon that shows a fullscreen workspace overview
with live window thumbnails. It runs in the background and is toggled with
`SIGUSR1`; there is no tray icon or long-running UI.

It supports two compositors, detected at startup from the environment:

- **Hyprland** (`HYPRLAND_INSTANCE_SIGNATURE`): workspace/client data from
  Hyprland's JSON IPC socket, thumbnails via `hyprland-toplevel-export-v1`.
- **Sway** (`SWAYSOCK`/`I3SOCK`): workspace/client data over the i3 IPC protocol.
  Sway has no toplevel-export equivalent, so windows are drawn as coloured
  placeholder rectangles.

The binary is a single crate with no library target. `src/main.rs` owns the
Wayland client, event loop, state, input handling, and thumbnail capture;
`src/ipc.rs` dispatches to `src/ipc/hyprland.rs` / `src/ipc/sway.rs`;
`src/render.rs` draws with Cairo/Pango; `src/icons.rs` resolves and caches app
icons; `src/config.rs` owns TOML parsing and defaults.

## Commands

```sh
cargo build --release                              # binary at target/release/hyprview
cargo test --locked                                # unit tests; no compositor needed
cargo fmt --all --check                            # formatting; enforced by CI and hooks
cargo clippy --all-targets --locked -- -D warnings # lint; keep this clean
```

A [`justfile`](justfile) wraps these, the hooks, and the packaging tasks:

```sh
just check    # fmt-check + clippy + test
just fmt      # apply rustfmt
just build    # release build
just audit    # cargo-deny advisory check
just install  # cargo install into ~/.cargo/bin
just install-hooks # enable the tracked git hooks
just package  # build the tracked Arch package
just srcinfo  # regenerate the AUR .SRCINFO
```

AUR packaging lives in `packaging/aur/hyprview-git/`. Regenerate `.SRCINFO` with
`makepkg --printsrcinfo > .SRCINFO` after editing the `PKGBUILD`.

`.github/workflows/ci.yml` runs formatting, clippy, the release build, the test
suite, and a `cargo-deny` advisory audit. Run `./install-hooks.sh` once per clone
(or `just install-hooks`) to make `pre-commit` format staged Rust files and
`pre-push` run the format check plus clippy.

Formatting is enforced: keep `cargo fmt --all --check` clean and let rustfmt own
alignment rather than hand-aligning columns.

## Testing

- Tests are pure unit tests co-located with the code they cover
  (`#[cfg(test)] mod tests`). They must pass without a Wayland compositor.
- `src/ipc/sway.rs` tests spin up a throwaway Unix socket and speak the i3 IPC
  protocol to it; follow that pattern for IPC changes.
- Anything that needs a live compositor is verified manually. The overlay can
  only be exercised inside Hyprland or Sway, so factor decision logic into pure
  functions and test those instead of testing `AppState` event handlers.

## Architecture and invariants

These are load-bearing; changing them without understanding the whole flow
usually produces hangs, leaked buffers, or a dead overlay.

- **Single-threaded event loop.** Wayland events, signals, and config-file
  changes are all multiplexed with `ppoll` on the Wayland fd and an inotify fd.
  Do not block the loop on IPC, thumbnail capture, or rendering.
- **Signals are blocked outside `ppoll`.** `SIGUSR1` (toggle), `SIGHUP` (reload),
  `SIGINT`, and `SIGTERM` are added to the process signal mask, and `ppoll`
  atomically unblocks them only while waiting. This closes the race where a
  toggle arrives between the flag check and the poll.
- **One daemon per session.** `try_lock_instance` takes an exclusive `flock` on
  `$XDG_RUNTIME_DIR/hyprview.lock` and writes the PID there. `--toggle` sends
  `SIGUSR1` to that PID and exits; if the lock is free, no daemon is running.
- **Config precedence is defaults < file < CLI flags.** CLI flags are OR-ed
  with config values (`no_preview || config.behavior.no_preview`). A new field
  must be added in three places: the struct + `Default`, the `SECTIONS` table in
  `warn_unknown_keys`, and `config.example.toml` (plus the README if it is
  user-visible).
- **`config.example.toml` is compiled in.** `config.rs` uses `include_str!` to
  seed a user's config on first run, so editing it changes runtime behaviour.
  It also falls back to the pre-rename `~/.config/hyprexpose/config.toml`.
- **Thumbnail capture is asynchronous.** `advance_capture` runs one state-
  machine step per loop iteration: start capture, wait for `buffer_done`, map an
  memfd, copy into a `wl_buffer`, wait for `ready`, copy pixels out, unmap, and
  destroy. Captures have a 500 ms timeout and are queued one at a time so the
  overlay can draw immediately and repaint as thumbnails arrive.
- **`hide()` must cancel in-flight captures.** It clears the queue, takes
  `active_capture`, `munmap`s the mapped buffer, and destroys the frame, buffer,
  layer surface, and surface. Missing any of these leaks or crashes on the next
  show.
- **`pending_focus` is delayed.** Focusing a window is done on a detached thread
  after a 100 ms sleep because the overlay surface must be torn down before the
  compositor can focus the underlying window.
- **Keyboard input is raw evdev keycodes.** There is no xkbcommon dependency.
  `keycode_to_keysym` and `key_name_to_keysym` are hand-maintained tables; new
  bindings generally need entries in both, plus Shift tracking via keycodes
  42/54 if the binding is modifier-sensitive.
- **Mouse support is opt-in.** `--allow-mouse` / `behavior.allow_mouse` gates
  pointer events; without it the overlay is keyboard-only.
- **Sway must degrade gracefully.** `--no-preview` and a missing
  `hyprland-toplevel-export` global both fall back to coloured rectangles; do
  not make thumbnails mandatory for rendering or startup.
- **Rendering is Cairo ARgb32.** Thumbnail pixel data from `tiny-skia` is RGBA;
  `icons::premultiplied_rgba_to_cairo_argb` swaps R/B before handing it to
  Cairo. Keep premultiplied alpha intact.
- **Icon lookup is best-effort.** `icons.rs` searches the icon theme, `.desktop`
  files, and `Icon=` values; every failure path returns `None` rather than
  aborting a draw.

## Conventions

- Keep dependencies lean. This is deliberately a small, dependency-light
  binary; Hyprland's JSON even uses a hand-rolled parser instead of a JSON
  crate. Ask before adding a new dependency.
- Comments explain *why*, not *what*. Match the existing tone, including the
  section banner comments in `main.rs`.
- Error handling is best-effort: IPC, icon, and rendering helpers return
  `Option`/ignore errors and log with `eprintln!`. The daemon should stay alive
  when a single compositor request fails.
- `src/ipc.rs` is the compositor abstraction. New compositor-specific behaviour
  belongs in `hyprland.rs` / `sway.rs` behind a `Compositor` match, not in
  `main.rs`.
- `protocols/hyprland-toplevel-export-v1.xml` is the vendored input for
  `wayland-scanner`; `build.rs` re-runs when it changes. Don't edit the generated
  bindings by hand. `wlr-layer-shell` bindings come from the
  `wayland-protocols-wlr` crate, not a local XML.
- `docs/superpowers/` contains historical design notes and implementation
  plans. They are useful context but may describe planned behavior that has
  since changed (for example, Tab shifting is implemented with raw Shift
  keycodes, not two configurable bindings). Treat the code as current.
- Keep `README.md`, `config.example.toml`, and the config structs in sync. The
  README controls table must match `KeybindingsConfig` defaults.
- Commit messages: imperative subject, body explaining the reasoning. Match the
  existing history (`git log --oneline`).
