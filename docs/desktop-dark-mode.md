# Desktop dark mode

Voxa for Windows and macOS (the GPUI app in `desktop/app`) has a light and a dark palette. Every colour the app draws lives in one file, `desktop/app/src/theme.rs`; pages read the current palette with `theme::p()`.

## How it picks light or dark

- Settings, System, **Appearance**: System (the default), Light, Dark. Stored in `settings.json` as `"appearance": "system" | "light" | "dark"`. Settings files without the field, or with a value this version doesn't know, load as System and keep their other settings.
- **System** follows the OS: GPUI's `cx.window_appearance()` at start, and `observe_window_appearance` while running (Windows: the `ImmersiveColorSet` setting change; macOS: the window's effective appearance).
- A change applies live, without a restart: the window re-reads `settings.json` (it already did, four times a second) and repaints; the tray process gets the window's `ReloadSettings` command and repaints the strip.
- **Windows caption**: `os::window_caption` sets `DWMWA_USE_IMMERSIVE_DARK_MODE` (20) to match the palette, then the caption colour (35) and caption text colour (36) to the nav colour and text colour. GPUI resets attribute 20 to the system's mode after an OS switch, so Voxa sets it again 250 ms later.
- **macOS title bar**: `os::window_follows` sets the NSWindow's appearance to nil for System, `NSAppearanceNameAqua` for Light and `NSAppearanceNameDarkAqua` for Dark, so the native title bar matches.
- The brand mark on the strip (the sepia "v" tile) and app badges keep their colours in both modes, like the app icon.
- The sign-in window (`desktop/signin`) shows ChatGPT's own page in WebView2/WKWebView, so there are no Voxa colours to theme there.

## Palette

| Role | Light | Dark | Notes |
|---|---|---|---|
| Page | `#FBFAF8` | `#242120` | v7 mock-up |
| Nav, caption bar | `#F1EEEA` | `#1C1A18` | v7 mock-up |
| Card, inputs, white buttons | `#FFFFFF` | `#2D2A28` | v7 mock-up |
| Text | `#1A1A1A` | `#F2F0ED` | v7 mock-up |
| Softer body text | `#3D3D3D` | `#DAD5CF` | |
| Muted text | `#555555` | `#C2BCB5` | Mock-up `#B9B3AC` is 6.86:1 on the dark card; lifted to clear AAA |
| Sepia (links, primary buttons, ink line, caret) | `#7A3F22` | `#E0A07C` | Android's `#D9946D` is 5.71:1 on the lighter desktop card; `#E0A07C` is the same hue lifted to 6.44 (card) to 7.84 (nav), the Android range |
| Text on sepia | `#FFFFFF` | `#24140C` | |
| Hover | `#F6F4F1` | `#363230` | |
| Success text and icon | `#0E5C0E`, `#0F7B0F` | `#45C27C` | |
| Warning text | `#9D5D00` | `#E3A44A` | |
| Error text | `#C42B1C`, `#B3241B` | `#F07A70` | |
| Danger button | `#C42B1C`, white text | `#C42B1C`, white text | Same in both: white on red is 5.66:1 |
| Error InfoBar, glyph | `#FDE7E9`, `#C42B1C` | `#442726`, `#FF99A4` | v7 mock-up |
| Warning InfoBar, glyph | `#FFF4CE`, `#9D5D00` | `#433519`, `#E3A44A` | |
| Insights bars | `#DCCBBE`, highlighted sepia | `#9A6F57`, highlighted sepia | |
| Strip | `#FBFAF8` at 95 % | `#2A2725` at 95 % | |

Light mode changes, all from folding near-identical values into one palette: the strip's background is the page colour `#FBFAF8` (was `#F9F7F4`, which left the strip's muted text at 6.97:1), the strip's warning title uses the app's warning text `#9D5D00` (was `#7A5100`), the strip's "no speech" glyph uses the muted colour, borders at 12 to 13 % black alpha share one value, the input placeholder uses the muted text colour (was black at 20 %, 1.6:1), the text-selection fill is a sepia wash (was blue) and the caret is sepia (was blue).

## Contrast, WCAG 2.x

