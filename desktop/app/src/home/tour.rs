//! The first-run walkthrough: runs on the real pages, highlights the real element for each step,
//! and moves on when the user actually does the thing (signs in, speaks, presses the shortcut,
//! dictates, opens History...). Steps that only explain something have a Next button.

use super::*;
use gpui::{Bounds, Pixels, canvas};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Step {
    Welcome,
    SignIn,
    Mic,
    Shortcut,
    Practice,
    History,
    Detail,
    Words,
    Settings,
    Done,
}

const ORDER: [Step; 10] = [Step::Welcome, Step::SignIn, Step::Mic, Step::Shortcut, Step::Practice, Step::History, Step::Detail, Step::Words, Step::Settings, Step::Done];

pub struct Tour {
    pub step: Step,
    /// Newest history entry when the tour started: the practice dictation is the next one.
    pub since: u64,
    /// When the current step started (for "can't hear you" and similar hints).
    pub step_started: std::time::Instant,
    /// The step's goal is met; moving on shortly (lets the success state show).
    pub advancing: bool,
    pub autostart: bool,
}

impl Home {
    /// Where an element is drawn, kept for the tour's spotlight. Put inside a `.relative()` element.
    pub(super) fn mark(&self, key: &'static str) -> AnyElement {
        let store = self.marks.clone();
        canvas(
            move |b: Bounds<Pixels>, window, _| {
                let changed = store.borrow().get(key) != Some(&b);
                if changed {
                    store.borrow_mut().insert(key, b);
                    // A refresh asked for while drawing is dropped; ask for the next frame.
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
    }

    pub fn start_tour(&mut self, cx: &mut Context<Self>) {
        let since = self.voxa.read(cx).history.entries.last().map(|e| e.id).unwrap_or(0);
        self.tour = Some(Tour { step: Step::Welcome, since, step_started: std::time::Instant::now(), advancing: false, autostart: true });
        self.go(Page::Home, cx);
    }

    /// Developer switch (VOXA_TOUR=<n>): open the tour at step n, to check how each step looks.
    pub fn start_tour_at(&mut self, n: usize, cx: &mut Context<Self>) {
        self.start_tour(cx);
        if let Some(step) = ORDER.get(n) {
            self.tour_to(*step, cx);
        }
    }

    pub(super) fn tour_step(&self) -> Option<Step> {
        self.tour.as_ref().map(|t| t.step)
    }

    fn tour_to(&mut self, step: Step, cx: &mut Context<Self>) {
        let signed_in = self.voxa.read(cx).readiness().session == Session::Valid;
        let step = if step == Step::SignIn && signed_in { Step::Mic } else { step };
        self.voxa.update(cx, |v, cx| v.mic_test(step == Step::Mic, cx));
        if matches!(step, Step::Welcome | Step::SignIn | Step::Mic | Step::Shortcut | Step::Practice) && self.page != Page::Home {
            self.go(Page::Home, cx);
        }
        if let Some(t) = self.tour.as_mut() {
            t.step = step;
            t.step_started = std::time::Instant::now();
            t.advancing = false;
        }
        cx.notify();
    }

    pub(super) fn tour_next(&mut self, cx: &mut Context<Self>) {
        let Some(step) = self.tour_step() else { return };
        match ORDER.iter().position(|s| *s == step).and_then(|i| ORDER.get(i + 1)) {
            Some(next) => self.tour_to(*next, cx),
            None => self.tour_finish(false, cx),
        }
    }

    /// Goal met: show success briefly, then move on.
    fn tour_done_soon(&mut self, ms: u64, cx: &mut Context<Self>) {
        let Some(t) = self.tour.as_mut() else { return };
        if t.advancing {
            return;
        }
        t.advancing = true;
        let step = t.step;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(ms)).await;
            let _ = this.update(cx, |h, cx| {
                if h.tour_step() == Some(step) {
                    h.tour_next(cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn tour_finish(&mut self, skipped: bool, cx: &mut Context<Self>) {
        let autostart = self.tour.as_ref().map(|t| t.autostart).unwrap_or(true);
        self.tour = None;
        self.voxa.update(cx, |v, cx| {
            v.mic_test(false, cx);
            if !skipped && autostart != v.start_with_windows {
                v.set_autostart(autostart, cx);
            }
            v.update_settings(|s| s.onboarded = true, cx);
        });
        cx.notify();
    }

    /// Check whether the user has done what the current step asks.
    pub(super) fn tour_check(&mut self, cx: &mut Context<Self>) {
        let Some(t) = self.tour.as_ref() else { return };
        if t.advancing {
            return;
        }
        let (step, since) = (t.step, t.since);
        let (session, state, heard, practice) = {
            let v = self.voxa.read(cx);
            let heard = v.levels.iter().rev().take(25).filter(|l| **l > 0.3).count();
            let practice = v.history.entries.iter().rev().any(|e| e.id > since && matches!(e.status, Status::Pasted | Status::Ready) && !e.text.is_empty());
            (v.readiness().session, v.live_state(), heard, practice)
        };
        match step {
            Step::SignIn if session == Session::Valid => self.tour_done_soon(900, cx),
            Step::Mic if heard >= 8 => self.tour_done_soon(900, cx),
            Step::Shortcut if state == "recording" => self.tour_to(Step::Practice, cx),
            Step::Practice if practice => self.tour_done_soon(1600, cx),
            Step::History if self.page == Page::History => self.tour_to(Step::Detail, cx),
            Step::Detail if matches!(self.page, Page::Detail(_)) => self.tour_done_soon(1200, cx),
            Step::Words if self.page == Page::Words => self.tour_done_soon(900, cx),
            _ => {}
        }
    }

    fn tour_target(&self) -> Option<&'static str> {
        match self.tour_step()? {
            Step::Welcome | Step::Done => None,
            Step::SignIn | Step::Mic => Some("ready"),
            Step::Shortcut => Some("keys"),
            Step::Practice => Some("practice"),
            Step::History => Some("nav-history"),
            Step::Detail => Some("history-first"),
            Step::Words => Some("nav-words"),
            Step::Settings => Some(if self.page == Page::Settings { "settings-shortcut" } else { "nav-settings" }),
        }
    }

    /// The dimmed overlay with a cut-out around the target, and the callout card.
    pub(super) fn tour_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.tour.as_ref()?;
        let (step, advancing, waited) = (t.step, t.advancing, t.step_started.elapsed().as_secs());
        let vp = window.viewport_size();
        let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
        let target = self.tour_target().and_then(|k| self.marks.borrow().get(k).copied());

        let shortcut = self.voxa.read(cx).settings.hotkey.replace('+', " + ");
        let hold = self.voxa.read(cx).settings.hold_to_talk;
        let state = self.voxa.read(cx).live_state();
        let n = ORDER.iter().position(|s| *s == step).unwrap_or(0);

        let (title, body): (String, String) = match step {
            Step::Welcome => ("Welcome to Voxa".into(), "Let's get you dictating in about a minute. You'll do each step for real, right here.".into()),
            Step::SignIn => ("Sign in to ChatGPT".into(), "Voxa writes through your own ChatGPT account. A ChatGPT window opens; Voxa never sees your password.".into()),
            Step::Mic => ("Check your microphone".into(), if advancing { "Sounds right.".into() } else { "Say something. The line should move.".into() }),
            Step::Shortcut => ("This is your shortcut".into(), if hold { format!("Hold {shortcut} and talk. Press it now.") } else { format!("Press {shortcut} now.") }),
            Step::Practice => (
                "Say a sentence".into(),
                if advancing {
                    "That's it. It works the same in every app: click where you want to type, press the shortcut, talk.".into()
                } else {
                    match state.as_str() {
                        "recording" if hold => "Listening. Let go when you're done.".into(),
                        "recording" => format!("Listening. Press {shortcut} again when you're done."),
                        "writing" => "Writing it out...".into(),
                        "needs" => "Voxa needs you: look at the strip at the bottom of the screen.".into(),
                        _ => format!("Press {shortcut}, say a sentence, then press it again. Your words land in the box."),
                    }
                },
            ),
            Step::History => ("Everything you say is kept".into(), "Even if it didn't paste. Click History.".into()),
            Step::Detail => ("Open a dictation".into(), if advancing { "Here you can edit it, play it back, or send it again.".into() } else { "Click its text to see it in full.".into() }),
            Step::Words => ("Teach Voxa your words".into(), if advancing { "Fix names ChatGPT gets wrong, and add snippets you say often.".into() } else { "Click Words.".into() }),
            Step::Settings => {
                if self.page == Page::Settings {
                    ("Your shortcut".into(), "Change it here. Prefer holding the key while you talk? Turn on Hold to talk just below.".into())
                } else {
                    ("Make it yours".into(), "Click Settings.".into())
                }
            }
            Step::Done => ("You're set".into(), "Voxa lives in the tray, next to the clock. Press your shortcut in any app to dictate.".into()),
        };

        // Extra content per step.
        let mut extra: Vec<AnyElement> = Vec::new();
        match step {
            Step::SignIn => {
                let signing = self.voxa.read(cx).signing_in;
                extra.push(if signing {
                    div().text_size(px(13.)).text_color(rgb(MUTED)).child("Finish in the ChatGPT window...").into_any_element()
                } else {
                    wbtn("tour-signin", "Sign in to ChatGPT", true).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.voxa.update(cx, |v, cx| v.sign_in(cx)))).into_any_element()
                });
            }
            Step::Mic => {
                let levels: Vec<f32> = self.voxa.read(cx).levels.iter().copied().collect();
                let tick = levels.len().saturating_sub(1);
                extra.push(div().h(px(40.)).flex().items_center().child(crate::ink_line_faded(levels, tick, false)).into_any_element());
                extra.push(div().text_size(px(12.)).text_color(rgb(MUTED)).child(self.voxa.read(cx).mic_name.clone()).into_any_element());
                if waited >= 6 && !advancing {
                    extra.push(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(13.))
                            .child("Can't hear you?")
                            .child(wbtn("tour-mic", "Choose a microphone", false).h(px(28.)).text_size(px(13.)).on_click(|_, _, _| crate::os::open_sound_settings()))
                            .into_any_element(),
                    );
                }
            }
            Step::Shortcut if waited >= 8 => {
                extra.push(div().text_size(px(12.)).text_color(rgb(MUTED)).child("Nothing yet? Another app may use this shortcut. You can change it in Settings later.").into_any_element());
            }
            Step::Done => {
                let on = self.tour.as_ref().map(|t| t.autostart).unwrap_or(true);
                extra.push(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(div().flex_1().text_size(px(14.)).child(crate::os::START_AT_LOGIN_LONG))
                        .child(toggle("tour-autostart", on).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if let Some(t) = this.tour.as_mut() {
                                t.autostart = !on;
                            }
                            cx.notify();
                        })))
                        .into_any_element(),
                );
            }
            _ => {}
        }

