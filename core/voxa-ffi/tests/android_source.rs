//! Structural regression checks on the Android keyboard's Kotlin, for lifecycle bugs that an
//! instrumented test can't reproduce deterministically (Android tearing the service down).
//! They read the source the APK is built from; the behaviour itself was checked on the phone.

const KEYBOARD: &str = include_str!("../../../android/app/src/main/java/com/voxa/android/ime/VoxaKeyboard.kt");

fn body_of<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("{signature} not found"));
    let open = start + src[start..].find('{').expect("body");
    let mut depth = 0usize;
    for (i, ch) in src[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces in {signature}")
}

fn pos(body: &str, what: &str) -> usize {
    body.find(what).unwrap_or_else(|| panic!("{what:?} missing"))
}

/// The keyboard crashed every time Android closed it: the engine was torn down before
/// super.onDestroy() asked it whether it was recording. The flow is destroyed last, and every
/// late callback is dropped first.
#[test]
fn closing_the_keyboard_destroys_the_flow_only_after_the_system_is_done_with_it() {
    let b = body_of(KEYBOARD, "override fun onDestroy()");
    let flag = pos(b, "destroyed = true");
    let callbacks = pos(b, "main.removeCallbacksAndMessages(null)");
    let sup = pos(b, "super.onDestroy()");
    let flow = pos(b, "flow.destroy()");
    assert!(flag < callbacks && callbacks < sup && sup < flow, "order: destroyed flag, drop callbacks, super.onDestroy(), then flow.destroy()");
}

/// Late work after the keyboard closed (a stop, a level, a timer) must do nothing.
#[test]
fn work_arriving_after_the_keyboard_closed_does_nothing() {
    let stop = body_of(KEYBOARD, "private fun stopIfRecording(");
    assert!(stop.trim_start_matches('{').trim_start().starts_with("if (destroyed) return"));
    let taken = body_of(KEYBOARD, "private fun onMicInterrupted()");
    assert!(taken.contains("if (destroyed ||"));
    for (i, _) in KEYBOARD.match_indices("main.post { ") {
        let lambda = &KEYBOARD[i..i + 60.min(KEYBOARD.len() - i)];
        assert!(lambda.contains("!destroyed"), "a posted callback checks the keyboard is still there: {lambda}");
    }
}

/// The mic key's tap and hold rules live in KeyRules (tested on the emulator in RegressionsTest);
/// the keyboard must use them, not its own copy.
#[test]
fn the_mic_key_uses_the_tested_rules() {
    assert!(KEYBOARD.contains("KeyRules.stopOnRelease("));
    assert!(KEYBOARD.contains("KeyRules.stopOnCancel("));
    assert!(KEYBOARD.contains("KeyRules.inCall("));
    assert!(KEYBOARD.contains("KeyRules.fieldIdentity("));
}
