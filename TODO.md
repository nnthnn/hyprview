# TODO

Deferred issues found during the docs/release cleanup review. These predate the
cleanup and should be fixed in focused pull requests with regression tests where
possible. Do not bundle them into documentation-only changes.

## High priority

- [x] Fix `show_empty_workspaces` layout/index mismatch. `redraw()` filters the
      workspace list but keeps `state.selected` indexed against the unfiltered
      list, while `workspace_at()`/`window_at()` and key navigation use the full
      list. See `src/main.rs` (`redraw`, `workspace_at`, `window_at`) and add a
      regression test for the visible-index mapping.

## Medium priority

- [x] Clear `tab_target` on every arrow-key press, not only when the movement
      succeeds. See `AppState::handle_key` in `src/main.rs`.
- [x] Do not reset `selected` to workspace 0 when the filter becomes empty.
      `update_filter_state()` currently treats `""` as matching every
      workspace. See `src/main.rs`.
- [x] Reject a stale `tab_target` when confirming with a filter active, or clear
      it whenever the filter excludes it. See the Confirm branch in
      `AppState::handle_key`.
- [x] Make out-of-range number keys a no-op instead of closing the overlay. See
      the `XKB_KEY_1..=XKB_KEY_9` branch in `src/main.rs`.
- [x] Reset `hovered_window`, pointer coordinates, and `shift_held` when the
      overlay is hidden so reopening cannot show a stale tooltip or reverse the
      first Tab. See `hide()` in `src/main.rs`.
- [x] Add a read timeout to Sway IPC requests so a stalled compositor cannot
      block the single-threaded event loop. See `src/ipc/sway.rs`.
- [x] Drain or skip the thumbnail capture queue when previews are disabled or
      `hyprland-toplevel-export` is unavailable; otherwise the overlay polls
      every 50 ms while visible. See `advance_capture()` and the show path in
      `src/main.rs`.
- [x] Apply `behavior.no_preview` and `behavior.allow_mouse` on config reload,
      or document that they require a restart. Enabling mouse support after
      startup also requires binding `wl_pointer` on the existing seat. See the
      reload block in `src/main.rs`.

## Low priority / hardening

- [x] Guard `window_at()` against empty workspaces; the min/max fold over an
      empty client list overflows `i32` in debug builds.
- [x] Document the US-QWERTY/ASCII input limitation, or move to `xkbcommon`.
- [x] Make `parse_hex()` char-safe and reject malformed lengths instead of
      byte-slicing a potentially multi-byte string.
- [x] Cache Cairo surfaces for thumbnails instead of cloning image data and
      recreating a surface for every window on every redraw.
- [x] Harden the `/tmp/hyprview.lock` fallback against symlinks and non-regular
      files (`O_NOFOLLOW`, mode `0600`).
- [x] Handle escaped quotes in the hand-rolled Hyprland JSON parser.
- [x] Check for short writes when copying the rendered frame into the SHM
      memfd.
- [ ] Support XPM icons in `resolve_icon()`. (Icon cache invalidation for
      theme/size changes is done.)

## Docs / packaging follow-ups

- [ ] Decide whether the README/site tagline should say “Hyprland and Sway” to
      match the crate description and the documented Sway support.
- [ ] Make README images and links that are excluded from the crate package
      work on crates.io (for example by using absolute URLs).
