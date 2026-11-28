//! The strip's window decisions, kept apart from Win32 and GPUI so each one has a test
//! (docs/regressions.md). The OS layers (`win.rs`, `main.rs`) only carry them out.

#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicBool, Ordering};

pub const WM_POWERBROADCAST: u32 = 0x0218;
pub const WM_DISPLAYCHANGE: u32 = 0x007E;
#[cfg(test)]
const PBT_APMSUSPEND: usize = 0x0004;
pub const PBT_APMRESUMESUSPEND: usize = 0x0007;
pub const PBT_APMPOWERSTATUSCHANGE: usize = 0x000A;
pub const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
pub const SW_HIDE: i32 = 0;
pub const SW_SHOWNOACTIVATE: i32 = 4;

/// Why the strip's graphics may be gone after this window message (it went invisible after
/// Modern Standby, a charger change on a two-GPU laptop, a display change), or None.
pub fn graphics_maybe_lost(msg: u32, wparam: usize) -> Option<&'static str> {
    match (msg, wparam) {
        (WM_POWERBROADCAST, PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC) => Some("woke from sleep"),
        (WM_POWERBROADCAST, PBT_APMPOWERSTATUSCHANGE) => Some("power source changed"),
        (WM_DISPLAYCHANGE, _) => Some("display changed"),
        _ => None,
    }
}

/// The "rebuild the strip" mark. The OS messages only set it; the rebuild happens when the user
/// presses the shortcut (or resumes a paused take), never on its own: full-screen games change the
/// display mode constantly, and a rebuild per change stole their focus in a loop.
pub struct WakeFlag(AtomicBool);

impl WakeFlag {
    pub const fn new() -> Self {
        Self(AtomicBool::new(false))
    }
    /// Marks the strip for a rebuild; true only for the first mark since the last take.
    pub fn mark(&self) -> bool {
        !self.0.swap(true, Ordering::Relaxed)
    }
    /// At the shortcut: true once after any number of marks.
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}

/// ShowWindow's command for the strip: shown without activating, or hidden. Never an activating
/// show (SW_SHOW, SW_SHOWNORMAL, SW_RESTORE), which would take focus from the app being dictated into.
pub fn show_command(show: bool) -> i32 {
    if show { SW_SHOWNOACTIVATE } else { SW_HIDE }
}

/// The strip's own posted message (WM_APP + 0x156): a move, show or hide asked from inside GPUI,
/// carried out from the message loop (win.rs post_strip).
pub const WM_STRIP: u32 = 0x8000 + 0x156;

/// What a posted strip message asks for (its wParam).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StripAsk {
    Place = 0,
    Show = 1,
    Hide = 2,
}

impl StripAsk {
    pub fn visibility(show: bool) -> Self {
        if show { StripAsk::Show } else { StripAsk::Hide }
    }
}

/// From the message loop, for a posted strip message `code` and whether the strip is visible
/// now: (move it home first, the ShowWindowAsync command). A show of a hidden strip places it
/// first, so it never appears at an old spot; a hide never moves it.
pub fn on_strip_message(code: usize, visible: bool) -> (bool, Option<i32>) {
    match code {
        c if c == StripAsk::Place as usize => (true, None),
        c if c == StripAsk::Show as usize => (!visible, Some(show_command(true))),
        c if c == StripAsk::Hide as usize => (false, Some(show_command(false))),
        _ => (false, None),
    }
}

/// The strip's home: bottom centre of the main screen, 64 logical px above its bottom edge.
/// `monitor` is (left, top, right, bottom), `size` the outer (w, h) in physical px, frame included.
pub fn home_position(monitor: (i32, i32, i32, i32), size: (i32, i32), scale: f32) -> (i32, i32) {
    let (l, _t, r, b) = monitor;
    let (w, h) = size;
    let inner_h = (84.0 * scale).round() as i32;
    let frame = ((h - inner_h) / 2).max(0);
    (l + (r - l - w) / 2, b - (64.0 * scale).round() as i32 - inner_h - frame)
}

/// Whether the strip must be moved to `home`: only when it is really out of place (a screen
/// change, or a rebuilt window carrying an old rectangle that is now off screen). A needless move
/// while GPUI handles the shortcut made it log "RefCell already borrowed" and skip the show.
/// The strip's one size in logical pixels (main.rs strip_options).
pub const STRIP_SIZE: (f32, f32) = (760.0, 84.0);