Ratios are computed with the WCAG 2.x relative-luminance formula. Colours with alpha are flattened onto what sits under them (the page for borders and the strip). Targets: body text AAA (7:1), muted text AAA (7:1), other text and status text AA (4.5:1), icons, glyphs and component edges 3:1 (WCAG 1.4.11). `desktop/app/src/theme.rs` has a unit test that checks the text, status and icon pairs below, so a palette edit can't quietly break them.

| Pair | Target | Light | Dark |
|---|---|---|---|
| Body text on page | body text, AAA 7 | `#1A1A1A` on `#FBFAF8` **16.68** ok | `#F2F0ED` on `#242120` **14.06** ok |
| Body text on nav (nav items, wordmark) | body text, AAA 7 | `#1A1A1A` on `#F1EEEA` **15.05** ok | `#F2F0ED` on `#1C1A18` **15.25** ok |
| Body text on card | body text, AAA 7 | `#1A1A1A` on `#FFFFFF` **17.40** ok | `#F2F0ED` on `#2D2A28` **12.53** ok |
| Softer body text on card (status lines, tour copy) | body text, AAA 7 | `#3D3D3D` on `#FFFFFF` **10.86** ok | `#DAD5CF` on `#2D2A28` **9.77** ok |
| Softer body text on page | body text, AAA 7 | `#3D3D3D` on `#FBFAF8` **10.41** ok | `#DAD5CF` on `#242120` **10.96** ok |
| Text on a hovered button or row | body text, AAA 7 | `#1A1A1A` on `#F6F4F1` **15.85** ok | `#F2F0ED` on `#363230` **11.15** ok |
| Text on the error InfoBar | body text, AAA 7 | `#1A1A1A` on `#FDE7E9` **14.74** ok | `#F2F0ED` on `#442726` **11.80** ok |
| Text on the warning InfoBar | body text, AAA 7 | `#1A1A1A` on `#FFF4CE` **15.82** ok | `#F2F0ED` on `#433519` **10.48** ok |
| Muted text on page | muted text, AAA 7 | `#555555` on `#FBFAF8` **7.15** ok | `#C2BCB5` on `#242120` **8.49** ok |
| Muted text on card | muted text, AAA 7 | `#555555` on `#FFFFFF` **7.46** ok | `#C2BCB5` on `#2D2A28` **7.57** ok |
| Input placeholder (muted) in an input | muted text, AAA 7 | `#555555` on `#FFFFFF` **7.46** ok | `#C2BCB5` on `#2D2A28` **7.57** ok |
| Sepia text on page ("See all in History", back link) | text, AA 4.5 | `#7A3F22` on `#FBFAF8` **7.86** ok | `#E0A07C` on `#242120` **7.22** ok |
| Sepia text on card ("Waiting for you") | text, AA 4.5 | `#7A3F22` on `#FFFFFF` **8.20** ok | `#E0A07C` on `#2D2A28` **6.44** ok |
| Sepia on nav | text, AA 4.5 | `#7A3F22` on `#F1EEEA` **7.09** ok | `#E0A07C` on `#1C1A18` **7.84** ok |
| Primary button text on sepia | text, AA 4.5 | `#FFFFFF` on `#7A3F22` **8.20** ok | `#24140C` on `#E0A07C` **8.04** ok |
| Primary button text, hovered | text, AA 4.5 | `#FFFFFF` on `#66341C` **10.13** ok | `#24140C` on `#EAB596` **9.78** ok |
| Danger button text | text, AA 4.5 | `#FFFFFF` on `#C42B1C` **5.66** ok | `#FFFFFF` on `#C42B1C` **5.66** ok |
| Danger button text, hovered | text, AA 4.5 | `#FFFFFF` on `#A32416` **7.44** ok | `#FFFFFF` on `#D6392A` **4.69** ok |
| "Ready to dictate" on card | text, AA 4.5 | `#0E5C0E` on `#FFFFFF` **8.22** ok | `#45C27C` on `#2D2A28` **6.29** ok |
| Warning text on card ("Not pasted", "Needs you first") | text, AA 4.5 | `#9D5D00` on `#FFFFFF` **5.25** ok | `#E3A44A` on `#2D2A28` **6.57** ok |
| Warning text on page | text, AA 4.5 | `#9D5D00` on `#FBFAF8` **5.03** ok | `#E3A44A` on `#242120` **7.37** ok |
| "Not sent" on card | text, AA 4.5 | `#C42B1C` on `#FFFFFF` **5.66** ok | `#F07A70` on `#2D2A28` **5.24** ok |
| Delete icon button on card | text, AA 4.5 | `#B3241B` on `#FFFFFF` **6.59** ok | `#F07A70` on `#2D2A28` **5.24** ok |
| Plan pill | text, AA 4.5 | `#0E6B4F` on `#E8F5EF` **5.79** ok | `#7FD9A8` on `#1F3A2C` **7.29** ok |
| "Signed in" pill | text, AA 4.5 | `#0E5C0E` on `#DFF6DD` **7.19** ok | `#8FD98A` on `#23391F` **7.43** ok |
| Strip: text | body text, AAA 7 | `#1A1A1A` on `#FBFAF8` **16.68** ok | `#F2F0ED` on `#2A2725` **13.04** ok |
| Strip: muted (app, detail) | muted text, AAA 7 | `#555555` on `#FBFAF8` **7.15** ok | `#C2BCB5` on `#2A2725` **7.88** ok |
| Strip: warning title | text, AA 4.5 | `#9D5D00` on `#FBFAF8` **5.03** ok | `#E3A44A` on `#2A2725` **6.84** ok |
| Strip: secondary button text on its face | body text, AAA 7 | `#1A1A1A` on `#FFFFFF` **17.40** ok | `#F2F0ED` on `#2D2A28` **12.53** ok |
| Caption bar title (Windows) | body text, AAA 7 | `#1A1A1A` on `#F1EEEA` **15.05** ok | `#F2F0ED` on `#1C1A18` **15.25** ok |
| Success icon on card | icon or component, 3 | `#0F7B0F` on `#FFFFFF` **5.44** ok | `#45C27C` on `#2D2A28` **6.29** ok |
| Error glyph on the error InfoBar | icon or component, 3 | `#C42B1C` on `#FDE7E9` **4.79** ok | `#FF99A4` on `#442726` **6.61** ok |
| Warning glyph on the warning InfoBar | icon or component, 3 | `#9D5D00` on `#FFF4CE` **4.77** ok | `#E3A44A` on `#433519` **5.49** ok |
| Mark inside the error glyph | icon or component, 3 | `#FFFFFF` on `#C42B1C` **5.66** ok | `#2A1A14` on `#FF99A4` **8.23** ok |
| Mark inside the warning glyph | icon or component, 3 | `#FFFFFF` on `#9D5D00` **5.25** ok | `#2A1A14` on `#E3A44A` **7.70** ok |
| Toggle off: knob on card | icon or component, 3 | `#5F5F5F` on `#FFFFFF` **6.39** ok | `#C2BCB5` on `#2D2A28` **7.57** ok |
| Toggle off: track outline on card | icon or component, 3 | `#757575` on `#FFFFFF` **4.61** ok | `#9F9D9C` on `#2D2A28` **5.28** ok |
| Toggle on: knob on sepia | icon or component, 3 | `#FFFFFF` on `#7A3F22` **8.20** ok | `#24140C` on `#E0A07C` **8.04** ok |
| Toggle on: sepia track on card | icon or component, 3 | `#7A3F22` on `#FFFFFF` **8.20** ok | `#E0A07C` on `#2D2A28` **6.44** ok |
| Words icon (sepia) on its wash | icon or component, 3 | `#7A3F22` on `#F3E7DE` **6.75** ok | `#E0A07C` on `#45322A` **5.44** ok |
| Insights: highlighted bar on card | icon or component, 3 | `#7A3F22` on `#FFFFFF` **8.20** ok | `#E0A07C` on `#2D2A28` **6.44** ok |
| Insights: other bars on card (WCAG 1.4.11 graphic) | icon or component, 3 | `#DCCBBE` on `#FFFFFF` **1.58** BELOW | `#9A6F57` on `#2D2A28` **3.25** ok |
| Insights: zero line on card | decorative, none | `#CFC6BD` on `#FFFFFF` **1.68** | `#5C5550` on `#2D2A28` **1.95** |
| Input focus outline (sepia) on page | icon or component, 3 | `#7A3F22` on `#FBFAF8` **7.86** ok | `#E0A07C` on `#242120` **7.22** ok |
| Input caret (sepia) | icon or component, 3 | `#7A3F22` on `#FFFFFF` **8.20** ok | `#E0A07C` on `#2D2A28` **6.44** ok |
| Input outline at rest on page (WCAG 1.4.11 boundary) | icon or component, 3 | `#DCDBD9` on `#FBFAF8` **1.33** BELOW | `#474544` on `#242120` **1.68** BELOW |
| Card hairline on page | decorative, none | `#ECEBE9` on `#FBFAF8` **1.14** | `#333130` on `#242120` **1.24** |
| Strip: recording dot | icon or component, 3 | `#E5483D` on `#FBFAF8` **3.78** ok | `#F2555A` on `#2A2725` **4.40** ok |
| Strip: paused glyph | icon or component, 3 | `#8A8A8A` on `#FBFAF8` **3.31** ok | `#9A948E` on `#2A2725` **4.95** ok |
| Strip: spinner and ink line (sepia) | icon or component, 3 | `#7A3F22` on `#FBFAF8` **7.86** ok | `#E0A07C` on `#2A2725` **6.70** ok |
| Strip: done check | icon or component, 3 | `#0F7B0F` on `#FBFAF8` **5.21** ok | `#45C27C` on `#2A2725` **6.54** ok |
| Strip: error glyph | icon or component, 3 | `#C42B1C` on `#FBFAF8` **5.43** ok | `#F07A70` on `#2A2725` **5.46** ok |
| Strip: quill | icon or component, 3 | `#2D2418` on `#FBFAF8` **14.62** ok | `#EDE4DA` on `#2A2725` **11.81** ok |

