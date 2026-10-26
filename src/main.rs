//! xsp: xmonad's exclusive scratchpads, for Alacritty on macOS.
//!
//!   F1, F3, F4   toggle scratchpad 0, 1, 2 (`alacritty --title scratch-N`)
//!
//! A key spawns its scratchpad when it has no window, hides it when it is shown on the current
//! Space, and otherwise brings it to the current Space and focuses it. Showing one hides the
//! others shown on that Space. `xsp daemon` holds the hot keys; `xsp toggle N` and `xsp status`
//! talk to it through ~/.cache/xsp.sock.

mod sys;

use std::cell::RefCell;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::ptr::{null_mut, NonNull};
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy, NSRunningApplication, NSWorkspace,
    NSWorkspaceApplicationKey, NSWorkspaceDidActivateApplicationNotification, NSWorkspaceOpenConfiguration,
};
use objc2_foundation::{
    ns_string, NSArray, NSDictionary, NSError, NSMutableDictionary, NSNotification, NSNumber, NSOperationQueue, NSString,
    NSURL,
};
use sys::*;

const PADS: usize = 3;
const KEYS: [u32; PADS] = [kVK_F1, kVK_F3, kVK_F4];
const ALACRITTY: &str = "/Applications/Nix Apps/Alacritty.app";

fn log(message: &str) {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        let now = libc::time(null_mut());
        libc::localtime_r(&now, &mut tm);
    }
    eprintln!("{:02}:{:02}:{:02} {message}", tm.tm_hour, tm.tm_min, tm.tm_sec);
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn socket_path() -> String {
    format!("{}/.cache/xsp.sock", home())
}

fn bundle_path(n: usize) -> String {
    format!("{}/Library/Application Support/xsp/scratch-{n}.app", home())
}

fn bundle_id(n: usize) -> String {
    format!("local.xsp.scratch-{n}")
}

// ---------------------------------------------------------------------------------------------
// Running on the main thread, now or a little later

extern "C" fn run_boxed(context: *mut c_void) {
    let f = unsafe { Box::from_raw(context as *mut Box<dyn FnOnce()>) };
    f();
}

fn after_ms(ms: i64, f: impl FnOnce() + 'static) {
    let boxed: Box<Box<dyn FnOnce()>> = Box::new(Box::new(f));
    unsafe {
        dispatch_after_f(dispatch_time(0, ms * 1_000_000), &_dispatch_main_q, Box::into_raw(boxed) as *mut c_void, run_boxed);
    }
}

struct SyncCall<F, R> {
    f: Option<F>,
    result: Option<R>,
}

extern "C" fn run_sync<F: FnOnce() -> R, R>(context: *mut c_void) {
    let call = unsafe { &mut *(context as *mut SyncCall<F, R>) };
    call.result = call.f.take().map(|f| f());
}

fn on_main<F: FnOnce() -> R + Send, R: Send>(f: F) -> R {
    let mut call = SyncCall { f: Some(f), result: None };
    unsafe { dispatch_sync_f(&_dispatch_main_q, &mut call as *mut SyncCall<F, R> as *mut c_void, run_sync::<F, R>) };
    call.result.expect("main queue ran the call")
}

/// Runs `then` once `ready` holds, checking every 20 ms for at most `ms`; it gets whether it did.
fn when(ready: impl Fn() -> bool + 'static, ms: i64, then: impl FnOnce(bool) + 'static) {
    if ready() {
        then(true);
    } else if ms <= 0 {
        then(false);
    } else {
        after_ms(20, move || when(ready, ms - 20, then));
    }
}

// ---------------------------------------------------------------------------------------------
// Windows and Spaces

#[derive(Clone, Copy)]
struct Window {
    id: u32,
    pid: i32,
    layer: i64,
    onscreen: bool,
    width: f64,
    height: f64,
}

