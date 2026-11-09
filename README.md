<p align="center">
  <img src="assets/logo.svg" width="120" alt="Hyprview logo">
</p>

<h1 align="center">hyprview</h1>

<p align="center">
  <strong>A fast, keyboard-driven workspace overview for Hyprland and Sway.</strong><br>
  Live window thumbnails, type-to-filter, and app icons — no compositor plugin required.
</p>

<p align="center">
  <a href="https://github.com/nnthnn/hyprview/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/nnthnn/hyprview/actions/workflows/ci.yml/badge.svg"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="Hyprland 0.55+" src="https://img.shields.io/badge/Hyprland-0.55%2B-58d3f7">
  <img alt="Sway" src="https://img.shields.io/badge/Sway-supported-4cae4f">
</p>

<p align="center">
  <img src="demo.gif" width="720" alt="hyprview demo">
</p>

hyprview runs as a small background daemon. On demand it puts up a fullscreen
overlay showing every workspace with real window thumbnails, then lets you
navigate, filter, focus, and move windows — all from the keyboard or, if you
opt in, the mouse.

## Features

- **Live window thumbnails** via `hyprland-toplevel-export-v1` (Hyprland only)
- **Workspace grid** with named/numbered labels and real window layout per card
- **Keyboard-first navigation** with arrows or `hjkl`, plus `1`–`9` for direct card jumps
- **Tab / Shift+Tab window cycling** across all workspaces, starting from the focused window
- **Type-to-filter** across workspace names, window classes, and titles, with non-matches dimmed
- **Enter to act** on the Tab-highlighted window, the single filter match, or the selected workspace
- **Move windows** with `m` — send the active window to the selected workspace
- **Close windows** with `Delete`, targeting the hovered window or single filter match
- **App icons** on thumbnails, resolved from the icon theme and `.desktop` files
- **Hover tooltip** with the app icon, name, class, and window title
- **Optional mouse support** — hover to select, left-click to focus/switch, right-click to move
- **Config hot reload** when the config file changes (or on `SIGHUP`)
- **Hyprland and Sway** support from one binary, auto-detected at startup
- **Daemon lifetime** with a single-instance lock and near-zero CPU while hidden
- **No process spawning** — workspace/window operations go straight over the compositor's IPC socket

## Requirements

- Linux with a Wayland compositor that supports `wlr-layer-shell` (Hyprland and Sway do)
- **Hyprland 0.55+** for the full feature set, including live thumbnails
- **Sway** works for everything except thumbnails; windows are drawn as coloured
  placeholders with their app id (the same fallback as `--no-preview`)
- No GTK, Qt, or other toolkit at runtime

## Install

The AUR package isn't published yet, so for now build from source. The tracked
packaging source is in
[`packaging/aur/hyprview-git`](packaging/aur/hyprview-git).

### Arch Linux

Using the tracked `hyprview-git` package:

```
git clone https://github.com/nnthnn/hyprview.git
cd hyprview/packaging/aur/hyprview-git
makepkg -si
```

### From source

Build dependencies: a Rust toolchain, `pkg-config`, and the Wayland, Cairo, and
Pango development headers.

```
# Arch
sudo pacman -S --needed base-devel rust wayland pango

# Debian / Ubuntu
sudo apt install cargo pkg-config libwayland-dev libpango1.0-dev
```

Then:

```
git clone https://github.com/nnthnn/hyprview.git
cd hyprview
cargo build --release
```

The binary is at `target/release/hyprview`.

Install it system-wide:

```
sudo install -Dm755 target/release/hyprview /usr/local/bin/hyprview
```

Or user-local (make sure `~/.local/bin` is on your `PATH`):

```
install -Dm755 target/release/hyprview ~/.local/bin/hyprview
```

## Usage

Start the daemon in the background:

```
hyprview &
```

Toggle the overlay from anywhere:

```
hyprview --toggle
# equivalent:
pkill -SIGUSR1 hyprview
```

Force a config reload:

```
pkill -SIGHUP hyprview
```

`hyprview --toggle` uses a PID lock file in `$XDG_RUNTIME_DIR`, so it can find
the running daemon without a process lookup. Starting a second `hyprview`
instance is a no-op — the existing instance keeps running.

