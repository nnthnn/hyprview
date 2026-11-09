# Configuration

hyprview reads `$XDG_CONFIG_HOME/hyprview/config.toml`, falling back to
`~/.config/hyprview/config.toml`. Every key is optional. On first run the file
is seeded from the bundled [`config.example.toml`](../config.example.toml); an
existing pre-rename `~/.config/hyprexpose/config.toml` is migrated automatically.

The file is watched for changes and reloaded live. Send `SIGHUP` to force a
reload:

```sh
pkill -SIGHUP hyprview
```

Unknown sections and keys are reported on stderr, so typos do not silently fall
back to defaults. [`config.example.toml`](../config.example.toml) contains the
same options with inline comments.

## `[appearance]`

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

## `[colors]`

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

## `[behavior]`

| Key | Default | Meaning |
| --- | --- | --- |
| `no_preview` | `false` | Same as `--no-preview` |
| `switch_on_move` | `true` | After `m`, switch to the destination workspace and close |
| `allow_mouse` | `false` | Enable mouse navigation; `--allow-mouse` forces it on |
| `show_empty_workspaces` | `false` | Show cards for workspaces with no windows |

## `[keybindings]`

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
