# Brand renders: social card, still, GIF and MP4

Generic tooling for an animated 1280 by 640 card (a GitHub social preview, a README hero). Nothing in
here knows about Voxa; the content lives in the page (here `../social-preview.html`).

## The HTML contract

A page that follows this can be rendered by `render.py`:

| Piece | What the page does |
| --- | --- |
| `window.render(t)` | Draws frame `t` (ms) deterministically: same `t`, same pixels. No timers, no CSS animations or transitions; every moving value is a function of `t`. |
| `window.DURATION` | Loop length in ms. Frame 0 and frame `DURATION` should look the same for a seamless loop. |
| `window.READY` | Optional. Set to `true` once fonts and images are loaded. Without it the renderer waits for `document.fonts` and `readyState`. |
| `?t=<ms>` | Renders that frame and stays frozen (no animation loop). The renderer opens the page this way. |
| `?still` | Renders the finished frame, the one used for the static PNG. |
| `?guides` | Draws the safe zone guide lines and reports violations on the page (include `og-guides.js`). |
| `data-safe="name"` | Marks content that must stay inside the safe zone: text, logos, the icon, cards. Backgrounds and decoration that may bleed to the edges are not marked. |
| `prefers-reduced-motion` | With no `?t`, the page shows the finished frame, still. |

Load fonts with `@font-face` from files in the repo so every render matches.

## Safe zone

GitHub's repository open graph template: canvas 1280 by 640, important content inside x 80 to 1200 and
y 80 to 560 (its 40 pt border at 2x). `og-guides.js` gives the page `?guides` and `window.checkSafe()`.
`render.py` runs the same check itself on every run, at the still and every 100 ms over the loop (any
`data-safe` element that is visible, at opacity 0.02 or more, and outside the zone fails), and exits with
code 2 on a violation. Change the zone with `--zone x0,y0,x1,y1`.

## render.py

Python standard library only. Drives real Chrome through agent-browser (one page load, then
`render(t)` and a screenshot per frame, batched) and assembles with ffmpeg.

```
python render.py page.html                         # check, still PNG, GIF and MP4 next to the page
python render.py page.html --only check            # just the safe zone check
python render.py page.html --only png --still-t 7000
python render.py page.html --fps 30 --gif-fps 20 --gif-width 960 --gif-colors 128 --gif-max-mb 4
```

Outputs: `<page>.png` (or `--png name`), `<anim-name>.gif` and `<anim-name>.mp4` (`--anim-name`,
default `hero`). MP4 is H.264, yuv420p, `+faststart`. The GIF uses a generated palette and steps down
until it is under `--gif-max-mb` (MiB): dithered, then undithered, then three quarters of the colours,
then 92 % width, then 80 % width and half the colours. The PNG falls back to 256 colours if it is over
`--png-max-mb`. Tool paths: `--agent-browser` (defaults to the native Windows binary under the npm
folder, because the `.cmd` shim mangles arguments) and `--ffmpeg` (defaults to PATH, then the WinGet
install). `--keep-frames DIR` keeps the captured PNG frames.

Exit codes: 0 fine, 2 a check failed (safe zone, or a file over its size target).
