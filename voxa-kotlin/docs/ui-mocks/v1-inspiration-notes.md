# Voxa Kotlin — v1 Inspiration Notes

**Project:** Voxa, Android Kotlin/Compose app. Voice-dictation IME that captures a ChatGPT session via WebView, then transcribes speech using OpenAI's `/backend-api/transcribe` and inserts the text into the active text field.

**Screens covered by the redesign:**
1. Auth (WebView-based ChatGPT sign-in)
2. Home / Settings (IME status, settings, account)
3. IME keyboard surface (the working keyboard view)

**Slop bucket to escape:** Warm-Paper-Cozy AI (current voxa state). Cream + amber + italic serif "voxa" + letter-spaced eyebrow + circle-numbered "How it works" list. This is the 2026 named community slop bucket — opposite of the Purple Problem in appearance, identical in mechanism.

---

## Hard avoids (must NOT propose)

- Warm-paper-quill (Fraunces italic + cream + terracotta + quill SVG)
- Purple/indigo + violet on near-black (Purple Problem)
- Linear Magic Blue `#5E6AD2` + Geist/Inter + frosted glass cards
- shadcn defaults
- Tailwind-500 hex palette (`#3b82f6` `#22c55e` `#8b5cf6` `#ec4899` `#f97316` etc.) — these are the AI-default tell
- v0/Lovable/Bolt pastel + film grain
- Vague aspirational microcopy ("Elevate your voice", "Transform your typing")

---

## Reference signals (mixed sources)

| Source | Take |
|---|---|
| **Wispr Flow** (real, shipping) | Voice-dictation utility app. Iconic moment: large central waveform on dark canvas, focus on the *signal*, status as monospace. Identity is "audio engineer's recording tool." Not literary. |
| **Superwhisper** (real, shipping) | macOS-native dictation. Single mic pill, very dark canvas, signal-cyan accent. Strict typography. Reads as a system utility, not an "AI app." |
| **AudioPen** | Voice-to-text. Big tappable mic, bright accent (theirs is amber), waveform = single-line trace, not bars. Friendly but not childish. |
| **Granola** | AI notes. Strong identity via off-white-on-warm-grey + green accent, but the *green* is `#7CFC9D` phosphor not Tailwind. Type pairing: Söhne + JetBrains Mono. |
| **Teenage Engineering OP-1 / Field Recorder UI** | Hardware aesthetic. Bright orange "REC" lamp, hairline grid panels, monospace labels, deliberate industrial coldness. |
| **Things 3 mobile** | Premium iOS settings done right. Generous whitespace, restrained accent, very clear typographic hierarchy. The "production-grade app" baseline. |
| **Linear mobile** | App-shell IA. Tight monospace labels for status, generous touch targets, restrained color, but **Magic Blue is now slop** — borrow the IA not the palette. |
| **Heliboard / FUTO** | Reference for IME chrome (privacy-keyboard land). Layouts that respect Android system bars, dedicated bottom action row, clear key-style targets. |
| **Cron / Notion Calendar** mobile | Settings dense-but-clean reference. Grouped rows with leading icon + label + trailing accessory. Mobile-native, not web-shrunk. |

---

## 4 emergent directions

### A. Studio Console *(Recommended)*
Recording-studio rack. Hardware-tool aesthetic — the app *is* a piece of recording gear.
- **Personality:** restrained, technical, no decoration
- **Palette:** bg `#0F0F11` (rack-ink black) · surface `#16161A` · hairline `#26262C` · primary `#FF5B2E` (REC-lamp orange — saturated alarm, NOT amber) · panel-white `#F2F2F3` · zinc-mid `#7A7C82`
- **Type:** Space Grotesk (display, tabular caps) + Inter Tight (body) + JetBrains Mono (status readouts, durations)
- **IME signature:** big circular REC button with concentric LED ring; level-meter strip above (live-amplitude); monospace `00:08 / 02:00` counter; chunky "DONE / CANCEL" hardware-style transport at bottom
- **Settings signature:** grouped rows look like rack panels, hairline dividers, every trailing value monospaced
- **Axes:** Restrained / Cool-neutral / Contemporary / Mid-sat / Utilitarian

