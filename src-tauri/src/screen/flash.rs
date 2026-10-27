//! A brief border around the captured window, so the user sees what was
//! captured. Drawn after the capture, so it's never in the screenshot.

use super::ScreenRect;

#[cfg(target_os = "windows")]
pub fn flash(rect: ScreenRect) {
    std::thread::Builder::new()
        .name("screen-flash".into())
        .spawn(move || {
            if let Err(e) = windows_impl::run(rect) {
                log::debug!("Couldn't flash the captured window: {e}");
            }
        })
        .ok();
}

#[cfg(not(target_os = "windows"))]
pub fn flash(_rect: ScreenRect) {}

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::ScreenRect;
    use std::time::{Duration, Instant};
    use windows::core::w;
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, CreateSolidBrush, DeleteObject, SetWindowRgn, HBRUSH, RGN_DIFF,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, PeekMessageW,
        RegisterClassW, SetLayeredWindowAttributes, ShowWindow, TranslateMessage, LWA_ALPHA, MSG,
        PM_REMOVE, SW_SHOWNOACTIVATE, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    };

    /// Kusema's accent (logo) colour, as 0x00BBGGRR.
    const COLOR: u32 = 0x00FB_979B;
    const THICKNESS: i32 = 4;
    const PEAK_ALPHA: u8 = 220;
    const HOLD: Duration = Duration::from_millis(250);
    const FADE: Duration = Duration::from_millis(350);

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    pub fn run(rect: ScreenRect) -> windows::core::Result<()> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class = w!("KusemaCaptureFlash");
            // Registered once; the class keeps its brush for the app's lifetime.
            static REGISTERED: std::sync::Once = std::sync::Once::new();
            REGISTERED.call_once(|| {
                let brush = CreateSolidBrush(COLORREF(COLOR));
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: instance.into(),
                    lpszClassName: class,
                    hbrBackground: HBRUSH(brush.0),
                    ..Default::default()
                });
            });

            // Just outside the window, so the border frames it.
            let (x, y) = (rect.left - THICKNESS, rect.top - THICKNESS);
            let (w, h) = (rect.width() + 2 * THICKNESS, rect.height() + 2 * THICKNESS);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_NOACTIVATE,
                class,
                w!(""),
                WS_POPUP,
                x,
                y,
                w,
                h,
                None,
                None,
                Some(instance.into()),
                None,
            )?;

            // Only the frame is part of the window; the middle stays clickable
            // and visible.
            let frame = CreateRectRgn(0, 0, w, h);
            let inner = CreateRectRgn(THICKNESS, THICKNESS, w - THICKNESS, h - THICKNESS);
            CombineRgn(Some(frame), Some(frame), Some(inner), RGN_DIFF);
            let _ = DeleteObject(inner.into());
            SetWindowRgn(hwnd, Some(frame), false); // the window owns `frame` now

            SetLayeredWindowAttributes(hwnd, COLORREF(0), PEAK_ALPHA, LWA_ALPHA)?;
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            let start = Instant::now();
            loop {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let elapsed = start.elapsed();
                if elapsed >= HOLD + FADE {
                    break;
                }
                if elapsed > HOLD {
                    let left = 1.0 - (elapsed - HOLD).as_secs_f32() / FADE.as_secs_f32();
                    let alpha = (PEAK_ALPHA as f32 * left.max(0.0)) as u8;
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                }
                std::thread::sleep(Duration::from_millis(16));
            }

            let _ = DestroyWindow(hwnd);
            Ok(())
        }
    }
}