### Result

- Every text pair meets its target in both modes: body text is AAA everywhere (lowest 9.77:1 dark, 10.41:1 light), muted text is AAA (lowest 7.15:1 light on the page, 7.57:1 dark on a card), status, accent and button text is at least AA (lowest 5.03:1 light, warning text on the page; 4.69:1 dark, white on the hovered danger button).
- Muted text is never drawn on the nav. If it ever is, light `#555555` on `#F1EEEA` is 6.45:1 (AA, not AAA).
- Below 3:1, both pre-existing light-mode choices left as they were: the Insights bars that aren't highlighted (`#DCCBBE`, 1.58:1; each bar's numbers are also given on hover and click, and the axis has labels) and the outline of an input or drop-down at rest (1.33:1 light, 1.68:1 dark; they are recognised by their placeholder text and their place in a card, and the focused outline is sepia at 6.4:1 or better). Dark mode's ordinary bars are `#9A6F57` at 3.25:1.
- Card hairlines and the chart's zero line are decorative.
- The strip's background is 95 % opaque. The ratios use it over the page colour; over a very different desktop the muted text can move by a few hundredths.

## Checked

- `cargo test -q --release --workspace` and `cargo build --release --workspace` pass on Windows. The contrast test and the settings tests (old files, unknown values, round trip) run there.
- Windows 11, a dev copy on a throwaway `VOXA_DATA` folder with made-up History: the Home, History, Insights, Words, Detail and Settings pages and the tour card, captured with PrintWindow in light, then switched to dark and back to System by rewriting `settings.json` while the window stayed open (the window repainted each time, caption bar included). Started in dark as well. The strip, shown by the test-only `VOXA_TEST_STRIP=recording|paused|signed-out` hook (needs `VOXA_DATA`; it draws a stand-alone flow, so no microphone opens and nothing is sent), switched live from light to dark and back by the same `ReloadSettings` command the window sends. The strip never took focus.
- Not checked: macOS (the title bar appearance and the OS switch there are written to the AppKit API but have not been compiled or run on a Mac), and a live OS light/dark switch on Windows while Voxa runs (the OS was left in dark mode; System was checked to pick dark).