fn number(value: Option<Retained<AnyObject>>) -> Option<Retained<NSNumber>> {
    value.and_then(|v| v.downcast::<NSNumber>().ok())
}

/// Every window, front to back. Owner, layer and bounds need no Screen Recording permission.
fn window_list(options: u32) -> Vec<Window> {
    let raw = unsafe { CGWindowListCopyWindowInfo(options | kCGWindowListExcludeDesktopElements, 0) };
    let Some(list) = (unsafe { Retained::from_raw(raw as *mut NSArray<NSDictionary<NSString, AnyObject>>) }) else {
        return Vec::new();
    };
    list.to_vec()
        .iter()
        .map(|w| {
            let field = |key: &NSString| number(w.objectForKey(key));
            let bounds = w.objectForKey(ns_string!("kCGWindowBounds")).and_then(|b| b.downcast::<NSDictionary>().ok());
            let size = |key: &NSString| number(bounds.as_ref().and_then(|b| b.objectForKey(key))).map_or(0.0, |n| n.doubleValue());
            Window {
                id: field(ns_string!("kCGWindowNumber")).map_or(0, |n| n.unsignedIntValue()),
                pid: field(ns_string!("kCGWindowOwnerPID")).map_or(0, |n| n.intValue()),
                layer: field(ns_string!("kCGWindowLayer")).map_or(0, |n| n.longLongValue()),
                onscreen: field(ns_string!("kCGWindowIsOnscreen")).is_some_and(|n| n.boolValue()),
                width: size(ns_string!("Width")),
                height: size(ns_string!("Height")),
            }
        })
        .collect()
}

fn active_space() -> u64 {
    unsafe { SLSGetActiveSpace(SLSMainConnectionID()) }
}

fn space_of(wid: u32) -> u64 {
    let windows = NSArray::from_retained_slice(&[NSNumber::new_u32(wid)]);
    let raw = unsafe { SLSCopySpacesForWindows(SLSMainConnectionID(), 0x7, Retained::as_ptr(&windows) as *const c_void) };
    let spaces = unsafe { Retained::from_raw(raw as *mut NSArray<NSNumber>) };
    spaces.and_then(|s| s.firstObject()).map_or(0, |n| n.unsignedLongLongValue())
}

// ---------------------------------------------------------------------------------------------
// ZeroSpace, the user's desktop switcher, moves windows between Spaces for us: SkyLight's
// bridged window moves need the Accessibility grant it holds, so xsp itself needs none.

fn zerospace(command: &str) -> Result<String, String> {
    let fail = |e: std::io::Error| format!("ZeroSpace: {e}");
    let mut stream = UnixStream::connect(format!("{}/.cache/zerospace.sock", home())).map_err(fail)?;
    stream.set_read_timeout(Some(Duration::from_secs(1))).map_err(fail)?;
    stream.set_write_timeout(Some(Duration::from_secs(1))).map_err(fail)?;
    stream.write_all(command.as_bytes()).map_err(fail)?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).map_err(fail)?;
    Ok(reply.trim().to_string())
}

/// The current desktop's number among the ordinary desktops, which is what `move` takes.
fn current_desktop() -> Result<usize, String> {
    let state = zerospace("state")?;
    let after = |key: &str| state.split(key).nth(1).unwrap_or_default().to_string();
    let desktops: Vec<u64> =
        after("desktops [").split(']').next().unwrap_or_default().split(',').filter_map(|s| s.trim().parse().ok()).collect();
    let current: u64 = after("current ").split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
    match desktops.iter().position(|&s| s == current) {
        Some(i) => Ok(i + 1),
        None => Err(format!("ZeroSpace cannot move windows to Space {current} ({state})")),
    }
}

// ---------------------------------------------------------------------------------------------
// The scratchpad apps: Alacritty under its own bundle id each, so LaunchServices can hide, show
// and activate exactly that one. They stay regular apps because macOS refuses to hide an
// LSUIElement app for anyone but the app itself.

