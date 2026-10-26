//! The C interfaces objc2 does not cover: Carbon hot keys, the window list, SkyLight's
//! read-only Space queries (private) and libdispatch.
#![allow(non_upper_case_globals, non_snake_case)]

use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct EventHotKeyID {
    pub signature: u32,
    pub id: u32,
}

#[repr(C)]
pub struct EventTypeSpec {
    pub event_class: u32,
    pub event_kind: u32,
}

pub type EventHandler = extern "C" fn(next: *mut c_void, event: *mut c_void, data: *mut c_void) -> i32;

pub const fn fourcc(code: &[u8; 4]) -> u32 {
    (code[0] as u32) << 24 | (code[1] as u32) << 16 | (code[2] as u32) << 8 | code[3] as u32
}

pub const kEventClassKeyboard: u32 = fourcc(b"keyb");
pub const kEventHotKeyPressed: u32 = 5;
pub const kEventParamDirectObject: u32 = fourcc(b"----");
pub const typeEventHotKeyID: u32 = fourcc(b"hkid");
pub const kVK_F1: u32 = 0x7A;
pub const kVK_F3: u32 = 0x63;
pub const kVK_F4: u32 = 0x76;

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    pub fn GetApplicationEventTarget() -> *mut c_void;
    pub fn InstallEventHandler(target: *mut c_void, handler: EventHandler, count: u32, types: *const EventTypeSpec,
                               data: *mut c_void, out: *mut *mut c_void) -> i32;
    pub fn RegisterEventHotKey(code: u32, modifiers: u32, id: EventHotKeyID, target: *mut c_void, options: u32,
                               out: *mut *mut c_void) -> i32;
    pub fn GetEventParameter(event: *mut c_void, name: u32, desired: u32, actual: *mut u32, size: usize,
                             actual_size: *mut usize, data: *mut c_void) -> i32;
}

pub const kCGWindowListOptionAll: u32 = 0;
pub const kCGWindowListOptionOnScreenOnly: u32 = 1 << 0;
pub const kCGWindowListExcludeDesktopElements: u32 = 1 << 4;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// A CFArray of CFDictionary, owned by the caller.
    pub fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> *mut c_void;
}

#[link(name = "SkyLight", kind = "framework")]
extern "C" {
    pub fn SLSMainConnectionID() -> i32;
    pub fn SLSGetActiveSpace(cid: i32) -> u64;
    /// Takes a CFArray of window numbers; returns a CFArray of space ids, owned by the caller.
    pub fn SLSCopySpacesForWindows(cid: i32, mask: i32, windows: *const c_void) -> *mut c_void;
}

pub type AXUIElementRef = *const c_void;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub static kCFTypeDictionaryKeyCallBacks: c_void;
    pub static kCFTypeDictionaryValueCallBacks: c_void;
    pub static kCFBooleanTrue: *const c_void;
    pub fn CFRelease(cf: *const c_void);
    pub fn CFDictionaryCreate(alloc: *const c_void, keys: *const *const c_void, values: *const *const c_void, count: isize,
                              key_callbacks: *const c_void, value_callbacks: *const c_void) -> *const c_void;
}

// Accessibility. AXIsProcessTrusted and the element calls need no link-time constants: the
// attribute and option names are plain CFStrings, which we pass as toll-free-bridged NSStrings.
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub static kAXTrustedCheckOptionPrompt: *const c_void;
    pub fn AXIsProcessTrusted() -> bool;
    pub fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    pub fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    pub fn AXUIElementSetAttributeValue(element: AXUIElementRef, attribute: *const c_void, value: *const c_void) -> i32;
}

pub type DispatchWork = extern "C" fn(context: *mut c_void);

extern "C" {
    pub static _dispatch_main_q: c_void;
    pub fn dispatch_sync_f(queue: *const c_void, context: *mut c_void, work: DispatchWork);
    pub fn dispatch_after_f(when: u64, queue: *const c_void, context: *mut c_void, work: DispatchWork);
    pub fn dispatch_time(when: u64, delta: i64) -> u64;
}
