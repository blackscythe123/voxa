#!/usr/bin/env python3
"""Render an animated HTML card to a still PNG, a GIF and an MP4, and check its safe zone.

Generic: knows nothing about the page's content. The page must follow the contract in README.md:
window.render(t) draws frame t (ms), window.DURATION is the loop length in ms, ?t=<ms> freezes on a
frame, ?still shows the finished frame, and elements marked data-safe must stay inside the safe zone.

Real Chrome through agent-browser, assembly through ffmpeg. Python standard library only.

  python render.py page.html                       # PNG, GIF and MP4 next to the page
  python render.py page.html --only png            # just the still
  python render.py page.html --still-t 7000 --gif-width 960 --gif-fps 20 --gif-max-mb 4
"""
import argparse, functools, json, os, shutil, subprocess, sys, tempfile, time
from pathlib import Path

def _default_ab():
    # Prefer the native binary: the npm .cmd shim on Windows mangles quotes and newlines in arguments.
    native = Path.home() / "AppData/Roaming/npm/node_modules/agent-browser/bin/agent-browser-win32-x64.exe"
    return str(native) if native.exists() else (shutil.which("agent-browser") or "agent-browser")


DEFAULT_AB = _default_ab()
DEFAULT_FFMPEG = shutil.which("ffmpeg") or str(Path.home() / (
    "AppData/Local/Microsoft/WinGet/Packages/Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe/"
    "ffmpeg-9.1-full_build/bin/ffmpeg.exe"))

MIB = 1024 * 1024

# The same check as og-guides.js, injected so pages need not include anything to be checked.
SAFE_JS = """(() => { const Z = %s, times = %s;
  const op = el => { let o = 1; for (let e = el; e && e.nodeType === 1; e = e.parentElement) { const s = getComputedStyle(e);
    if (s.display === 'none' || s.visibility === 'hidden') return 0; o *= +s.opacity; } return o; };
  const els = [...document.querySelectorAll('[data-safe]')], bad = {};
  const check = t => { if (t !== null) window.render(t); els.forEach(el => { if (op(el) < 0.02) return;
    const r = el.getBoundingClientRect(); if (!r.width && !r.height) return;
    if (r.left < Z.x0 - .5 || r.top < Z.y0 - .5 || r.right > Z.x1 + .5 || r.bottom > Z.y1 + .5) {
      const k = el.dataset.safe || el.id || el.tagName;
      if (!bad[k]) bad[k] = [t === null ? 'current' : t, Math.round(r.left), Math.round(r.top), Math.round(r.right), Math.round(r.bottom)]; } }); };
  times.forEach(check);
  return JSON.stringify({n: els.length, bad}); })()"""


class Browser:
    def __init__(self, exe, session, width, height):
        self.exe, self.env = exe, dict(os.environ, AGENT_BROWSER_SESSION=session)
        self.run("set", "viewport", str(width), str(height))

    def run(self, *args, stdin=None, timeout=90):
        # A fresh or just closed session can leave the first command hanging; time out and retry once.
        for attempt in (1, 2):
            try:
                p = subprocess.run([self.exe, *args], input=stdin, capture_output=True, text=True, env=self.env,
                                   shell=False, timeout=timeout)
                break
            except subprocess.TimeoutExpired:
                if attempt == 2:
                    raise SystemExit(f"agent-browser {' '.join(args[:2])} timed out twice")
                print(f"agent-browser {args[0]} timed out, retrying")
        if p.returncode != 0:
            raise SystemExit(f"agent-browser {' '.join(args[:2])} failed: {p.stderr or p.stdout}")
        return p.stdout.strip()

    def eval(self, js):
        out = self.run("eval", " ".join(l.strip() for l in js.splitlines())).splitlines()
        val = out[-1] if out else ""
        try:
            v = json.loads(val)
            return json.loads(v) if isinstance(v, str) and v[:1] in "[{" else v
        except json.JSONDecodeError:
            return val

    def open(self, url):
        self.run("open", url)
        for _ in range(100):  # wait for window.READY, or fonts and images when the page does not set it
            if self.eval("window.READY === true || (window.READY === undefined && document.readyState === 'complete' "
                         "&& document.fonts.status === 'loaded')") is True:
                return
            time.sleep(0.1)
        raise SystemExit("page never became ready (window.READY)")

    def batch(self, cmds):
        self.run("batch", "--bail", stdin=json.dumps(cmds))


