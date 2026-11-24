# Screen colour picker

**Status:** Implemented · **Scope:** One-shot screen sampling and colour values.

The colour picker takes one screenshot, lets the person click a pixel, and reports the colour.
It never records the screen or keeps an image.

## Flow

1. The person chooses **Pick a Colour** (or the model proposes `color.pick` and the panel asks).
2. `sam-linux::screenshot` calls `org.freedesktop.portal.Screenshot` once.
3. The portal, and on first use the person, approves the capture.
4. The portal returns a temporary PNG. Sam loads it into a preview window and deletes the file
   immediately.
5. The person clicks a pixel. Sam maps the click through the picture's `ContentFit::Contain`
   geometry, reads that one pixel, and shows hex, RGB and HSL in the detail card.
6. The preview window closes. No screenshot bytes are written to disk or retained after the
   window closes.

## One shot, no recording

The implementation deliberately does not use a live capture, a screencast pipe or the GNOME
Shell extension:

- no polling, recording or background capture;
- no image in the result, logs or cache;
- the result is a colour value and its text form only;
- the temporary file is deleted as soon as the texture is loaded.

## Pixel handling

`gdk_texture_download` returns CAIRO_FORMAT_ARGB32 in native byte order (little-endian
`B, G, R, A`). Screenshots are opaque, but the sampler unpremultiplies defensively so a
transparent pixel does not come back too dark.

`crates/sam-linux/src/color.rs` owns the conversions:

- `#rrggbb`;
- `rgb(r, g, b)`;
- HSL in degrees and percent;
- a plain-text block for Copy.

## Policy

- `color.pick` is `LowRisk`, `Never`, `Panel`.
- A person picking the row may run it.
- A model proposal asks for confirmation because `color.pick` is not on the model's automatic
  allowlist.
- The portal asks as well. Sam cannot bypass that dialog.

## Dependencies

- `xdg-desktop-portal` is required.
- `xdg-desktop-portal-gnome` is recommended for the GNOME backend.
- The RPM and `scripts/install-user.sh` install the action and its desktop files; no extra
  runtime library is needed beyond GTK/GDK.

## Limits

- The portal must be available. Without it the action reports a clean error and suggests
  installing `xdg-desktop-portal`.
- One capture per invocation; there is no live magnifier or continuous sampling.
- The preview uses the captured image's pixels, not a second screen read.
- The action is panel-only; the CLI does not open a picker.

## Tests

`crates/sam-linux/src/color.rs` checks hex, RGB, HSL, greys and hex parsing. The panel's pixel
mapping is covered by the geometry in `sample()`; the portal client is exercised on a live
GNOME session.
