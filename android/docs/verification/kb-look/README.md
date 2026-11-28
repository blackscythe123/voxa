# Keyboard look: verification (Android, API 35 emulator)

`kb` = the real Voxa keyboard over Contacts, `screen` = Settings, Keyboard look (preview is the real
KeyboardPanel). Shots 01 to 17 were taken on emulator-5554. Another agent then began installing and
force-stopping the app on 5554, so shots 08 and 10, 11 and 18 to 29 were retaken on a separate
emulator from the same system image (Pixel 7, API 35). All shots are scaled to 720 px wide.

| Check | Evidence |
|---|---|
| Refresh fix: Appearance changed while the keyboard process lives, keyboard picks it up on next show (same pid; log "look changed: keyboard view rebuilt") | 01-refresh-a-dark-start, 01-refresh-b-light-after-appearance |
| Bug found and fixed on the way: a rebuilt panel lost its navigation bar inset | 01-refresh-x-first-try-inset-bug (before the fix) |
| Look change reaches the live keyboard (Black picked, then shown) | 03-screen-black, 03-kb-black; 28-screen-try-it-with-real-keyboard |
| Voxa light / dark (unchanged v6 look) | 02-screen-voxa-light, 07-screen-voxa-dark, 07-kb-voxa-dark, 01-refresh-* |
| Presets | 03 Black, 04 Sand, 24-kb-paper (keys a step darker on a very light colour) |
| Phone colours light / dark, mic takes the system accent | 05-*, 06-* |
| Photo through the real system Photo Picker; EXIF-rotated 4000 x 3000 JPEG saved upright as 1440 x 1080 WebP | 08-picker, 09-* |
| Photo strength low (8 %) and high (capped at 36 % on the busy photo; drag stops at the cap, dot marks it) | 10-*, 11-* |
| Preview Ready / Listening / Done on a photo (green title falls back to text colour on a light wash) | 09-screen, 12-screen-photo-listening, 12-screen-photo-done |
| Real keyboard Ready / Listening (bars in text colour at 50 %, "Tap to stop") / Done ("Pasted in Contacts", text inserted) on a photo | 16-kb-photo-sepia-soft-* |
| Soften photo off / on | 15-screen-photo-sepia-soften-off, -on |
| Key shapes on / off (photo and Voxa) | 16-* (on), 17-* (off, photo), 25-* (off, Voxa) |
| Custom mic colour (Ocean); its ring switches to the text colour because blue on dark brown is below 3:1 | 17-* |
| Drag to move the photo (position stored 0.5 to 0.0, keyboard shows the top of the photo) | 18-* |
| Change / Remove photo sheet; Remove deletes the copy | 13-screen-photo-sheet, 26-screen-after-remove-photo |
| Dark Appearance on a photo: same photo, stronger wash | 21-* |
| Settings row value "Your photo" | 22-settings-row-your-photo |
| Broken file picked: refused with a message, previous photo kept, no crash | 19-picker-with-broken, 19-screen-broken-photo-refused |
| Saved copy corrupted on disk: decode fails, falls back to Voxa, prefs and files cleared, process alive, Voxa still the IME | 20-kb-corrupt-copy-falls-back-to-voxa |
| Saved copy deleted: falls back to Voxa when the colours are built | 23-kb-missing-copy-falls-back-to-voxa |
| Reset to Voxa look deletes the copy and every choice | 27-* |
| Try it field above the real keyboard, dictation lands in it; "Switch to Voxa" opens the system picker | 28-*, 29-switch-to-voxa-picker |
| Instrumented tests: 12 of 12 pass (6 core, 6 Keyboard look) | instrumented-tests-api35.xml |

## Size

arm64 release, built from clean worktrees: before 5,306,867 B (5e0041b), after 5,357,495 B
(6a950a2), so +50.6 KB (classes.dex +56 KB uncompressed). That is above the 10 to 25 KB estimate.
Replacing Material's Slider with a small one saved about 16 KB. The rest is the new screen, the
Photo Picker contract and the gesture code.

## Not verified

- Keyboard PSS with a photo shown, and PSS after the change at all. The before value was 33.1 to
  37.5 MB (release, emulator-5554, shown idle over Contacts). The after values were not measured
  because the owner asked to wrap up.
- A real HEIC photo and a phone (only the emulator, with synthetic JPEG photos).
- onTrimMemory releasing the photo (code only).
- Phone colours after a wallpaper change (code only: the stamp includes the system accent).
- TalkBack on the new screen.
