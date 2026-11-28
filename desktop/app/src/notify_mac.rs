//! macOS notifications: UNUserNotificationCenter through the existing objc dependency (and the
//! `block` crate cocoa already brings in). Needs the app bundle (desktop/macos/Info.plist,
//! app.voxa.desktop): a bare binary has no bundle and the center would throw, so it is skipped.
//! Interruption levels: sign-in "active", dictations "passive"; never time-sensitive, so Focus
//! and Do Not Disturb are respected.

#![allow(unexpected_cfgs)] // objc 0.2's macros

use std::ffi::{CStr, c_void};
use std::sync::Once;

use block::ConcreteBlock;
use cocoa::base::{id, nil};
use cocoa::foundation::NSString;
use objc::declare::ClassDecl;
use objc::runtime::{BOOL, Object, Sel};
use objc::{class, msg_send, sel, sel_impl};

#[link(name = "UserNotifications", kind = "framework")]
unsafe extern "C" {}

const DEFAULT_ACTION: &str = "com.apple.UNNotificationDefaultActionIdentifier";

fn ns(s: &str) -> id {
    unsafe { NSString::alloc(nil).init_str(s) }
}

fn rust(s: id) -> String {
    if s.is_null() {
        return String::new();
    }
    unsafe {
        let c: *const std::os::raw::c_char = msg_send![s, UTF8String];
        if c.is_null() { String::new() } else { CStr::from_ptr(c).to_string_lossy().into_owned() }
    }
}

/// The notification center, or None outside an app bundle.
fn center() -> Option<id> {
    unsafe {
        let bundle: id = msg_send![class!(NSBundle), mainBundle];
        let ident: id = msg_send![bundle, bundleIdentifier];
        let path: id = msg_send![bundle, bundlePath];
        if ident.is_null() || !rust(path).ends_with(".app") {
            return None;
        }
        let c: id = msg_send![class!(UNUserNotificationCenter), currentNotificationCenter];
        (!c.is_null()).then_some(c)
    }
}

/// userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:
extern "C" fn did_receive(_this: &Object, _sel: Sel, _center: id, response: id, handler: *mut c_void) {
    unsafe {
        let action = rust(msg_send![response, actionIdentifier]);
        let note: id = msg_send![response, notification];
        let request: id = msg_send![note, request];
        let ident = rust(msg_send![request, identifier]);
        let content: id = msg_send![request, content];
        let info: id = msg_send![content, userInfo];
        let launch = rust(msg_send![info, objectForKey: ns("launch")]);
        let button = rust(msg_send![info, objectForKey: ns("button")]);
        let args = if action == DEFAULT_ACTION { launch } else if action == ident { button } else { String::new() };
        if let Some(c) = super::parse(&args) {
            super::clicked(c);
        }
        if !handler.is_null() {
            let b = &*(handler as *const block::Block<(), ()>);
            b.call(());
        }
    }
}

static DELEGATE: Once = Once::new();

pub fn init() -> bool {
    let Some(c) = center() else {
        crate::os::log("notifications: not running from Voxa.app, so none are shown");
        return false;
    };
    DELEGATE.call_once(|| unsafe {
        let Some(mut decl) = ClassDecl::new("VoxaNotifyDelegate", class!(NSObject)) else { return };
        decl.add_method(
            sel!(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:),
            did_receive as extern "C" fn(&Object, Sel, id, id, *mut c_void),
        );
        let cls = decl.register();
        // Kept for the life of the app (the center holds its delegate weakly).
        let obj: id = msg_send![cls, new];
        let _: () = msg_send![c, setDelegate: obj];
        // Every button category, registered once: setNotificationCategories replaces them all,
        // so registering one per toast removed the other toast's button.
        let signin = voxa_core::notices::signed_out(0).action.unwrap_or_else(|| "Sign in".into());
        let copy = voxa_core::notices::ready(1, None).action.unwrap_or_else(|| "Copy".into());
        let cats = [category(super::TAG_SIGNIN, &signin, true), category(super::TAG_DICTATIONS, &copy, false), category(super::TAG_DICTATIONS, "Send now", false)];
        let list: id = msg_send![class!(NSArray), arrayWithObjects: cats.as_ptr() count: cats.len()];
        let set: id = msg_send![class!(NSSet), setWithArray: list];
        let _: () = msg_send![c, setNotificationCategories: set];
    });
    true
}

