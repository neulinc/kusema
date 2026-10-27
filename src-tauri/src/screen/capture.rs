//! Finding and capturing the foreground window.
//!
//! Windows: the window's visible bounds (DWM extended frame, in physical
//! pixels, as Kusema is per-monitor DPI aware) are copied from the composited
//! screen with GDI. That's what the user sees at the moment of the press, GPU
//! rendered content included, from any monitor or across two; it never waits
//! on the target app (unlike `PrintWindow`) and needs no capture session (unlike
//! Windows Graphics Capture, which also draws its own yellow border on
//! Windows 10). The window is in front when the hotkey is pressed, so nothing
//! else covers it apart from always-on-top windows, which the confirmation
//! card would show.

#[cfg(target_os = "windows")]
pub use windows_impl::*;

#[cfg(not(target_os = "windows"))]
pub use other::*;

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::super::{Capture, CaptureError, CapturedWindow, ScreenRect};
    use image::RgbImage;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Dwm::{
        DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
    };
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetClassNameW, GetForegroundWindow, GetSystemMetrics, GetWindowRect,
        GetWindowThreadProcessId, IsIconic, SetForegroundWindow, GA_ROOT, SM_CXVIRTUALSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    /// Smallest window worth capturing, in pixels.
    const MIN_SIZE: i32 = 32;

    /// What a top-level window's class says it is.
    pub(crate) fn classify(class: &str) -> Option<CaptureError> {
        match class {
            "Progman" | "WorkerW" => Some(CaptureError::Desktop),
            "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "TopLevelWindowForOverflowXamlIsland"
            | "Windows.UI.Core.CoreWindow"
            | "XamlExplorerHostIslandWindow" => Some(CaptureError::Taskbar),
            _ => None,
        }
    }

    fn class_name(hwnd: HWND) -> String {
        let mut buf = [0u16; 256];
        let len = unsafe { GetClassNameW(hwnd, &mut buf) };
        String::from_utf16_lossy(&buf[..len.max(0) as usize])
    }

    fn virtual_screen() -> ScreenRect {
        unsafe {
            let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
            ScreenRect {
                left,
                top,
                right: left + GetSystemMetrics(SM_CXVIRTUALSCREEN),
                bottom: top + GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        }
    }

    fn bounds(hwnd: HWND) -> Option<ScreenRect> {
        let mut rect = RECT::default();
        // The extended frame excludes the invisible resize borders.
        let dwm = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut rect as *mut RECT as *mut _,
                std::mem::size_of::<RECT>() as u32,
            )
        };
        if dwm.is_err() {
            unsafe { GetWindowRect(hwnd, &mut rect) }.ok()?;
        }
        Some(ScreenRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
    }

    fn is_cloaked(hwnd: HWND) -> bool {
        let mut cloaked = 0u32;
        unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut u32 as *mut _,
                std::mem::size_of::<u32>() as u32,
            )
        }
        .is_ok()
            && cloaked != 0
    }

    /// The window the user is working in, if it's one that can be captured.
    pub fn foreground_window() -> Result<CapturedWindow, CaptureError> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() {
            return Err(CaptureError::NoWindow);
        }
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        let hwnd = if root.is_invalid() { hwnd } else { root };

        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == std::process::id() {
            return Err(CaptureError::OwnWindow);
        }
        if let Some(e) = classify(&class_name(hwnd)) {
            return Err(e);
        }
        if unsafe { IsIconic(hwnd) }.as_bool() || is_cloaked(hwnd) {
            return Err(CaptureError::NoWindow);
        }

        let rect = bounds(hwnd)
            .ok_or(CaptureError::NoWindow)?
            .intersect(&virtual_screen());
        if rect.width() < MIN_SIZE || rect.height() < MIN_SIZE {
            return Err(CaptureError::NoWindow);
        }
        Ok(CapturedWindow {
            handle: hwnd.0 as isize,
            rect,
        })
    }

    /// Copy the window's area of the screen.
    pub fn capture(window: CapturedWindow) -> Result<Capture, CaptureError> {
        let ScreenRect { left, top, .. } = window.rect;
        let (width, height) = (window.rect.width(), window.rect.height());
        let failed = |what: &str| CaptureError::Failed(what.to_string());

        let mut bgra = vec![0u8; width as usize * height as usize * 4];
        unsafe {
            let screen = GetDC(None);
            if screen.is_invalid() {
                return Err(failed("no screen device context"));
            }
            let memory = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, width, height);
            let previous = SelectObject(memory, bitmap.into());

            let copied = BitBlt(
                memory,
                0,
                0,
                width,
                height,
                Some(screen),
                left,
                top,
                SRCCOPY,
            );
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    // Negative: rows top to bottom.
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let lines = if copied.is_ok() {
                GetDIBits(
                    memory,
                    bitmap,
                    0,
                    height as u32,
                    Some(bgra.as_mut_ptr().cast()),
                    &mut info,
                    DIB_RGB_COLORS,
                )
            } else {
                0
            };

            SelectObject(memory, previous);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);

            if copied.is_err() {
                return Err(failed("copying the screen failed"));
            }
            if lines != height {
                return Err(failed("reading the pixels failed"));
            }
        }

        let rgb = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|px| [px[2], px[1], px[0]])
            .collect();
        let image = RgbImage::from_raw(width as u32, height as u32, rgb)
            .ok_or_else(|| failed("unexpected image size"))?;
        Ok(Capture { image, window })
    }

    /// Bring a window back to the front.
    pub fn activate(handle: isize) {
        unsafe {
            let _ = SetForegroundWindow(HWND(handle as *mut _));
        }
    }

    /// The work area (screen minus taskbar) of the monitor showing most of
    /// `rect`, and that monitor's scale factor.
    pub fn monitor_work_area(rect: ScreenRect) -> Option<(ScreenRect, f64)> {
        use windows::Win32::Graphics::Gdi::{
            GetMonitorInfoW, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };
        use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
        let r = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        unsafe {
            let monitor = MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return None;
            }
            let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
            let work = info.rcWork;
            Some((
                ScreenRect {
                    left: work.left,
                    top: work.top,
                    right: work.right,
                    bottom: work.bottom,
                },
                dpi_x as f64 / 96.0,
            ))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn shell_windows_are_not_captured() {
            assert_eq!(classify("Progman"), Some(CaptureError::Desktop));
            assert_eq!(classify("WorkerW"), Some(CaptureError::Desktop));
            assert_eq!(classify("Shell_TrayWnd"), Some(CaptureError::Taskbar));
            assert_eq!(classify("OpusApp"), None); // Word
            assert_eq!(classify("Chrome_WidgetWin_1"), None);
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod other {
    use super::super::{Capture, CaptureError, CapturedWindow, ScreenRect};

    pub fn foreground_window() -> Result<CapturedWindow, CaptureError> {
        Err(CaptureError::Unsupported)
    }

    pub fn capture(_window: CapturedWindow) -> Result<Capture, CaptureError> {
        Err(CaptureError::Unsupported)
    }

    pub fn activate(_handle: isize) {}

    pub fn monitor_work_area(_rect: ScreenRect) -> Option<(ScreenRect, f64)> {
        None
    }
}