### B. Acid Lab
Riso-print + oscilloscope. Loud, lab-equipment energy. Anti-slop by being uncomfortable.
- **Personality:** moderate-loud, weird, lab-grade
- **Palette:** bg `#0B0F0A` (green-shifted near-black) · surface `#15191A` · primary `#C7F94B` (phosphor lime — NOT Tailwind green-500) · alert `#FF4D4D` · paper-white `#F2F4ED` · muted `#6B7568`
- **Type:** Geist Mono everywhere (display *and* body) + Inter Tight for long-form fallback
- **IME signature:** single-line oscilloscope trace (CRT line) on dark bg, scanline overlay, monospace terminal-style status `[REC ●] 00:08`, mic = lime-bordered square (no rounded corners)
- **Axes:** Moderate / Neutral / Futurist / Saturated / Abstract-modern

### C. Quiet Sheet
Premium light theme done with restraint. Things 3 / Granola in spirit. Light mode primary.
- **Personality:** restrained, expensive, mobile-native
- **Palette:** bg `#F7F5F0` (warm off-white, NOT pure white) · surface `#FFFFFF` · ink `#16161A` · muted `#7A7A82` · primary `#1F4FE0` (deep ink-blue, NOT indigo-500 `#6366f1`) · destructive `#E5484D`
- **Type:** Inter Tight display + Inter body + JetBrains Mono small-caps for tags/durations
- **IME signature:** very large flat mic disc, soft inner-shadow ring; tape-counter style timer in monospace; level-meter as horizontal hairline (not bars); minimal but generous spacing
- **Axes:** Restrained / Neutral-warm / Contemporary / Desat / Utilitarian

### D. Tape Deck
Analog hardware warm — escapes the cozy bucket by being *industrial* warm not *literary* warm.
- **Personality:** moderate, characterful, hand-feel
- **Palette:** bg `#1B1714` (walnut deep) · surface `#241F1A` · hairline `#332B22` · primary `#E94E1B` (REC orange — saturated, NOT the current `#C97D2E` amber) · VU-green `#7FB069` · cream `#EDE3D2` · muted `#8C7E6A`
- **Type:** Authentic Sans (use Inter Tight as web fallback) + Space Mono — NO italic serif, escapes the current cozy bucket
- **IME signature:** TWO analog VU meters (L/R) flanking the central record button; tape-counter timer in monospace cream; recessed hardware-look button (subtle gradient + inner shadow); status text in small-caps Space Mono
- **Axes:** Moderate / Warm / Heritage / Mid-sat / Literal-tactile

---

## Axis-spread audit

| Axis | A Studio | B Acid | C Sheet | D Tape | Spread |
|---|---|---|---|---|---|
| Energy | Restrained | Moderate-Loud | Restrained | Moderate | 2 positions ✓ |
| Temperature | Cool-neutral | Neutral | Neutral-warm | Warm | 3 ✓ |
| Era | Contemporary | Futurist | Contemporary | Heritage | 3 ✓ |
| Saturation | Mid | Saturated | Desat | Mid | 3 ✓ |
| Affinity | Utilitarian | Abstract-modern | Utilitarian | Literal-tactile | 3 ✓ |

All four cover ≥3 axes. None lands in a hard-avoid bucket.

---

## Recommended pick

**Direction A — Studio Console.** Three reasons:
1. **Subject-fit:** Voxa *is* a recording tool. A "rack-panel" aesthetic literalises that. The current italic-serif wordmark fights the actual subject.
2. **Production-grade:** Hardware-tool aesthetic reads as "real app" not "AI demo." Maps to user's "production-grade" requirement.
3. **Compose-feasible:** No exotic libraries needed. Hairlines, monospace text, simple gradients — all native Compose. Ships fast.

Direction D (Tape Deck) is the runner-up if the user wants more character. Direction B (Acid Lab) ships if they want maximum anti-slop differentiation.
