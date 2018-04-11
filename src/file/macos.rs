#[cfg(target_os = "macos")]

use std::ffi::CStr;
use libc;
use cocoa::base::{class, nil, id, YES, NO};
use cocoa::foundation::{NSString, NSArray, NSInteger};

pub fn open_file() -> Option<String> {
    unsafe {
        let app: id = msg_send![class("NSApplication"), sharedApplication];
        let key_window: id = msg_send![app, keyWindow];

        let panel: id = msg_send![class("NSOpenPanel"), openPanel];
        msg_send![panel, setAllowsMultipleSelection:NO];
        let file_types = NSArray::arrayWithObject(nil, NSString::alloc(nil).init_str("wav"));
        msg_send![panel, setAllowedFileTypes:file_types];
        msg_send![panel, setCanChooseDirectories:NO];
        msg_send![panel, setCanChooseFiles:YES];
        msg_send![panel, setFloatingPanel:YES];
        let result: NSInteger = msg_send![panel, runModal];
        let path = if result == 1 {
            let url: id = msg_send![panel, URL];
            let path: id = msg_send![url, path];
            let path: *const libc::c_char = msg_send![path, UTF8String];
            CStr::from_ptr(path).to_str().ok().map(|s| s.to_string())
        } else {
            None
        };

        msg_send![key_window, makeKeyAndOrderFront:nil];

        path
    }
}
