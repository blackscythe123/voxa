# UI critique of v7 native screens (2026-11-22)

Reviewed: `docs/ui-mocks/v7-native.html`, screens `docs/ui-mocks/v4-refs/v7-phone1..4.png`, `v7-desk1..3.png`.
Three EGM reviews: platform fidelity (Material 3 / Windows 11), first-time user, simplicity/scope.
(A first round was void: the screenshots handed to reviewers were blank captures. Recaptured and verified before this round.)

## Verdict

- v7 is a clear step up in realism. The Windows window (Home, Settings) nearly passes as WinUI; the phone app screens pass as
  "M3 by the book" but generic.
- **The two surfaces users see most still give it away:** the Windows strip and the Android keyboard recording bar.
- **The realism pass undid the v6 simplifications:** a three-tab Android app, a Windows Home that duplicates Settings.
- **Most strip / keyboard states are still only captions.** The core deliverable (every state drawn) is missing.
- **Brand barely survives:** the Fraunces wordmark renders in Roboto (CSS specificity bug, verified), the scheme reads blush
  not sepia, the ink line reads as a decorative sine wave and the nib as a drawing tool.

## 1. Surfaces that still look fake (fidelity)

| Surface | Problem | Fix |
|---|---|---|
| Windows strip | Solid dark-brown pill, 26px radius, pill buttons, in a light session; not a Windows 11 surface; no app name; overlaps the window | Acrylic, theme-aware, 8px radius Fluent overlay; 4px buttons (subtle Cancel, accent Stop); Voxa mark + target-app icon; clean gap above the taskbar; "Ctrl+Space to stop" hint |
| Android recording bar | Near-black pill inside a light keyboard looks like an injected media player; off-scheme coral stop; monospace timer; no Voxa mark | Draw at toolbar height on the keyboard's own surface; stop in primary/error container; Roboto tabular numerals; Voxa mark |
| Keyboard keys | Drop shadows, 7px radius, wide gaps, no suggestion strip, idle mic never shown | Flat keys, current Gboard/HeliBoard proportions, suggestion strip, Voxa mic in the toolbar with readiness dot |
| Host apps | Coloured-square taskbar icons, fake WhatsApp, lavender chat app with M3 baseline colours | Credible host apps and real app icons (mockup only) |
| Colour | M3 tonal palette from a brown seed produced blush/peach neutrals; mint checks and coral off-scheme | Hand-tune neutrals to warm paper grey; brown only for primary |
| Wordmark | `.tab-small .title` overrides `.wordmark`, so Fraunces never renders | Fix specificity; Large top app bar with Fraunces |
| Ink line | Even sine, constant amplitude, ~10px nib | Speech-like envelope with bursts and flat stretches; 16px+ nib, or drop the nib (first-time user read it as a drawing tool) |
| Live text | Phone 1 shows partial text appearing while recording; the product inserts text after Stop | Show an empty or unchanged field while recording |

## 2. Structure (simplicity)

- **Android: one screen, no nav bar.** Setup checklist until done, then History with failed jobs pinned on top; gear in the
  top app bar opens Settings; account lives in Settings. (M3: a nav bar is for 3+ peer destinations; Voxa has 2.)
- **Windows: two pages.** History is the landing page with InfoBars on top and a one-line "Ready · Ctrl Space" when nothing
  is wrong; Settings in the NavigationView footer. Drop Home (its cards belong in Settings, its list is History).
- Cut: Android search and overflow menu, word counts, the progress bar (it repeats the checklist), the History nav badge,
  the "Paste into the app I started in" toggle (it is a guarantee, not a preference).

## 3. Missing states (the actual deliverable)

Draw, on both platforms, with a dismiss on every Needs-you variant:
Idle mic with readiness dot (Android), Working, Done, offline (**records anyway, sends when connected**: v7's caption
contradicted the spec), couldn't reach ChatGPT, server error / timeout retry, no speech, target changed ("Paste in X"),
mic blocked on the strip and keyboard, session expired mid-dictation in the strip. Also the Android Settings screen
and the Windows History page.

## 4. Consistency and trust (first-time user)

- **One sign-in state everywhere.** v7 shows a tick, "session ended", "Sign out" and "Sign in again" for the same account.
- **Explain the ChatGPT login** where it is asked for: why, what is sent ("only what you say while the mic is on"), who makes Voxa.
- **"Sign in and send" sounds like it sends the email** and, from History, would paste into the wrong app.
  Use "Sign in" then Copy in History; the strip keeps the auto-resend.
- **One message per problem.** Windows Home shows the mic problem twice and sign-in twice. Mic blocked goes first (it blocks everything).
- **Recent is full before setup is done.** Show an empty state until the keyboard is on.
- **Phone 2's grey mic is the chat app's voice-note mic,** not Voxa's: confusing.
- Small: consistent timestamps; no "replay" promise without a replay control; disabled controls drawn disabled;
  InfoBars with filled severity glyphs and a close; no button stacked under an M3 list item; search out of the nav slot.
