# Desktop review, round 3 (2026-11-22)

Two EGM reviews of the current app (screens in this folder and `../desktop-v2`).

## A. Features users expect that Voxa lacks (ranked by value / effort)

1. Hold-to-talk alongside press-to-start/stop. High / Low.
2. Personal word list: local find-and-replace after transcription ("clawed code" -> "Claude Code"),
   learned from edits in History ("Always fix this?"). The transcription API takes no prompt or
   vocabulary, so this is the workaround. High / Low.
3. Pick the mic inside Voxa with a level meter and "can't hear you" warning. High / Low-Med.
4. Paste the last dictation again (shortcut). Med / Low.
5. Spoken formatting ("new line", "new paragraph", "bullet"), opt-in, local. Med / Low-Med.
6. Voice snippets ("my address" expands). Med / Low.
7. Faster long dictations: upload segments at pauses while still talking (no live streaming with
   this API). High / Med; rate-limit and boundary risks.
8. Per-app rules (no trailing period in terminals, lowercase in chats). Med / Med.
9. Language: cannot be forced; offer Retry when the script is unexpected.
10. AI rewrite / command mode: not possible through the Whisper endpoint; defer.
11. Transcribe a dropped audio file. Low-Med / Med.
12. Start/stop sounds. Med / Low.

## B. Engagement without dark patterns

Avoid: streak-loss notifications, streak freezes, leaderboards, auto-share, confetti per dictation,
inflated "time saved", random badges, comeback nagging.

Build:
1. Ink milestones at 1k / 10k / 50k / 100k / 250k words: a one-time quill flourish on the strip and
   a card on Home ("100,000 words, about a 400-page book").
2. Monday recap card on Home (words, time saved, top app, longest, busiest day), "Copy as image"
   with numbers only. No notifications.
3. Forgiving streak (weeks with 3+ dictation days, or one rest day a week), inkwell visual, resets
   silently.
4. Pen sounds: pen-down on start, pen-lift on finish, soft scratch on error; one toggle and volume.
5. "Firsts" tips (first dictation in a new app) and one Easter egg ("testing testing one two three"
   -> "Loud and clear", nothing pasted).

## C. Week-one churn risks

Start with Windows off; Ctrl+Space clashes (IME switch, VS Code suggestions); shortcut shown
inconsistently; false "You moved to Voxa" (fixed in 0.3.3); imported history dominating Insights as
"Unknown app"; wrong default mic / invented text from silence; silent sign-in expiry; paste failures
in elevated apps and password fields; first dictation feeling slow; privacy question (do dictations
appear in ChatGPT history?).

## D. Tour as a real product walkthrough (coach marks)

Steps run on the real UI and advance when the user does the thing: sign in (advance on success) ->
mic check (live meter, advance on sound, "can't hear you" after 4 s) -> press your shortcut (advance
on the hotkey event; detects conflicts after 8 s) -> first dictation into a practice field inside
Voxa (must be an allowed paste target) -> the strip, live (callout follows the real strip state) ->
optional first dictation in a real app -> History (new entry, Copy) -> Settings basics (shortcut,
filler words, keep history) -> tray (callout at the icon; overflow fallback), Start with Windows.

Visuals: 55% dim with a cutout and 2 px sepia ring around the measured target, 320 px callout with
an ink-stroke progress line, Back / Skip / Next (Next only on explain steps), 220 ms moves, reduced
motion respected. Outside the window (strip, tray): no dimming, a ring window and a click-through
callout. Re-entry from Settings and the tray; one-time contextual tips (first "Not pasted", first
offline, first long take, shortcut conflict), max one tip a day, "Don't show tips". Measure
step funnel, time to first practice paste (target: 80% within 90 s), day-1/day-7 retention by
tour outcome, tip action rates.
