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