/// The strip's size in physical pixels when the window has another size (more than a few pixels
/// off, so a frame or rounding never causes a resize), else None.
pub fn fix_size(now: (i32, i32), scale: f32) -> Option<(i32, i32)> {
    let want = ((STRIP_SIZE.0 * scale).round() as i32, (STRIP_SIZE.1 * scale).round() as i32);
    ((now.0 - want.0).abs() > 16 || (now.1 - want.1).abs() > 16).then_some(want)
}

pub fn needs_move(now: (i32, i32), home: (i32, i32)) -> bool {
    (now.0 - home.0).abs() > 8 || (now.1 - home.1).abs() > 8
}

/// What a strip rebuild does once it tried to open the new window.
#[derive(Debug, PartialEq)]
pub enum AfterOpen {
    /// The new window could not be opened: keep the old strip as it is (never blank).
    KeepOld,
    /// A take is live: show the new strip now, without activating.
    Show,
    /// Nothing needs the strip: paint one blank frame unseen, then hide.
    PaintBlankThenHide,
}

pub fn after_open(opened: bool, take_live: bool) -> AfterOpen {
    match (opened, take_live) {
        (false, _) => AfterOpen::KeepOld,
        (true, true) => AfterOpen::Show,
        (true, false) => AfterOpen::PaintBlankThenHide,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strip invisible after sleep, a charger change (two-GPU laptop) or a display change.
    #[test]
    fn sleep_charger_and_display_changes_mark_the_strip_for_a_rebuild() {
        assert_eq!(graphics_maybe_lost(WM_POWERBROADCAST, PBT_APMRESUMESUSPEND), Some("woke from sleep"));
        assert_eq!(graphics_maybe_lost(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC), Some("woke from sleep"));
        assert_eq!(graphics_maybe_lost(WM_POWERBROADCAST, PBT_APMPOWERSTATUSCHANGE), Some("power source changed"));
        assert_eq!(graphics_maybe_lost(WM_DISPLAYCHANGE, 0), Some("display changed"));
        // Going to sleep or any other message changes nothing.
        assert_eq!(graphics_maybe_lost(WM_POWERBROADCAST, PBT_APMSUSPEND), None);
        assert_eq!(graphics_maybe_lost(0x0201, 0), None);
    }

    /// The WM_DISPLAYCHANGE rebuild stole focus from full-screen games in a loop: any number of
    /// display changes only mark the strip, and the shortcut rebuilds it once.
    #[test]
    fn display_changes_only_mark_the_strip_and_the_shortcut_rebuilds_once() {
        let f = WakeFlag::new();
        assert!(!f.take(), "nothing to rebuild before any change");
        assert!(f.mark(), "the first change is logged");
        for _ in 0..500 {
            assert!(!f.mark(), "a game flipping display modes only re-marks, it never rebuilds");
        }
        assert!(f.take(), "the next shortcut press rebuilds");
        assert!(!f.take(), "and only once");
    }

    /// The strip is never shown with an activating flag.
    #[test]
    fn the_strip_is_never_shown_with_an_activating_flag() {
        assert_eq!(show_command(true), SW_SHOWNOACTIVATE);
        assert_eq!(show_command(false), SW_HIDE);
        for activating in [1, 3, 5, 9, 10] {
            assert_ne!(show_command(true), activating);
        }
    }

    /// RefCell already borrowed on the first strip show: no move unless off by more than 8 px.
    #[test]
    fn the_strip_is_moved_only_when_more_than_8_px_out_of_place() {
        let home = home_position((0, 0, 1920, 1080), (760, 84), 1.0);
        assert_eq!(home, (580, 932));
        assert!(!needs_move(home, home));
        assert!(!needs_move((home.0 + 8, home.1 - 8), home), "rounding wobble is not a move");
        assert!(needs_move((home.0 + 9, home.1), home));
    }

    /// "RefCell already borrowed" at 1.0.0 startup: the strip's moves and shows are posted and
    /// decided from the message loop. A hidden strip is placed before it shows, a hide never
    /// moves, and every show is a no-activate one.
    #[test]
    fn the_strip_is_placed_before_it_shows_and_never_activates() {
        assert_eq!(on_strip_message(StripAsk::Place as usize, false), (true, None));
        assert_eq!(on_strip_message(StripAsk::Place as usize, true), (true, None));
        assert_eq!(on_strip_message(StripAsk::Show as usize, false), (true, Some(SW_SHOWNOACTIVATE)));
        assert_eq!(on_strip_message(StripAsk::Show as usize, true), (false, Some(SW_SHOWNOACTIVATE)));
        assert_eq!(on_strip_message(StripAsk::Hide as usize, true), (false, Some(SW_HIDE)));
        assert_eq!(on_strip_message(StripAsk::Hide as usize, false), (false, Some(SW_HIDE)));
        assert_eq!(on_strip_message(7, true), (false, None), "an unknown code does nothing");
        assert_eq!(StripAsk::visibility(true), StripAsk::Show);
        assert_eq!(StripAsk::visibility(false), StripAsk::Hide);
        assert!((0x8000..=0xBFFF).contains(&WM_STRIP), "WM_STRIP is in the WM_APP range");
    }

    /// The same bug, structurally: win.rs's public strip calls only post; the synchronous
    /// SetWindowPos and ShowWindowAsync on the strip run only from the posted message.
    #[test]
    fn strip_moves_and_shows_are_posted_never_synchronous() {
        let win = include_str!("win.rs").replace("\r\n", "\n");
        let body = |name: &str| {
            let start = win.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("fn {name}"));
            let end = start + win[start..].find("\n}\n").expect("end of fn");
            win[start..end].to_string()
        };
        for f in ["strip_visible", "place_strip", "move_strip"] {
            let b = body(f);
            assert!(b.contains("post_strip("), "{f} posts");
            assert!(!b.contains("SetWindowPos") && !b.contains("ShowWindow"), "{f} never moves or shows synchronously");
        }
        assert!(body("no_activate_proc").contains("strip_message(hwnd, wparam)"), "the window procedure carries the message out");
    }

    /// A rebuilt strip carrying an old rectangle that is now off screen (a monitor unplugged, the
    /// resolution lowered) is put back at the main screen's bottom centre.
    #[test]
    fn a_rebuilt_strip_with_off_screen_bounds_goes_back_to_the_bottom_centre() {
        let monitor = (0, 0, 1366, 768);
        let home = home_position(monitor, (760, 84), 1.0);
        let old_on_a_gone_4k_screen = (2500, 2000);
        assert!(needs_move(old_on_a_gone_4k_screen, home));
        assert!(home.0 >= 0 && home.0 + 760 <= 1366 && home.1 >= 0 && home.1 + 84 <= 768, "home is on screen: {home:?}");
        // Scaled 150 %: still on screen and above the bottom edge.
        let (x, y) = home_position((0, 0, 2880, 1800), (1140, 126), 1.5);
        assert!(x >= 0 && y + 126 <= 1800);
    }

    /// strip_blank stuck if open_window failed: a failed open keeps the old strip and never blanks.
    #[test]
    fn a_failed_rebuild_keeps_the_old_strip_and_never_leaves_it_blank() {
        assert_eq!(after_open(false, true), AfterOpen::KeepOld);
        assert_eq!(after_open(false, false), AfterOpen::KeepOld);
        assert_eq!(after_open(true, true), AfterOpen::Show);
        assert_eq!(after_open(true, false), AfterOpen::PaintBlankThenHide);
    }

    /// The rebuild runs only at the user's shortcut press or a resume, never from the window
    /// procedure that sees the display change (it stole focus from games in a loop). Structural:
    /// the OS message handler may only mark, and the rebuild has exactly these two callers.
    #[test]
    fn the_strip_is_rebuilt_only_from_the_shortcut_or_a_resume() {
        let main = include_str!("main.rs");
        assert_eq!(main.matches("rebuild_strip_if_woke(cx)").count(), 2, "callers changed: check none runs on an OS event");
        let win = include_str!("win.rs").replace("\r\n", "\n");
        let start = win.find("fn no_activate_proc").expect("no_activate_proc");
        let end = start + win[start..].find("\n}\n").expect("end of no_activate_proc");
        let body = &win[start..end];
        assert!(body.contains("WOKE.mark()"), "the window procedure marks the strip");
        assert!(!body.contains("open_window") && !body.contains("rebuild_strip"), "the window procedure never rebuilds");
    }

    #[test]
    fn a_strip_that_came_out_at_the_default_window_size_is_put_back_to_its_own() {
        // Seen on the owner's laptop: 1280 x 745 logical (1920 x 1118 at 1.5x), mostly off screen.
        assert_eq!(fix_size((1920, 1118), 1.5), Some((1140, 126)));
        assert_eq!(fix_size((1280, 745), 1.0), Some((760, 84)));
        // The right size, give or take a frame or rounding, is left alone.
        assert_eq!(fix_size((1140, 126), 1.5), None);
        assert_eq!(fix_size((762, 90), 1.0), None);
        // The size in main.rs is the one fix_size restores.
        let main = include_str!("main.rs");
        assert!(main.contains("size(px(760.), px(84.))"), "STRIP_SIZE matches strip_options");
    }
}