/// Copies `from` over `to` when they differ, through a rename so a running copy keeps its file.
fn refresh(from: &str, to: &str) -> std::io::Result<()> {
    let bytes = std::fs::read(from)?;
    if std::fs::read(to).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    let temp = format!("{to}.new");
    std::fs::write(&temp, &bytes)?;
    std::fs::set_permissions(&temp, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
    std::fs::rename(&temp, to)
}

/// Builds or updates a wrapper bundle. Alacritty's binary must be a copy, refreshed when nix
/// replaces it: LaunchServices resolves a symlink, and the process then checks in as Alacritty.
#[allow(deprecated)]
fn prepare_bundle(n: usize) -> Result<(), String> {
    let contents = format!("{}/Contents", bundle_path(n));
    let fail = |e: std::io::Error| format!("scratch-{n} bundle: {e}");
    std::fs::create_dir_all(format!("{contents}/MacOS")).map_err(fail)?;
    std::fs::create_dir_all(format!("{contents}/Resources")).map_err(fail)?;
    refresh(&format!("{ALACRITTY}/Contents/MacOS/alacritty"), &format!("{contents}/MacOS/alacritty")).map_err(fail)?;
    refresh(&format!("{ALACRITTY}/Contents/Resources/alacritty.icns"), &format!("{contents}/Resources/alacritty.icns"))
        .map_err(fail)?;

    let source = NSString::from_str(&format!("{ALACRITTY}/Contents/Info.plist"));
    let Some(info) = (unsafe { NSMutableDictionary::<NSString, AnyObject>::dictionaryWithContentsOfFile(&source) }) else {
        return Err(format!("cannot read {source}"));
    };
    let name = NSString::from_str(&format!("scratch-{n}"));
    info.insert(ns_string!("CFBundleIdentifier"), &NSString::from_str(&bundle_id(n)));
    info.insert(ns_string!("CFBundleName"), &name);
    info.insert(ns_string!("CFBundleDisplayName"), &name);
    let target = NSString::from_str(&format!("{contents}/Info.plist"));
    let current = unsafe { NSDictionary::<NSString, AnyObject>::dictionaryWithContentsOfFile(&target) };
    if !current.is_some_and(|c| c.isEqualToDictionary(&info)) && !unsafe { info.writeToFile_atomically(&target, true) } {
        return Err(format!("cannot write {target}"));
    }
    Ok(())
}

struct Pad {
    n: usize,
    app: Retained<NSRunningApplication>,
    pid: i32,
    hidden: bool,
    /// Its windows, the terminal (the largest) first.
    windows: Vec<Window>,
}

impl Pad {
    fn shown_here(&self) -> bool {
        !self.hidden && self.windows.first().is_some_and(|w| w.onscreen)
    }
}

fn launched(app: &NSRunningApplication) -> f64 {
    app.launchDate().map_or(f64::MAX, |d| d.timeIntervalSince1970())
}

fn pads() -> Vec<Option<Pad>> {
    let all = window_list(kCGWindowListOptionAll);
    (0..PADS)
        .map(|n| {
            // The first one launched: Alacritty's SpawnNewInstance starts more under the same bundle
            let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(&bundle_id(n)));
            let app = apps
                .to_vec()
                .into_iter()
                .filter(|a| !a.isTerminated())
                .min_by(|a, b| launched(a).total_cmp(&launched(b)))?;
            let pid = app.processIdentifier();
            let mut windows: Vec<Window> =
                all.iter().filter(|w| w.pid == pid && w.layer == 0 && w.width >= 100.0 && w.height >= 100.0).copied().collect();
            windows.sort_by(|a, b| (b.width * b.height).total_cmp(&(a.width * a.height)));
            Some(Pad { n, hidden: app.isHidden(), app, pid, windows })
        })
        .collect()
}