def ff(ffmpeg, *args):
    p = subprocess.run([ffmpeg, "-hide_banner", "-loglevel", "error", "-y", *args], capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit("ffmpeg failed: " + p.stderr)


print = functools.partial(print, flush=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("html", type=Path)
    ap.add_argument("--out-dir", type=Path, help="default: the page's folder")
    ap.add_argument("--png", default=None, help="still file name (default <page>.png)")
    ap.add_argument("--anim-name", default="hero", help="base name for the GIF and MP4 (default hero)")
    ap.add_argument("--only", choices=["png", "gif", "mp4", "check"], action="append", help="repeatable; default all")
    ap.add_argument("--width", type=int, default=1280); ap.add_argument("--height", type=int, default=640)
    ap.add_argument("--still-t", type=float, help="still frame time in ms (default: the page's ?still)")
    ap.add_argument("--fps", type=int, default=30, help="capture and MP4 frame rate")
    ap.add_argument("--crf", type=int, default=20, help="H.264 quality, lower is better")
    ap.add_argument("--gif-fps", type=int, default=20); ap.add_argument("--gif-width", type=int, default=960)
    ap.add_argument("--gif-colors", type=int, default=128); ap.add_argument("--gif-max-mb", type=float, default=4.0, help="MiB")
    ap.add_argument("--png-max-mb", type=float, default=1.0, help="MiB")
    ap.add_argument("--zone", default="80,80,1200,560", help="safe zone x0,y0,x1,y1 (GitHub open graph template)")
    ap.add_argument("--check-step", type=float, default=100, help="ms between safe zone checks over the loop")
    ap.add_argument("--no-check", action="store_true", help="skip the safe zone check")
    ap.add_argument("--agent-browser", default=DEFAULT_AB); ap.add_argument("--ffmpeg", default=DEFAULT_FFMPEG)
    ap.add_argument("--session", default="og-render")
    ap.add_argument("--keep-frames", type=Path, help="keep the captured frames in this folder")
    a = ap.parse_args()

    only = set(a.only or ["png", "gif", "mp4", "check"])
    if not a.no_check: only.add("check")
    html = a.html.resolve(); out = (a.out_dir or html.parent).resolve(); out.mkdir(parents=True, exist_ok=True)
    url = html.as_uri()
    b = Browser(a.agent_browser, a.session, a.width, a.height)
    b.open(url + "?t=0")
    dur = b.eval("window.DURATION")
    if not isinstance(dur, (int, float)) or b.eval("typeof window.render") != "function":
        raise SystemExit("page must expose window.render(t) and window.DURATION (ms)")
    print(f"page: {html.name}, loop {dur:.0f} ms")
    failed = False

    if "check" in only and not a.no_check:
        x0, y0, x1, y1 = (float(v) for v in a.zone.split(","))
        js = SAFE_JS % (json.dumps(dict(x0=x0, y0=y0, x1=x1, y1=y1)), "%s")
        times = [i * a.check_step for i in range(int(dur // a.check_step) + 1)]
        r = b.eval(js % json.dumps(times)); n, bad = r["n"], dict(r["bad"])
        b.open(url + "?still")
        for k, v in b.eval(js % "[null]")["bad"].items(): bad.setdefault(k, ["still"] + v[1:])
        if n == 0:
            print("safe zone: no element is marked data-safe, nothing checked"); failed = True
        elif bad:
            failed = True
            for k, it in bad.items():
                print(f"safe zone FAIL: {k} at t={it[0]} spans x {it[1]} to {it[3]}, y {it[2]} to {it[4]} (zone {a.zone})")
        else:
            print(f"safe zone: {n} data-safe elements inside {a.zone} at the still and every {a.check_step:.0f} ms")
        b.open(url + "?t=0")

    tmp = Path(tempfile.mkdtemp(prefix="og-render-"))
    try:
        if "png" in only:
            png = out / (a.png or html.with_suffix(".png").name)
            b.open(url + (f"?t={a.still_t}" if a.still_t is not None else "?still"))
            raw = tmp / "still.png"; b.run("screenshot", str(raw))
            shutil.copyfile(raw, png)
            if png.stat().st_size > a.png_max_mb * MIB:  # fall back to a 256 colour PNG
                ff(a.ffmpeg, "-i", str(raw), "-vf", "split[a][b];[a]palettegen=max_colors=256:stats_mode=full[p];"
                   "[b][p]paletteuse=dither=sierra2_4a", str(png))
            print(f"still: {png} ({png.stat().st_size / 1e3:.0f} kB)")
            if png.stat().st_size > a.png_max_mb * MIB: print("still: over the size target"); failed = True

        if only & {"gif", "mp4"}:
            b.open(url + "?t=0")
            frames = tmp / "frames"; frames.mkdir()
            n = round(dur * a.fps / 1000)
            cmds = []
            for i in range(n):
                cmds += [["eval", f"render({i * 1000 / a.fps:.3f})"], ["screenshot", str(frames / f"f{i:05d}.png")]]
            for k in range(0, len(cmds), 80):
                b.batch(cmds[k:k + 80])
                print(f"\rframes: {min(n, (k + 80) // 2)}/{n}", end="", flush=True)
            print()
            seq = str(frames / "f%05d.png")
            if "mp4" in only:
                mp4 = out / f"{a.anim_name}.mp4"
                ff(a.ffmpeg, "-framerate", str(a.fps), "-i", seq, "-c:v", "libx264", "-preset", "slow", "-crf", str(a.crf),
                   "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-tune", "animation", str(mp4))
                print(f"mp4: {mp4} ({mp4.stat().st_size / 1e3:.0f} kB, {a.fps} fps, {a.width}x{a.height})")
            if "gif" in only:
                gif = out / f"{a.anim_name}.gif"
                # step down quality until under the target: colours first, then dither, then width
                w0, c0, even = a.gif_width, a.gif_colors, lambda x: int(x) // 2 * 2
                tries = [(w0, c0, "bayer:bayer_scale=4"), (w0, c0, "none"), (w0, c0 * 3 // 4, "none"),
                         (even(w0 * .92), c0 * 3 // 4, "none"), (even(w0 * .8), max(32, c0 // 2), "none")]
                for w, c, d in tries:
                    ff(a.ffmpeg, "-framerate", str(a.fps), "-i", seq, "-vf",
                       f"fps={a.gif_fps},scale={w}:-2:flags=lanczos,split[a][b];[a]palettegen=max_colors={c}:stats_mode=full[p];"
                       f"[b][p]paletteuse=dither={d}:diff_mode=rectangle", "-loop", "0", str(gif))
                    size = gif.stat().st_size
                    print(f"gif: {w}px, {c} colours, dither {d}: {size / MIB:.2f} MiB")
                    if size <= a.gif_max_mb * MIB: break
                else:
                    print("gif: over the size target"); failed = True
                print(f"gif: {gif}")
            if a.keep_frames:
                shutil.copytree(frames, a.keep_frames, dirs_exist_ok=True)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    sys.exit(2 if failed else 0)


if __name__ == "__main__":
    main()