        // Which steps can be moved on by hand.
        let next_label = match step {
            Step::Welcome => Some("Start"),
            Step::Mic => Some(if advancing { "Next" } else { "Skip" }),
            Step::Shortcut | Step::Practice => Some("Skip"),
            Step::Settings if self.page == Page::Settings => Some("Next"),
            Step::Done => Some("Finish"),
            _ => None,
        };
        let progress = div().flex().gap(px(3.)).children((0..ORDER.len()).map(|i| div().flex_1().h(px(3.)).rounded(px(2.)).bg(rgb(if i <= n { INK } else { 0xE7E1DA }))));
        let footer = div()
            .mt(px(14.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().flex_1().text_size(px(12.)).text_color(rgb(MUTED)).child(format!("{} of {}", n + 1, ORDER.len())))
            .when(step != Step::Done, |d| {
                d.child(div().id("tour-skip").text_size(px(13.)).text_color(rgb(MUTED)).cursor_pointer().hover(|s| s.text_color(rgb(INK))).child("Skip tour").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.tour_finish(true, cx))))
            })
            .children(next_label.map(|l| {
                wbtn("tour-next", l, l != "Skip").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if this.tour_step() == Some(Step::Done) { this.tour_finish(false, cx) } else { this.tour_next(cx) }
                }))
            }));
        let waiting = !advancing && next_label.is_none() || matches!(step, Step::Shortcut | Step::Practice) && !advancing;
        let card = div()
            .w(px(360.))
            .p(px(18.))
            .rounded(px(10.))
            .bg(rgb(0xFFFFFF))
            .border_1()
            .border_color(rgba(0x0000001A))
            .shadow_lg()
            .font_family("Segoe UI")
            .child(progress)
            .child(div().mt(px(14.)).flex().items_center().gap(px(8.)).when(advancing, |d| d.child(icon(I_CHECK, 14.).text_color(rgb(0x0F7B0F)))).child(div().text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child(title)))
            .child(div().mt(px(6.)).text_size(px(14.)).line_height(px(20.)).text_color(rgb(0x3D3D3D)).child(body))
            .when(!extra.is_empty(), |d| d.child(div().mt(px(12.)).flex().flex_col().gap(px(8.)).children(extra)))
            .when(waiting, |d| d.child(div().mt(px(10.)).flex().items_center().gap(px(6.)).text_size(px(12.)).text_color(rgb(INK)).child(icon('\u{E916}', 11.)).child("Waiting for you")))
            .child(footer);

        // Dim everything but the target; the target stays clickable.
        let dim = rgba(0x1A12_0C8C);
        let block = |x: f32, y: f32, w: f32, h: f32| div().absolute().left(px(x)).top(px(y)).w(px(w.max(0.))).h(px(h.max(0.))).bg(dim).occlude();
        let mut layer = div().absolute().top_0().left_0().w(px(vw)).h(px(vh));
        let (card_x, card_y);
        match target {
            Some(b) => {
                let pad = 6.0;
                let (x, y) = (f32::from(b.origin.x) - pad, f32::from(b.origin.y) - pad);
                let (w, h) = (f32::from(b.size.width) + pad * 2.0, f32::from(b.size.height) + pad * 2.0);
                layer = layer
                    .child(block(0., 0., vw, y))
                    .child(block(0., y + h, vw, vh - y - h))
                    .child(block(0., y, x, h))
                    .child(block(x + w, y, vw - x - w, h))
                    .child(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)).rounded(px(8.)).border_2().border_color(rgb(INK)));
                // To the right if it fits (the menu), else below, else above; never over the target.
                let (cw, ch) = (360., 290.);
                if x + w + 16. + cw <= vw - 16. && w < 400. {
                    card_x = x + w + 16.;
                    card_y = y.clamp(16., (vh - ch).max(16.));
                } else {
                    // Line up with the target's right edge when it is on the right, else its left.
                    card_x = if x + w > vw * 0.6 { (x + w - cw).clamp(16., (vw - cw - 16.).max(16.)) } else { x.clamp(16., (vw - cw - 16.).max(16.)) };
                    card_y = if y + h + 14. + ch < vh { y + h + 14. } else { (y - ch - 14.).max(16.) };
                }
            }
            None => {
                layer = layer.child(block(0., 0., vw, vh));
                card_x = (vw - 360.) / 2.;
                card_y = (vh - 280.) / 2.;
            }
        }
        Some(layer.child(div().absolute().left(px(card_x)).top(px(card_y)).occlude().child(card)).into_any_element())
    }
}
