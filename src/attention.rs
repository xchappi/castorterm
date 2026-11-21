//! Getting the user's attention when an agent waits for them and the window
//! is in the background: the taskbar button flashes until the window is
//! brought to the front (Windows). Elsewhere this does nothing for now.

use gpui::Window;

/// Flashes the window's taskbar button until it comes to the foreground.
pub fn request(window: &Window) {
    #[cfg(windows)]
    flash(window);
    #[cfg(not(windows))]
    let _ = window;
}

#[cfg(windows)]
fn flash(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    struct FlashWInfo {
        size: u32,
        hwnd: isize,
        flags: u32,
        count: u32,
        timeout: u32,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn FlashWindowEx(info: *const FlashWInfo) -> i32;
    }
    // Flash the taskbar button until the window is in the foreground.
    const FLASHW_TRAY: u32 = 0x2;
    const FLASHW_TIMERNOFG: u32 = 0xC;

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return;
    };
    let info = FlashWInfo {
        size: std::mem::size_of::<FlashWInfo>() as u32,
        hwnd: win32.hwnd.get(),
        flags: FLASHW_TRAY | FLASHW_TIMERNOFG,
        count: 0,
        timeout: 0,
    };
    // SAFETY: `info` is a valid FLASHWINFO for the duration of the call and
    // `hwnd` is this window's handle.
    unsafe {
        FlashWindowEx(&info);
    }
}
