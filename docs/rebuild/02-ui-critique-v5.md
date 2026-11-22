# UI critique of v5 quill directions (2026-11-22)

Reviewed: `docs/ui-mocks/v5-quill-directions.html` (screenshots in `docs/ui-mocks/v4-refs/v5-*.png`)
against `docs/rebuild/01-flow-audit.md`. Three independent EGM reviews: design critic, first-time user,
simplicity/scope. Findings below are merged and de-duplicated; where reviewers agreed, that is noted.

## Verdict

- **The three directions are a re-skin** (the mockup says so: "only tokens change"). Choosing colours is cheap;
  the structure is where the work is. Ship one theme. (all three reviewers)
- **Reviewer picks:** design critic: C's strip structure with B's typography. First-time user: A is the most
  legible, C the least. Simplicity: pick any one and move on.
- **The ink line is the one real signature, but it is half done.** Static, the nib reads as a "send" arrow,
  and it appears in 1 of 8 states.

## 1. Strip: rebuild the grammar (all three reviewers)

- 8 states become **4 layouts**: Recording, Working, Done, **Needs you** (one line, one action, ×).
  Signed out, offline, Cloudflare, no speech, focus moved and mic denied are text variants of "Needs you",
  supplied by the core so desktop and Android say the same thing.
- **Drop every badge.** Each one repeats its title ("Signed out" + "Sign in to dictate"); three reads for one message.
- **Fixed leading glyph slot** (shape + colour, never colour alone) so the left edge never changes.
- **Exactly one action** per state; **a dismiss on every persistent state** (as drawn, most failures cannot be closed).
- While recording, consider a **compact pill**: ink line + timer, no text; widen only when it needs you.

## 2. Flow-spec violations and missing states

- Focus moved has two actions; "Back to WhatsApp" pulls you away and is unreliable. Use one action: Copy (or "Paste in Slack").
- Offline and Cloudflare are merged again (the audit's systemic flaw 2). Cloudflare: "Couldn't reach ChatGPT, retry".
- Missing: Retry after server error, Allow microphone, delivery failed, timeout result, Android Writing and No speech,
  Hub empty / offline / signed-out / pending-queue states.
- "Paste again" in the Hub pastes into the Hub (recreates the documented bug). Copy only.
- Undo after paste sends Ctrl+Z into someone else's app; "Change mic" needs a picker inside the strip. Defer both.

## 3. Wording (first-time user)

| Today | Problem | Use instead |
|---|---|---|
| "Ready when you are" + "1 to fix"; Android "Ready" pill + "1 left" | Contradiction | Headline and pill driven by readiness: "Almost ready: 1 thing to fix" |
| "Copy later" (desktop) vs "Discard" (Android) | Unclear, and differs by platform | Same action from the core on both |
| "Paste here" | Does not name the app | "Paste in Slack" |
| Unlabelled × while recording/writing | Close or discard? | "Cancel" text |
| "Writing it out" | Sounds like typing letter by letter | Keep the voice, but make "Working" clear (reviewers split: the simplicity review liked it) |
| "Sign in" | Signed in to what? | "Sign in to ChatGPT" |
| (1) (2) (3), History "1", mic green dot | Unexplained | Drop numbers outside C; "1 not sent"; label the dot |
| No privacy line | Trust gap | "Audio goes to ChatGPT to become text; recordings stay on this device; delete anytime." |
| macOS-only "Allow" row on Windows | Irrelevant | Show per platform |

## 4. Hub and Android scope (simplicity)

- Cut: stats ("typing saved" has no defined formula), language chips (defer until the endpoint supports it),
  search, Shortcuts page, avatar footer, nav count.
- Keep: readiness, showing **failing rows only**, collapsing to "Ready · Ctrl Space"; History with Copy, failed jobs pinned
  as a "Needs you" group; Settings (hotkey, mic, account, retention).
- Desktop: tray icon with readiness dot + one window (status, history, settings), not a dashboard with a sidebar.
- Android: keyboard bar is the product; Home is a setup checklist that becomes History once done.
- Make History read as a dated running transcript (a page of writing), not rows of buttons.

## 5. What reads as "vibecoded"

Pill badges and pills with soft tinted fills, three-stat tile rows, cards inside cards, green check circles,
sidebar + avatar footer, "Ready when you are". Replace with typographic hierarchy, hairline rules, one accent used
only for the one action.

## 6. Colour, type and accessibility bugs

- C's accent `#B8321A` is effectively its danger `#A0281A`: every main action reads as an error (blocker).
  B's accent and danger are also too close. Make the action colour ink and keep red for record and error only.
- C's 48px numerals never render (`.stat .n` 28px rule wins); C's "square corners" are false (pills at 999px);
  `.fn` numbering leaks into A and B.
- 11.5px text breaks the 12px floor (badges, target label, nav count). Android buttons ~30px, keys 38px, mic 44px: all under 48dp.
- Secondary strip text dimmed with opacity; use solid tokens. Status shown by colour alone in several places.
- A's success tint is nearly invisible; C's dim second lines and yellow-on-black badges are hard to read; C wraps on phones.

## 7. The quill as a system (design critic)

Real nib pointing in the writing direction; the stroke continues while Working and settles into a signed mark on Done;
guaranteed minimum width on Android; driven by the live mic level, not a static path.
