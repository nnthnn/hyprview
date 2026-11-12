# TODO

Open follow-ups found during reviews. Remove items as they are fixed, and keep
each change focused with regression tests where possible; do not bundle them
into documentation-only changes.

## Low priority / hardening

- [ ] Support XPM icons. This needs two parts: a lookup fallback (the
      `freedesktop-icons` crate only probes `.png`, `.svg`, and a mistyped
      `.xmp`, so `.xpm` files in `/usr/share/pixmaps` and legacy icon themes
      are never returned) and a decoder producing a Cairo `ImageSurface`
      (hex colors, `None`, and ideally common named colors). Icon cache
      invalidation for theme/size changes is done.
- [ ] Harden `--toggle`'s lock-file handling: don't create the lock file when
      no daemon is running, and read the PID through the locked handle instead
      of reopening the path.
- [ ] Share the workspace card size/offset math between `render::draw` and
      `workspace_at`/`window_at`; only the window rectangles are shared today,
      so a layout change could make clicks target the wrong card.
- [ ] Convert thumbnail pixels according to the negotiated `wl_shm` format
      instead of always wrapping them as Cairo ARGB32. Treating XRGB8888 as
      ARGB32 would read the unused high byte (zero) as alpha, making the
      thumbnail fully transparent.

## UI polish

- [ ] Update the `--help` mouse description now that right-click closes a
      window and Shift+left-click moves the active one.
- [ ] Use a configurable font (or the existing tooltip font) for the filter
      bar instead of hardcoded `Sans 14`.
