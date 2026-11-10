# Development

This document covers how hyprview works internally and how to build, test, and
release it. For user-facing setup and usage, see the [README](../README.md).

## Architecture

hyprview is a single-threaded Wayland client. It runs as a daemon so the
overlay can appear instantly, and all compositor requests go directly over the
existing Unix sockets — no `hyprctl`, `swaymsg`, or other shell process is
spawned.

1. Starts as a daemon, takes a single-instance `flock` in `$XDG_RUNTIME_DIR`,
   discovers the Wayland globals, and waits in `ppoll`.
2. On `SIGUSR1` (or `--toggle`), creates a fullscreen `wlr-layer-shell` surface
   and queries the compositor: Hyprland's JSON socket or Sway's i3 IPC.
3. Draws the grid immediately with placeholder rectangles, then captures window
   thumbnails one at a time through `hyprland-toplevel-export-v1`, repainting as
   each arrives. Captures have a 500 ms timeout and never block the event loop.
4. Handles input: keyboard, optional pointer, and live config reloads via
   inotify.
5. On confirm or move, performs the requested action, tears down the overlay,
   and (for window focus) waits for the surface to disappear before focusing the
   target window. Closing a window leaves the overlay open.
6. Goes back to sleep until the next toggle.

The Wayland event loop, state, and input handling live in `src/main.rs`.
`src/ipc.rs` dispatches compositor-specific behaviour to `src/ipc/hyprland.rs`
and `src/ipc/sway.rs`; `src/render.rs` draws with Cairo/Pango; `src/icons.rs`
resolves and caches app icons; `src/config.rs` owns TOML parsing and defaults.

See [`AGENTS.md`](../AGENTS.md) for the invariants that keep the event loop and
thumbnail pipeline safe to change. Historical design notes live in
[`docs/archive/`](archive/).

## Building

Build dependencies are listed in the [README](../README.md#from-source). The
release binary is built with:

```sh
cargo build --release
```

## Tests

The unit tests run without a compositor and cover the pure helpers: config
parsing, key mapping, filtering, Tab-cycle candidates, icon lookup, and
rendering metrics.

```sh
cargo fmt --all --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release
```

A [`justfile`](../justfile) wraps the common tasks:

```sh
just check         # format + lint + test
just fmt           # apply rustfmt
just build         # release build
just release 0.2.0 # bump the version, commit it, and push the release tag
just audit         # cargo-deny advisory check
just install       # cargo install into ~/.cargo/bin
just install-hooks # enable the tracked git hooks
just package       # build the Arch package
just srcinfo       # regenerate the AUR .SRCINFO
just site          # preview the project site at http://localhost:8000
```

## CI and hooks

CI runs the same format, clippy, build, test, and advisory-audit checks. To have
the fast checks run locally before code leaves your machine, enable the tracked
hooks once per clone:

```sh
./install-hooks.sh
```

`pre-commit` formats staged Rust files; `pre-push` runs the format check and
clippy.

## Project site

The project site is plain HTML/CSS in [`site/`](../site/);
[`.github/workflows/pages.yml`](../.github/workflows/pages.yml) stages it with
the logo and demo GIF and deploys it to GitHub Pages. Preview it locally with
`just site`.

## Packaging

AUR packaging lives in
[`packaging/aur/hyprview-git`](../packaging/aur/hyprview-git). After editing the
`PKGBUILD`, regenerate `.SRCINFO` with:

```sh
cd packaging/aur/hyprview-git
makepkg --printsrcinfo > .SRCINFO
```

## Releases

Releases are cut by pushing a `vX.Y.Z` tag that matches `Cargo.toml` (or by
running `just release X.Y.Z` from `main`, which bumps `Cargo.toml` and
`Cargo.lock`, commits the bump, and pushes the tag).
[`.github/workflows/release.yml`](../.github/workflows/release.yml) validates
the tag, attaches a stable source tarball, and builds an x86_64 binary with a
sha256 checksum. The release assets are the primary distribution path while
Arch restricts new AUR accounts and package pushes.
