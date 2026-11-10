# Changelog

All notable changes to hyprview are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `CONTRIBUTING.md` and this changelog.
- Prebuilt x86_64 binaries, checksums, and a source tarball on GitHub releases.
- Project docs are installed with the AUR package.

### Changed

- Moved architecture, build, packaging, and release notes out of the README into
  [`docs/development.md`](docs/development.md).
- Gave the crate a description, license, repository, homepage, and keywords.
- Archived historical design notes under [`docs/archive/`](docs/archive/).
- Install docs now lead with GitHub releases and Cargo; AUR publishing is paused
  while Arch restricts new accounts and package pushes.
- Thumbnail pixels are decoded to Cairo surfaces once per capture and reused on
  every redraw instead of being cloned and rebound each frame.

### Fixed

- The release binary job now installs the pinned Rust toolchain and builds on
  Ubuntu 22.04 for a wider glibc baseline.
- Documented color defaults now match the values compiled into the binary.
- Replaced an invalid crates.io category and corrected the development guide's
  description of when the overlay closes.
- Empty workspaces no longer shift the selection highlight or pointer
  hit-testing when `show_empty_workspaces` is off.
- Clearing a filter keeps the current card, out-of-range number keys are
  ignored, navigation always cancels the Tab highlight, and Tab targets filtered
  out of view are no longer focused.
- Hiding the overlay resets hover, pointer, and Shift state so the next show
  cannot render a stale tooltip or reverse the first Tab.
- `no_preview` and `allow_mouse` now take effect on config reload.
- Sway IPC replies time out instead of blocking the event loop, frame copies
  handle short writes, and the Hyprland JSON parser decodes escaped quotes.
- The instance lock refuses symlinks and non-regular files and is created with
  mode `0600`.
- Malformed or non-ASCII hex colors are rejected instead of risking a panic.
- Cached app icons are keyed by size and refreshed when the icon theme changes.

## [0.1.0] - 2026-11-09

### Added

- Fullscreen workspace overview on `wlr-layer-shell`, toggled with `SIGUSR1`.
- Live window thumbnails through `hyprland-toplevel-export-v1` on Hyprland.
- Sway fallback using i3 IPC and coloured placeholder windows.
- Keyboard navigation with arrows, `hjkl`, and `1`–`9` workspace jumps.
- Tab / Shift+Tab cycling across every window, starting from the focused window.
- Type-to-filter across workspace names, window classes, and titles.
- App icons resolved from the icon theme and `.desktop` files, plus hover
  tooltips.
- Window focus, move, and close actions, with optional mouse support.
- TOML configuration with hot reload, `SIGHUP` reload, and unknown-key warnings.
- Single-instance locking and `hyprview --toggle`.
- Unit tests, CI, git hooks, AUR packaging, and a tag-driven release workflow.

[Unreleased]: https://github.com/nnthnn/hyprview/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/nnthnn/hyprview/releases/tag/v0.1.0