/// Opens the scratchpad through LaunchServices: a launch when it is not running; otherwise a
/// reopen, which unhides it, brings back a minimised window, and activates it.
fn open(n: usize, arguments: &[&str]) {
    let config = NSWorkspaceOpenConfiguration::configuration();
    config.setArguments(&NSArray::from_retained_slice(&arguments.iter().map(|a| NSString::from_str(a)).collect::<Vec<_>>()));
    config.setActivates(true);
    config.setAddsToRecentItems(false);
    config.setPromptsUserIfNeeded(false);
    let url = NSURL::fileURLWithPath(&NSString::from_str(&bundle_path(n)));
    let done = RcBlock::new(move |_app: *mut NSRunningApplication, error: *mut NSError| {
        if let Some(error) = unsafe { error.as_ref() } {
            log(&format!("could not open scratch-{n}: {}", error.localizedDescription()));
        }
    });
    NSWorkspace::sharedWorkspace().openApplicationAtURL_configuration_completionHandler(&url, &config, Some(&done));
}

fn spawn(n: usize) -> Result<(), String> {
    prepare_bundle(n)?;
    open(n, &["--title", &format!("scratch-{n}")]);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Focus

thread_local! {
    /// Apps by when they were last active, most recent first; scratchpads and xsp left out.
    static RECENT: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
}

fn front_pid() -> i32 {
    NSWorkspace::sharedWorkspace().frontmostApplication().map_or(0, |a| a.processIdentifier())
}

fn note_activation(app: &NSRunningApplication) {
    let pid = app.processIdentifier();
    if pid == std::process::id() as i32 || app.bundleIdentifier().is_some_and(|id| id.to_string().starts_with("local.xsp.")) {
        return;
    }
    RECENT.with_borrow_mut(|recent| {
        recent.retain(|&p| p != pid);
        recent.insert(0, pid);
        recent.truncate(32);
    });
}

/// Where the focus goes when the focused scratchpad hides: the most recently active app with a
/// window on this Space, else the app of the topmost window here, else Finder, whose desktop is
/// on every Space. Never an app with no window here: activating one makes macOS switch Spaces.
fn focus_target(skip: &[i32]) -> Option<Retained<NSRunningApplication>> {
    let here: Vec<i32> = window_list(kCGWindowListOptionOnScreenOnly)
        .iter()
        .filter(|w| w.layer == 0 && w.width >= 100.0 && w.height >= 100.0 && !skip.contains(&w.pid))
        .map(|w| w.pid)
        .collect();
    let recent = RECENT.with_borrow(|r| r.iter().copied().find(|p| here.contains(p)));
    recent
        .or(here.first().copied())
        .and_then(NSRunningApplication::runningApplicationWithProcessIdentifier)
        .or_else(|| NSRunningApplication::runningApplicationsWithBundleIdentifier(ns_string!("com.apple.finder")).firstObject())
}

fn name(app: &NSRunningApplication) -> String {
    app.localizedName().map(|n| n.to_string()).unwrap_or_default()
}

/// Hides scratchpads. One that has the focus hands it to `focus_target` first, so macOS never
/// picks the next app itself and pulls the user to another Space.
fn hide(pads: Vec<Pad>, why: &str) {
    if pads.is_empty() {
        return;
    }
    let pids: Vec<i32> = pads.iter().map(|p| p.pid).collect();
    let names: Vec<String> = pads.iter().map(|p| format!("scratch-{}", p.n)).collect();
    log(&format!("hide {} ({why})", names.join(", ")));
    if pids.contains(&front_pid()) {
        if let Some(target) = focus_target(&pids) {
            target.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows);
            log(&format!("focus to {} (pid {})", name(&target), target.processIdentifier()));
        }
    }
    when(move || !pids.contains(&front_pid()), 300, move |moved| {
        if !moved {
            log("the focus did not leave the scratchpad; hiding anyway");
        }
        for pad in &pads {
            pad.app.hide();
        }
    });
}