### Hyprland config

```ini
# ~/.config/hypr/hyprland.conf
exec-once = hyprview
bind = $mainMod, Tab, exec, hyprview --toggle
```

### Sway config

```
# ~/.config/sway/config
exec hyprview
bindsym $mod+Tab exec hyprview --toggle
```

### Flags

| Flag | Description |
| --- | --- |
| `--toggle` | Send `SIGUSR1` to the running daemon and exit |
| `--no-preview` | Skip thumbnails and draw coloured placeholder rectangles |
| `--allow-mouse` | Enable mouse hover, click, and right-click support |

Flags can be combined, for example:

```
hyprview --no-preview --allow-mouse &
```

## Controls

| Input | Action |
| --- | --- |
| Arrow keys / `hjkl` | Navigate the workspace grid |
| Tab / Shift+Tab | Cycle the window highlight across all workspaces (starts from the focused window) |
| Enter | Focus the highlighted window, the single filter match, or switch to the selected workspace |
| `1`–`9` | Jump to the 1st–9th workspace card in the grid |
| Type | Filter workspaces, window classes, and titles |
| Backspace | Delete the last filter character |
| `m` | Move the active window to the selected workspace (when not filtering) |
| Delete | Close the hovered window, or the single filter match |
| Escape | Clear the Tab highlight, then the filter, then close the overlay |
| Mouse (with `--allow-mouse`) | Hover to select and show the tooltip; left-click a window to focus it, a card to switch workspace, or outside to close; right-click to move the active window to the selected workspace |

`hjkl` navigation is disabled while a filter is active; the arrow keys always
work. Number keys only jump when the filter is empty.

## Configuration

hyprview reads `$XDG_CONFIG_HOME/hyprview/config.toml`, falling back to
`~/.config/hyprview/config.toml`. Every key is optional. On first run the file
is seeded from the bundled [`config.example.toml`](config.example.toml); an
existing pre-rename `~/.config/hyprexpose/config.toml` is migrated automatically.

The file is watched for changes and reloaded live. You can also send `SIGHUP`
to force a reload. Unknown sections and keys are reported on stderr so typos
don't silently fall back to defaults.

See [`config.example.toml`](config.example.toml) for the annotated version of
everything below.

### `[appearance]`

| Key | Default | Meaning |
| --- | --- | --- |
| `font` | `"Sans 11"` | Font for window labels inside cards |
| `label_font` | `"Sans Bold 13"` | Font for workspace labels |
| `tooltip_font` | `"Sans"` | Base family for the hover tooltip (size and weight are appended) |
| `card_padding` | `24.0` | Gap between cards and the screen edge (px) |
| `card_radius` | `12.0` | Card corner radius (px) |
| `max_card_width` | `480.0` | Maximum card width (px) |
| `max_card_height` | `320.0` | Maximum card height (px) |
| `label_height` | `32.0` | Height reserved for the workspace label (px) |
| `thumb_padding` | `8.0` | Padding between the card edge and window area (px) |
| `select_border` | `3.0` | Selection highlight border width (px) |
| `window_gap` | `4.0` | Gap between adjacent window thumbnails (px) |
| `columns` | `0` | Fixed number of grid columns; `0` = auto (square root of workspace count) |
| `show_app_icons` | `true` | Draw app icons on thumbnails and tooltips |
| `icon_size` | `20.0` | App icon size (px), clamped to 8–256 |

### `[colors]`

Colors accept `"#rrggbb"`, `"#rrggbbaa"`, or `[r, g, b, a]` with values in
`0.0`–`1.0`.

| Key | Default | Meaning |
| --- | --- | --- |
| `background` | `"#000000bf"` | Fullscreen dim overlay |
| `card` | `"#1f1f26f2"` | Workspace card background |
| `selection` | `"#6699ffe6"` | Selected card border |
| `label` | `"#d9d9e6ff"` | Workspace label text |
| `empty_label` | `"#80808094"` | `(empty)` text in empty cards |
| `window_label` | `"#ffffffe6"` | Window class/title text |
| `active_window` | `"#ffbf33f2"` | Border around the active window (the one `m` moves) |
| `window_border` | `"#ffffff26"` | Subtle border around every thumbnail |
| `hover_border` | `"#ffffff99"` | Border around the hovered thumbnail |
| `hover_overlay` | `"#ffffff26"` | Brightness tint over the hovered thumbnail |
| `tooltip_background` | `"#0f0f1af2"` | Tooltip background |
| `tooltip_name` | `"#fffff2f2"` | Tooltip app name |
| `tooltip_class` | `"#b2b2bfcc"` | Tooltip window class |
| `tooltip_title` | `"#d9d9e7e5"` | Tooltip window title |