/// macOS can't report closed notifications without an async query: nothing to forget here.
pub fn take_dismissed() -> Vec<String> {
    Vec::new()
}

/// One category per tag and button label, whose single button carries the tag as its identifier.
/// `foreground`: the button opens Voxa (Sign in). Copy and Send now run in the background and
/// never bring Voxa to the front or take focus.
fn category(tag: &str, label: &str, foreground: bool) -> id {
    unsafe {
        // UNNotificationActionOptionForeground = 1 << 2; none = 0.
        let options: u64 = if foreground { 1 << 2 } else { 0 };
        let action: id = msg_send![class!(UNNotificationAction), actionWithIdentifier: ns(tag) title: ns(label) options: options];
        let actions: id = msg_send![class!(NSArray), arrayWithObject: action];
        let empty: id = msg_send![class!(NSArray), array];
        msg_send![class!(UNNotificationCategory), categoryWithIdentifier: ns(&format!("{tag}:{label}")) actions: actions intentIdentifiers: empty options: 0u64]
    }
}

/// `update`: it replaces one already showing: passive and silent, so it changes the count
/// without a new banner or sound.
pub fn show(tag: &str, title: &str, body: &str, button: Option<(&str, &str)>, launch: &str, active: bool, update: bool) -> bool {
    let active = active && !update;
    let Some(c) = center() else { return false };
    unsafe {
        let content: id = msg_send![class!(UNMutableNotificationContent), new];
        let _: () = msg_send![content, setTitle: ns(title)];
        let _: () = msg_send![content, setBody: ns(body)];
        let info: id = msg_send![class!(NSMutableDictionary), dictionary];
        let _: () = msg_send![info, setObject: ns(launch) forKey: ns("launch")];
        if let Some((label, args)) = button {
            let _: () = msg_send![info, setObject: ns(args) forKey: ns("button")];
            // Registered once in init (see there).
            let _: () = msg_send![content, setCategoryIdentifier: ns(&format!("{tag}:{label}"))];
        }
        let _: () = msg_send![content, setUserInfo: info];
        let has_level: BOOL = msg_send![content, respondsToSelector: sel!(setInterruptionLevel:)];
        if has_level != objc::runtime::NO {
            // UNNotificationInterruptionLevelPassive = 0, Active = 1.
            let _: () = msg_send![content, setInterruptionLevel: if active { 1u64 } else { 0u64 }];
        }
        if active {
            let sound: id = msg_send![class!(UNNotificationSound), defaultSound];
            let _: () = msg_send![content, setSound: sound];
        }
        let request: id = msg_send![class!(UNNotificationRequest), requestWithIdentifier: ns(tag) content: content trigger: nil];
        let _: () = msg_send![c, addNotificationRequest: request withCompletionHandler: nil];
    }
    crate::os::log(&format!("notification shown: {tag}"));
    true
}

pub fn remove(tag: &str) {
    let Some(c) = center() else { return };
    unsafe {
        let ids: id = msg_send![class!(NSArray), arrayWithObject: ns(tag)];
        let _: () = msg_send![c, removeDeliveredNotificationsWithIdentifiers: ids];
    }
}

/// Ask once for alerts and sounds (the system shows its own prompt).
pub fn ask_permission() -> bool {
    let Some(c) = center() else { return false };
    let done = ConcreteBlock::new(|granted: BOOL, _err: id| {
        crate::os::log(&format!("notifications {}", if granted != objc::runtime::NO { "allowed" } else { "not allowed" }));
    })
    .copy();
    unsafe {
        // UNAuthorizationOptionSound | UNAuthorizationOptionAlert
        let _: () = msg_send![c, requestAuthorizationWithOptions: (1u64 << 1) | (1u64 << 2) completionHandler: &*done];
    }
    true
}