// ---------------------------------------------------------------------------------------------
// The action, after XMonad.Util.ExclusiveScratchpads.scratchpadAction

fn toggle(n: usize) -> String {
    let mut all = pads();
    let pad = all[n].take();
    let others: Vec<Pad> = all.into_iter().flatten().filter(Pad::shown_here).collect();
    let Some(pad) = pad else {
        if let Err(e) = spawn(n) {
            log(&e);
            return e;
        }
        // Alacritty activates itself once its window is up; the others hide after that
        let bundle = bundle_id(n);
        let is_front = move || {
            let front = NSWorkspace::sharedWorkspace().frontmostApplication();
            front.and_then(|a| a.bundleIdentifier()).is_some_and(|id| id.to_string() == bundle)
        };
        when(is_front, 3000, move |_| hide(others, "spawned another"));
        return format!("spawned scratch-{n}");
    };
    if pad.windows.is_empty() {
        return format!("scratch-{n} is still starting");
    }
    if pad.shown_here() {
        let mut list = vec![pad];
        list.extend(others);
        hide(list, "toggled off");
        return format!("hid scratch-{n}");
    }
    show_here(pad, others)
}

fn show_here(pad: Pad, others: Vec<Pad>) -> String {
    let n = pad.n;
    let here = active_space();
    let away: Vec<u32> = pad.windows.iter().map(|w| w.id).filter(|&w| ![0, here].contains(&space_of(w))).collect();
    let mut report = String::from("already on this Space");
    if !away.is_empty() {
        report = match current_desktop() {
            Ok(desktop) => away
                .iter()
                .map(|w| zerospace(&format!("move {w} {desktop}")).unwrap_or_else(|e| e))
                .collect::<Vec<_>>()
                .join("; "),
            Err(e) => format!("{e}; it stays on its own Space, and macOS switches there"),
        };
    }
    // ZeroSpace's moves land asynchronously; activating before they do would switch Spaces
    let pid = pad.pid;
    after_ms(if away.is_empty() { 0 } else { 50 }, move || {
        open(n, &[]);
        when(move || front_pid() == pid, 1000, move |_| hide(others, "showed another"));
    });
    format!("showed scratch-{n} ({report})")
}

fn status() -> String {
    let front = NSWorkspace::sharedWorkspace().frontmostApplication();
    let mut lines = vec![format!(
        "space {} (desktop {}), focus on {} (pid {})",
        active_space(),
        current_desktop().map_or_else(|e| format!("? {e}"), |d| d.to_string()),
        front.as_deref().map(name).unwrap_or_default(),
        front.as_ref().map_or(0, |a| a.processIdentifier()),
    )];
    for (n, pad) in pads().iter().enumerate() {
        lines.push(match pad {
            None => format!("scratch-{n}: not running"),
            Some(p) => {
                let windows: Vec<String> = p
                    .windows
                    .iter()
                    .map(|w| format!("window {} on space {}, {}", w.id, space_of(w.id), if w.onscreen { "on screen" } else { "off screen" }))
                    .collect();
                let state = if p.shown_here() { "shown here" } else if p.hidden { "hidden" } else { "shown elsewhere" };
                format!("scratch-{n}: pid {}, {state}, {}", p.pid, if windows.is_empty() { "no window".into() } else { windows.join("; ") })
            }
        });
    }
    lines.join("\n")
}

fn command(line: &str) -> String {
    let words: Vec<&str> = line.split_whitespace().collect();
    match words.as_slice() {
        ["toggle", n] => match n.parse::<usize>() {
            Ok(n) if n < PADS => {
                let result = toggle(n);
                log(&format!("xsp toggle {n}: {result}"));
                result
            }
            _ => format!("no scratchpad {n}; they are 0 to {}", PADS - 1),
        },
        ["status"] => status(),
        _ => "usage: xsp toggle 0|1|2 | xsp status".into(),
    }
}

