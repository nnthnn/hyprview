<p align="center">
  <img src="assets/logo.svg" width="120" alt="Hyprview logo">
</p>

# Hyprview

![vibecoded](https://img.shields.io/badge/vibecoded-ff69b4?style=flat&logo=sparkles&logoColor=white)

Lightweight workspace overview for [Hyprland](https://hyprland.org) and [Sway](https://swaywm.org). Shows active workspaces with real window thumbnails, navigate with keyboard or mouse, press Enter to switch.

> **Requires Hyprland >= 0.55** (uses the Lua-based IPC dispatch introduced in that release).

> **Sway support:** the compositor is auto-detected at startup (via `SWAYSOCK`). Window thumbnails require the `hyprland-toplevel-export` protocol, which Sway doesn't implement — on Sway, windows are drawn as colored rectangles with their app id instead (same as `--no-preview`). Everything else (grid, keyboard/mouse navigation, switching, moving windows) works the same.

![Demo](demo.gif)

## Features

- Works on Hyprland and Sway (auto-detected)
- Real window thumbnails via hyprland-toplevel-export protocol (Hyprland only)
- Fullscreen overlay using wlr-layer-shell
- Keyboard navigation (arrow keys / hjkl)
- Tab / Shift+Tab cycling through every window, starting from the focused one
- Type-to-filter windows and workspaces
- App icons on window thumbnails
- Move the active window to another workspace with `m`
- Runs as a daemon, toggled with SIGUSR1 (~0% CPU when hidden)
- Direct IPC via the compositor's unix socket (no process spawning)
- TOML config file for colors, fonts, and behavior

## Dependencies

**Build:** Rust toolchain (`cargo`), `libwayland-dev`, `libpango1.0-dev`

On Arch:

```
pacman -S rust wayland pango
```

On Ubuntu/Debian:

```
apt install cargo libwayland-dev libpango1.0-dev
```

## Build

```
cargo build --release
```

The binary is at `target/release/hyprview`.

## Usage

Start the daemon:

```
hyprview &
```

Disable window previews (uses colored rectangles instead):

```
hyprview --no-preview &
```

Enable mouse navigation (hover to select, left-click to switch, right-click to move active window):

```
hyprview --allow-mouse &
```

Flags can be combined: `hyprview --no-preview --allow-mouse &`.

Toggle the overlay:

```
pkill -SIGUSR1 hyprview
```

Add to your Hyprland config:

```ini
exec-once = hyprview
bind = $mainMod, Tab, exec, pkill -SIGUSR1 hyprview
```

Or to your Sway config:

```
exec hyprview
bindsym $mod+Tab exec pkill -SIGUSR1 hyprview
```

## Controls

| Key | Action |
|---|---|
| Arrow keys / hjkl | Navigate workspaces |
| Tab / Shift+Tab | Cycle through windows (starting from the focused one) |
| Enter | Switch to the highlighted window, or selected workspace |
| 1–9 | Jump directly to a workspace |
| Type | Filter windows and workspaces |
| Backspace | Delete the last filter character |
| `m` | Move active window to selected workspace |
| Delete | Close hovered window (or the single filter match) |
| Escape | Clear filter/highlight, then close overlay |
| Mouse (`--allow-mouse`) | Hover to select, left-click to switch, right-click to move |

## Configuration

Copy the example config and edit to taste:

```
mkdir -p ~/.config/hyprview
cp config.example.toml ~/.config/hyprview/config.toml
```

Config is loaded from `$XDG_CONFIG_HOME/hyprview/config.toml` (falls back to `~/.config/hyprview/config.toml`). On first run, an existing `~/.config/hyprexpose/config.toml` is migrated automatically. All fields are optional — defaults are used for anything not specified.

See [`config.example.toml`](config.example.toml) for all available options with inline documentation.

## How it works

Hyprview runs as a background daemon that does nothing until it receives SIGUSR1. On signal, it:

1. Queries the compositor's IPC socket for active workspaces and clients (Hyprland socket or i3-ipc on Sway)
2. Captures window thumbnails via the hyprland-toplevel-export protocol (Hyprland only)
3. Renders workspace cards with Cairo/Pango onto a wlr-layer-shell overlay
4. Waits for keyboard input, then switches workspace (or moves a window) and hides

## License

MIT. Hyprview is a fork of [hyprexpose](https://github.com/ThiagoAVicente/hyprexpose) by ThiagoAVicente.