### `[behavior]`

| Key | Default | Meaning |
| --- | --- | --- |
| `no_preview` | `false` | Same as `--no-preview` |
| `switch_on_move` | `true` | After `m`, switch to the destination workspace and close |
| `allow_mouse` | `false` | Enable mouse navigation; `--allow-mouse` forces it on |
| `show_empty_workspaces` | `false` | Show cards for workspaces with no windows |

### `[keybindings]`

Each action accepts one key name or an array of names. Supported names are
`"Escape"`, `"Return"`/`"Enter"`, `"Tab"`, `"Left"`, `"Right"`, `"Up"`,
`"Down"`, `"Delete"`, `"space"`, and any single printable character. Number
keys `1`–`9` are fixed and cannot be remapped.

| Key | Default | Action |
| --- | --- | --- |
| `close` | `"Escape"` | Close overlay (after clearing highlight/filter) |
| `confirm` | `"Return"` | Focus highlighted window or switch workspace |
| `left` | `["Left", "h"]` | Previous workspace |
| `right` | `["Right", "l"]` | Next workspace |
| `up` | `["Up", "k"]` | Workspace above |
| `down` | `["Down", "j"]` | Workspace below |
| `move_window` | `"m"` | Move the active window to the selected workspace |
| `close_window` | `"Delete"` | Close the hovered or single-match window |
| `tab` | `"Tab"` | Cycle window highlight; hold Shift to reverse |

## How it works

1. Starts as a daemon, takes a single-instance `flock` in `$XDG_RUNTIME_DIR`,
   discovers the Wayland globals, and waits in `ppoll`.
2. On `SIGUSR1` (or `--toggle`), creates a fullscreen `wlr-layer-shell` surface
   and queries the compositor: Hyprland's JSON socket or Sway's i3 IPC.
3. Draws the grid immediately with placeholder rectangles, then captures window
   thumbnails one at a time through `hyprland-toplevel-export-v1`, repainting as
   each arrives. Captures have a 500 ms timeout and never block the event loop.
4. Handles input: keyboard, optional pointer, and live config reloads via
   inotify.
5. On confirm/move/close, performs the requested action, tears down the overlay,
   and (for window focus) waits for the surface to disappear before focusing the
   target window.
6. Goes back to sleep until the next toggle.

All compositor requests are sent directly over the existing Unix sockets; no
`hyprctl`, `swaymsg`, or shell process is spawned.

## Development

The unit tests run without a compositor and cover the pure helpers: config
parsing, key mapping, filtering, Tab-cycle candidates, icon lookup, and rendering
metrics.

```
cargo fmt --all --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
cargo build --release
```

A [`justfile`](justfile) wraps the common tasks:

```
just check         # format + lint + test
just fmt           # apply rustfmt
just build         # release build
just audit         # cargo-deny advisory check
just install       # cargo install into ~/.cargo/bin
just install-hooks # enable the tracked git hooks
just package       # build the Arch package
just srcinfo       # regenerate the AUR .SRCINFO
```

CI runs the same format, clippy, build, test, and advisory-audit checks. To have
the fast checks run locally before code leaves your machine, enable the tracked
hooks once per clone:

```
./install-hooks.sh
```

`pre-commit` formats staged Rust files; `pre-push` runs the format check and
clippy.

AUR packaging lives in [`packaging/aur/hyprview-git`](packaging/aur/hyprview-git).
After editing the `PKGBUILD`, regenerate `.SRCINFO` with:

```
cd packaging/aur/hyprview-git
makepkg --printsrcinfo > .SRCINFO
```

## License

MIT — see [`LICENSE`](LICENSE).

hyprview is a fork of [hyprexpose](https://github.com/ThiagoAVicente/hyprexpose)
by ThiagoAVicente.