// ---------------------------------------------------------------------------------------------
// Hot keys, notifications and the socket

extern "C" fn hot_key_pressed(_next: *mut c_void, event: *mut c_void, _data: *mut c_void) -> i32 {
    let mut key = EventHotKeyID::default();
    unsafe {
        GetEventParameter(event, kEventParamDirectObject, typeEventHotKeyID, null_mut(), std::mem::size_of::<EventHotKeyID>(),
                          null_mut(), &mut key as *mut EventHotKeyID as *mut c_void);
    }
    let n = key.id as usize;
    if n < PADS {
        let result = toggle(n);
        log(&format!("key for scratch-{n}: {result}"));
    }
    0
}

fn register_hot_keys() {
    let signature = fourcc(b"xsp!");
    unsafe {
        let target = GetApplicationEventTarget();
        let spec = EventTypeSpec { event_class: kEventClassKeyboard, event_kind: kEventHotKeyPressed };
        InstallEventHandler(target, hot_key_pressed, 1, &spec, null_mut(), null_mut());
        for (n, code) in KEYS.into_iter().enumerate() {
            let mut reference = null_mut();
            let status = RegisterEventHotKey(code, 0, EventHotKeyID { signature, id: n as u32 }, target, 0, &mut reference);
            if status != 0 {
                log(&format!("could not register the key for scratch-{n}: {status}"));
            }
        }
    }
}

fn observe_activations() {
    let block = RcBlock::new(|note: NonNull<NSNotification>| {
        let app = unsafe { note.as_ref() }
            .userInfo()
            .and_then(|info| info.objectForKey(unsafe { NSWorkspaceApplicationKey }))
            .and_then(|a| a.downcast::<NSRunningApplication>().ok());
        if let Some(app) = app {
            note_activation(&app);
        }
    });
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
            Some(&NSOperationQueue::mainQueue()),
            &block,
        )
    };
    std::mem::forget(observer);
}

fn serve(mut stream: UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buffer = [0u8; 256];
    let n = stream.read(&mut buffer).unwrap_or(0);
    let line = String::from_utf8_lossy(&buffer[..n]).into_owned();
    let reply = on_main(move || command(&line));
    let _ = stream.write_all(format!("{reply}\n").as_bytes());
}

fn listen() {
    let path = socket_path();
    let _ = std::fs::create_dir_all(format!("{}/.cache", home()));
    let _ = std::fs::remove_file(&path);
    match UnixListener::bind(&path) {
        Ok(listener) => {
            std::thread::spawn(move || listener.incoming().flatten().for_each(serve));
        }
        Err(e) => log(&format!("socket {path}: {e}")),
    }
}

fn daemon() {
    let mtm = MainThreadMarker::new().expect("main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    for n in 0..PADS {
        if let Err(e) = prepare_bundle(n) {
            log(&e);
        }
    }
    if let Some(front) = NSWorkspace::sharedWorkspace().frontmostApplication() {
        note_activation(&front);
    }
    observe_activations();
    register_hot_keys();
    listen();
    log(&format!("xsp ready\n{}", status()));
    app.run();
}

fn client(line: &str) -> i32 {
    let mut stream = match UnixStream::connect(socket_path()) {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("xsp: the daemon is not running ({}: {e})", socket_path());
            return 1;
        }
    };
    let mut reply = String::new();
    if stream.write_all(line.as_bytes()).is_err() || stream.read_to_string(&mut reply).is_err() {
        eprintln!("xsp: no reply from the daemon");
        return 1;
    }
    print!("{reply}");
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let code = match words.as_slice() {
        ["daemon"] => {
            daemon();
            0
        }
        ["toggle", _] | ["status"] => client(&words.join(" ")),
        _ => {
            eprintln!("usage: xsp daemon | xsp toggle 0|1|2 | xsp status");
            2
        }
    };
    std::process::exit(code);
}
