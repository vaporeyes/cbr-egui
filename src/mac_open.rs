// ABOUTME: Receives file-open requests from macOS (Finder double-click, `open -a`) and CLI args.
// ABOUTME: Queues the paths so the egui update loop can import them and open the reader.

use std::path::PathBuf;
use std::sync::Mutex;

use eframe::egui;

static PENDING_OPENS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static REPAINT_CONTEXT: Mutex<Option<egui::Context>> = Mutex::new(None);

/// Queues existing files passed on the command line so terminal launches like
/// `cbr-egui comic.cbz` open the file once the UI is up.
pub fn queue_paths_from_args() {
    let paths: Vec<PathBuf> = std::env::args()
        .skip(1)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .collect();
    push_paths(paths);
}

/// Drains queued open requests. Called by the UI once per frame.
pub fn take_pending() -> Vec<PathBuf> {
    std::mem::take(&mut *PENDING_OPENS.lock().expect("pending opens lock"))
}

/// Stores the egui context so open events arriving while the app idles can
/// wake the event loop and get processed on the next frame.
pub fn set_repaint_context(ctx: egui::Context) {
    *REPAINT_CONTEXT.lock().expect("repaint context lock") = Some(ctx);
}

fn push_paths(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    PENDING_OPENS
        .lock()
        .expect("pending opens lock")
        .extend(paths);
    if let Some(ctx) = REPAINT_CONTEXT
        .lock()
        .expect("repaint context lock")
        .as_ref()
    {
        ctx.request_repaint();
    }
}

/// Observes NSApplicationWillFinishLaunchingNotification and adds the
/// open-files delegate method when it fires. AppKit delivers a launch
/// document between willFinishLaunching and didFinishLaunching, so the
/// delegate method must exist by then; eframe's app creator closure (which
/// runs inside didFinishLaunching) is too late for that first event. Must
/// run in `main` before the eframe event loop starts.
#[cfg(target_os = "macos")]
pub fn install_launch_hook() {
    platform::install_launch_hook();
}

#[cfg(not(target_os = "macos"))]
pub fn install_launch_hook() {}

/// Adds an `application:openURLs:` implementation to winit's application
/// delegate so AppKit routes open-documents Apple Events to this app. winit
/// 0.30 does not implement the method itself. Safe to call repeatedly; the
/// eframe creator closure calls it as a fallback to the launch hook.
#[cfg(target_os = "macos")]
pub fn install_open_handler() {
    platform::install_delegate_method();
}

#[cfg(not(target_os = "macos"))]
pub fn install_open_handler() {}

#[cfg(target_os = "macos")]
mod platform {
    use std::ffi::{CStr, c_void};
    use std::os::raw::c_char;

