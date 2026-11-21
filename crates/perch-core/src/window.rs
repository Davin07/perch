//! Windows-only window polish for the app shell.

#[cfg(windows)]
pub fn polish_titlebar() {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn find(hwnd: HWND, out: LPARAM) -> i32 {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == std::process::id()
            && IsWindowVisible(hwnd) != 0
            && GetWindowTextLengthW(hwnd) > 0
        {
            *(out as *mut HWND) = hwnd;
            return 0;
        }
        1
    }

    std::thread::spawn(|| {
        for _ in 0..30 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let mut hwnd: HWND = std::ptr::null_mut();
            unsafe {
                EnumWindows(Some(find), &mut hwnd as *mut HWND as LPARAM);
            }
            if hwnd.is_null() {
                continue;
            }
            let one: u32 = 1;
            let no_border: u32 = 0xFFFF_FFFE;
            let caption: u32 = 0x000E0B0A; // COLORREF #0A0B0E
            let text: u32 = 0x00F7F5F4;
            unsafe {
                for (attr, value) in [
                    (20u32, &one as *const u32),      // DWMWA_USE_IMMERSIVE_DARK_MODE
                    (34, &no_border as *const u32),   // DWMWA_BORDER_COLOR = none
                    (35, &caption as *const u32),     // DWMWA_CAPTION_COLOR
                    (36, &text as *const u32),        // DWMWA_TEXT_COLOR
                ] {
                    DwmSetWindowAttribute(hwnd, attr, value as *const _, 4);
                }
            }
            return;
        }
    });
}

#[cfg(not(windows))]
pub fn polish_titlebar() {}
