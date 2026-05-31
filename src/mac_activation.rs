// ABOUTME: macOS AppKit activation helper for launches started from `cargo run`.
// ABOUTME: Promotes the process to a foreground app so the window receives keyboard input.

#[cfg(target_os = "macos")]
pub fn activate() {
    use std::ffi::c_void;
    use std::os::raw::{c_char, c_int};

    #[link(name = "objc", kind = "dylib")]
    unsafe extern "C" {
        fn objc_getClass(name: *const c_char) -> *mut c_void;
        fn sel_registerName(name: *const c_char) -> *mut c_void;
    }

    // objc_msgSend is variadic in C; transmute a single declaration to each call shape.
    #[link(name = "objc", kind = "dylib")]
    unsafe extern "C" {
        fn objc_msgSend();
    }

    unsafe {
        let raw = objc_msgSend as *const ();
        let msg_send_void: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
            std::mem::transmute(raw);
        let msg_send_int: unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) -> *mut c_void =
            std::mem::transmute(raw);
        let msg_send_usize: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void =
            std::mem::transmute(raw);
        let msg_send_bool: unsafe extern "C" fn(*mut c_void, *mut c_void, bool) -> *mut c_void =
            std::mem::transmute(raw);
        let cls_name = c"NSApplication";
        let ns_application = objc_getClass(cls_name.as_ptr());
        if ns_application.is_null() {
            return;
        }

        let shared = msg_send_void(ns_application, sel_registerName(c"sharedApplication".as_ptr()));
        if shared.is_null() {
            return;
        }

        // NSApplicationActivationPolicyRegular = 0
        msg_send_int(shared, sel_registerName(c"setActivationPolicy:".as_ptr()), 0);
        msg_send_int(shared, sel_registerName(c"unhide:".as_ptr()), 0);
        msg_send_bool(
            shared,
            sel_registerName(c"activateIgnoringOtherApps:".as_ptr()),
            true,
        );

        let ns_running = objc_getClass(c"NSRunningApplication".as_ptr());
        if ns_running.is_null() {
            return;
        }
        let current = msg_send_void(ns_running, sel_registerName(c"currentApplication".as_ptr()));
        if current.is_null() {
            return;
        }
        // NSApplicationActivateAllWindows | NSApplicationActivateIgnoringOtherApps = 1 | 2 = 3
        msg_send_usize(current, sel_registerName(c"activateWithOptions:".as_ptr()), 3);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn activate() {}