    #[link(name = "objc", kind = "dylib")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
        fn object_getClass(obj: *mut c_void) -> *mut c_void;
        fn objc_allocateClassPair(
            superclass: *mut c_void,
            name: *const c_char,
            extra_bytes: usize,
        ) -> *mut c_void;
        fn objc_registerClassPair(cls: *mut c_void);
        fn class_addMethod(
            cls: *mut c_void,
            name: *mut c_void,
            imp: *mut c_void,
            types: *const c_char,
        ) -> bool;
        fn objc_msgSend();
    }

    // objc_msgSend is variadic in C; transmute a single declaration to each call shape.
    type MsgSendId = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
    type MsgSendUsize = unsafe extern "C" fn(*mut c_void, *mut c_void) -> usize;
    type MsgSendIdAtIndex = unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void;
    type MsgSendIdFromCStr =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void;
    type MsgSendCStr = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *const c_char;
    type MsgSendAddObserver = unsafe extern "C" fn(
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *mut c_void,
    );

    /// AppKit invokes this for open-documents Apple Events once the method is
    /// added to the application delegate. `urls` is an NSArray of NSURL.
    unsafe extern "C" fn handle_open_urls(
        _this: *mut c_void,
        _cmd: *mut c_void,
        _application: *mut c_void,
        urls: *mut c_void,
    ) {
        if urls.is_null() {
            return;
        }
        let mut paths = Vec::new();
        unsafe {
            let raw = objc_msgSend as *const ();
            let send_id: MsgSendId = std::mem::transmute(raw);
            let send_usize: MsgSendUsize = std::mem::transmute(raw);
            let send_at_index: MsgSendIdAtIndex = std::mem::transmute(raw);
            let send_cstr: MsgSendCStr = std::mem::transmute(raw);

            let count = send_usize(urls, sel_registerName(c"count".as_ptr()));
            for index in 0..count {
                let url = send_at_index(urls, sel_registerName(c"objectAtIndex:".as_ptr()), index);
                if url.is_null() {
                    continue;
                }
                let ns_path = send_id(url, sel_registerName(c"path".as_ptr()));
                if ns_path.is_null() {
                    continue;
                }
                let utf8 = send_cstr(ns_path, sel_registerName(c"UTF8String".as_ptr()));
                if utf8.is_null() {
                    continue;
                }
                let path = CStr::from_ptr(utf8).to_string_lossy().into_owned();
                paths.push(std::path::PathBuf::from(path));
            }
        }
        super::push_paths(paths);
    }

    /// Notification callback fired during NSApplication finishLaunching,
    /// after winit has installed its delegate but before AppKit dispatches
    /// the launch open-documents event.
    unsafe extern "C" fn on_will_finish_launching(
        _this: *mut c_void,
        _cmd: *mut c_void,
        _notification: *mut c_void,
    ) {
        install_delegate_method();
    }

    pub fn install_launch_hook() {
        unsafe {
            let raw = objc_msgSend as *const ();
            let send_id: MsgSendId = std::mem::transmute(raw);
            let send_id_from_cstr: MsgSendIdFromCStr = std::mem::transmute(raw);
            let send_add_observer: MsgSendAddObserver = std::mem::transmute(raw);

            let ns_object = objc_getClass(c"NSObject".as_ptr());
            if ns_object.is_null() {
                return;
            }
            let hook_class = objc_allocateClassPair(ns_object, c"CbrEguiLaunchHook".as_ptr(), 0);
            if hook_class.is_null() {
                return;
            }
            let hook_sel = sel_registerName(c"cbrAppWillFinishLaunching:".as_ptr());
            class_addMethod(
                hook_class,
                hook_sel,
                on_will_finish_launching as *const () as *mut c_void,
                c"v@:@".as_ptr(),
            );
            objc_registerClassPair(hook_class);

            // The observer instance is intentionally leaked; it must outlive
            // the launch sequence and is created exactly once.
            let observer = send_id(
                send_id(hook_class, sel_registerName(c"alloc".as_ptr())),
                sel_registerName(c"init".as_ptr()),
            );
            if observer.is_null() {
                return;
            }

            let ns_string = objc_getClass(c"NSString".as_ptr());
            if ns_string.is_null() {
                return;
            }
            let name = send_id_from_cstr(
                ns_string,
                sel_registerName(c"stringWithUTF8String:".as_ptr()),
                c"NSApplicationWillFinishLaunchingNotification".as_ptr(),
            );
            let center_class = objc_getClass(c"NSNotificationCenter".as_ptr());
            if center_class.is_null() {
                return;
            }
            let center = send_id(center_class, sel_registerName(c"defaultCenter".as_ptr()));
            if center.is_null() {
                return;
            }
            send_add_observer(
                center,
                sel_registerName(c"addObserver:selector:name:object:".as_ptr()),
                observer,
                hook_sel,
                name,
                std::ptr::null_mut(),
            );
        }
    }

    pub fn install_delegate_method() {
        unsafe {
            let raw = objc_msgSend as *const ();
            let send_id: MsgSendId = std::mem::transmute(raw);

            let ns_application = objc_getClass(c"NSApplication".as_ptr());
            if ns_application.is_null() {
                return;
            }
            let shared = send_id(
                ns_application,
                sel_registerName(c"sharedApplication".as_ptr()),
            );
            if shared.is_null() {
                return;
            }
            let delegate = send_id(shared, sel_registerName(c"delegate".as_ptr()));
            if delegate.is_null() {
                return;
            }
            let class = object_getClass(delegate);
            if class.is_null() {
                return;
            }
            class_addMethod(
                class,
                sel_registerName(c"application:openURLs:".as_ptr()),
                handle_open_urls as *const () as *mut c_void,
                c"v@:@@".as_ptr(),
            );
        }
    }
}
