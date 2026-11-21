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

/// Physical desktop placement: where the app window should open and its physical client size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// The chosen display is faster than the primary one: render on the discrete GPU.
    pub prefer_discrete: bool,
}

/// The placement that makes the window both fully visible and fastest: the display with the
/// highest refresh rate, with `logical` scaled to its DPI and clamped to its work area.
#[cfg(windows)]
pub fn placement(logical: (f32, f32)) -> Option<Placement> {
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::{LPARAM, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, EnumDisplaySettingsW, GetMonitorInfoW, DEVMODEW,
        ENUM_CURRENT_SETTINGS, HDC, HMONITOR, MONITORINFOEXW,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    struct Display {
        monitor: HMONITOR,
        work: RECT,
        hz: u32,
        primary: bool,
        device: [u16; 32],
    }

    unsafe extern "system" fn collect(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let displays = &mut *(data as *mut Vec<Display>);
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, &mut info as *mut MONITORINFOEXW as *mut _) == 0 {
            return 1;
        }
        let mut mode: DEVMODEW = std::mem::zeroed();
        mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        let hz = if EnumDisplaySettingsW(info.szDevice.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode) != 0 {
            mode.dmDisplayFrequency
        } else {
            0
        };
        displays.push(Display {
            monitor,
            work: info.monitorInfo.rcWork,
            hz,
            primary: info.monitorInfo.dwFlags & 1 != 0,
            device: info.szDevice,
        });
        1
    }

    let mut displays: Vec<Display> = Vec::new();
    unsafe {
        EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(collect), &mut displays as *mut _ as LPARAM);
    }
    // Highest refresh wins; the primary breaks a tie.
    let best = displays.iter().max_by_key(|d| (d.hz, u8::from(d.primary)))?;
    let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
    unsafe {
        GetDpiForMonitor(best.monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
    }
    let scale = f32::from(dpi_x.max(96) as u16) / 96.0;
    let work_w = (best.work.right - best.work.left).max(1);
    let work_h = (best.work.bottom - best.work.top).max(1);
    let margin = (24.0 * scale).round() as i32;
    let width = ((logical.0 * scale).round() as i32).clamp(640, (work_w - margin).max(640));
    let height = ((logical.1 * scale).round() as i32).clamp(480, (work_h - margin).max(480));
    let primary_hz = displays.iter().find(|d| d.primary).map_or(0, |d| d.hz);
    let _ = best.device;
    Some(Placement {
        x: best.work.left + (work_w - width) / 2,
        y: best.work.top + (work_h - height) / 2,
        width: width as u32,
        height: height as u32,
        prefer_discrete: !best.primary && best.hz > primary_hz,
    })
}

#[cfg(not(windows))]
pub fn placement(_logical: (f32, f32)) -> Option<Placement> { None }
