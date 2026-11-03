//! Draws the glow: four thin layered windows on a thread of their own, which
//! sleeps until it's told something changed. Each strip is drawn once and then
//! left to the compositor; only the pulse changes anything while recording.

use super::{current_style, pixel_alpha, pulse_alpha, strips, Style};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Once;
use std::time::{Duration, Instant};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW,
    MonitorFromWindow, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HDC, MONITORINFO,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow,
    MsgWaitForMultipleObjects, PeekMessageW, PostThreadMessageW, RegisterClassW,
    SetWindowDisplayAffinity, SetWindowPos, ShowWindow, SystemParametersInfoW, TranslateMessage,
    UpdateLayeredWindow, HWND_TOPMOST, MSG, PM_NOREMOVE, PM_REMOVE, QS_ALLINPUT,
    SPI_GETCLIENTAREAANIMATION, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SW_HIDE,
    SW_SHOWNOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, ULW_ALPHA, WDA_EXCLUDEFROMCAPTURE,
    WM_APP, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

static THREAD_ID: AtomicU32 = AtomicU32::new(0);
static STARTED: Once = Once::new();

/// How often to look again while pulsing, and while not.
const PULSE_TICK: Duration = Duration::from_millis(100);
const STILL_TICK: Duration = Duration::from_millis(250);
/// Other always-on-top windows can come in front; go back on top this often.
const TOPMOST_EVERY: Duration = Duration::from_secs(2);
/// Length of a dash when paused, in logical pixels.
const DASH: i32 = 64;

/// Tells the drawing thread something changed, starting it the first time.
pub fn wake() {
    STARTED.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("recording-glow".into())
            .spawn(run);
        if let Err(e) = spawned {
            log::warn!("Couldn't start the recording glow: {e}");
        }
    });
    let id = THREAD_ID.load(Ordering::Acquire);
    if id != 0 {
        unsafe {
            let _ = PostThreadMessageW(id, WM_APP, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn create_strip() -> windows::core::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = w!("KusemaRecordingGlow");
        static REGISTERED: Once = Once::new();
        REGISTERED.call_once(|| {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
        });
        let hwnd = CreateWindowExW(
            // Click-through, never takes focus, not in the taskbar or Alt+Tab.
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!(""),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        // It's for the person at the computer: not in screenshots, in what Ask
        // about my screen captures, or in a screen share.
        if !capturable_for_testing() {
            let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        }
        Ok(hwnd)
    }
}

/// Debug builds only: lets a test take a picture of the glow (and of the
/// recording pill, with `KUSEMA_DEBUG_CAPTURABLE`).
fn capturable_for_testing() -> bool {
    cfg!(debug_assertions)
        && (std::env::var_os("KUSEMA_DEBUG_GLOW_CAPTURABLE").is_some()
            || std::env::var_os("KUSEMA_DEBUG_CAPTURABLE").is_some())
}

/// Keeps one of Kusema's windows for the person at the computer: out of
/// screenshots, screen shares and what Ask about my screen captures. It still
/// shows on screen as usual.
pub fn exclude_from_capture(hwnd: isize) {
    if capturable_for_testing() {
        return;
    }
    unsafe {
        let _ = SetWindowDisplayAffinity(HWND(hwnd as *mut _), WDA_EXCLUDEFROMCAPTURE);
    }
}

/// Windows' "Animation effects" setting. If it can't be read, stay still.
fn animations_allowed() -> bool {
    let mut on = windows::core::BOOL(0);
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut on as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
            && on.as_bool()
    }
}

/// The monitor to glow on, where the user is working: its bounds in physical
/// pixels, and its scale.
fn active_monitor() -> Option<(RECT, f32)> {
    unsafe {
        let monitor = MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let (mut dpi_x, mut dpi_y) = (96, 96);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        Some((info.rcMonitor, dpi_x as f32 / 96.0))
    }
}

/// What the strips currently show; a difference means drawing them again.
#[derive(PartialEq, Clone, Copy)]
struct Drawn {
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    thickness: i32,
    dash: i32,
    rgb: (u8, u8, u8),
}

fn draw_strip(hwnd: HWND, screen: HDC, at: [i32; 4], drawn: &Drawn, alpha: u8) {
    let [x, y, width, height] = at;
    if width <= 0 || height <= 0 {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        return;
    }
    unsafe {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let memory = CreateCompatibleDC(Some(screen));
        let mut bits = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(memory), &info, DIB_RGB_COLORS, &mut bits, None, 0)
        else {
            let _ = DeleteDC(memory);
            return;
        };
        let pixels =
            std::slice::from_raw_parts_mut(bits as *mut u32, width as usize * height as usize);
        let (r, g, b) = (drawn.rgb.0 as u32, drawn.rgb.1 as u32, drawn.rgb.2 as u32);
        let (origin_x, origin_y) = (x - drawn.left, y - drawn.top);
        for row in 0..height {
            for column in 0..width {
                let a = pixel_alpha(
                    origin_x + column,
                    origin_y + row,
                    drawn.width,
                    drawn.height,
                    drawn.thickness,
                    drawn.dash,
                ) as u32;
                // Premultiplied BGRA, as UpdateLayeredWindow expects.
                pixels[(row * width + column) as usize] =
                    (a << 24) | ((r * a / 255) << 16) | ((g * a / 255) << 8) | (b * a / 255);
            }
        }
        let previous = SelectObject(memory, bitmap.into());
        let position = POINT { x, y };
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let source = POINT { x: 0, y: 0 };
        let _ = UpdateLayeredWindow(
            hwnd,
            Some(screen),
            Some(&position),
            Some(&size),
            Some(memory),
            Some(&source),
            COLORREF(0),
            Some(&blend(alpha)),
            ULW_ALPHA,
        );
        SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

fn blend(alpha: u8) -> BLENDFUNCTION {
    BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: alpha,
        AlphaFormat: AC_SRC_ALPHA as u8,
    }
}

fn run() {
    unsafe {
        // Makes the thread's message queue, so `wake` can reach it.
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        THREAD_ID.store(GetCurrentThreadId(), Ordering::Release);
    }
    let mut windows = Vec::with_capacity(4);
    for _ in 0..4 {
        match create_strip() {
            Ok(hwnd) => windows.push(hwnd),
            Err(e) => {
                log::warn!("Couldn't create the recording glow: {e}");
                return;
            }
        }
    }

    let mut drawn: Option<Drawn> = None;
    let mut last_alpha = 0u8;
    let mut began = Instant::now();
    let mut last_topmost = Instant::now();

    loop {
        let style = current_style();
        let wait = match style {
            None => {
                if drawn.take().is_some() {
                    for hwnd in &windows {
                        unsafe {
                            let _ = ShowWindow(*hwnd, SW_HIDE);
                        }
                    }
                }
                None
            }
            Some(style) => {
                let started = drawn.is_none();
                if started {
                    began = Instant::now();
                }
                let pulsing = style.pulses && animations_allowed();
                let alpha = if pulsing {
                    pulse_alpha(style.alpha, began.elapsed().as_secs_f32())
                } else {
                    style.alpha
                };
                show(&windows, style, alpha, &mut drawn, &mut last_alpha);
                if started || last_topmost.elapsed() >= TOPMOST_EVERY {
                    last_topmost = Instant::now();
                    for hwnd in &windows {
                        unsafe {
                            let _ = SetWindowPos(
                                *hwnd,
                                Some(HWND_TOPMOST),
                                0,
                                0,
                                0,
                                0,
                                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                            );
                        }
                    }
                }
                Some(if pulsing { PULSE_TICK } else { STILL_TICK })
            }
        };

        unsafe {
            let timeout = wait.map_or(u32::MAX, |d| d.as_millis() as u32);
            MsgWaitForMultipleObjects(None, false, timeout, QS_ALLINPUT);
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

fn show(windows: &[HWND], style: Style, alpha: u8, drawn: &mut Option<Drawn>, last_alpha: &mut u8) {
    let Some((monitor, scale)) = active_monitor() else {
        return;
    };
    let wanted = Drawn {
        left: monitor.left,
        top: monitor.top,
        width: monitor.right - monitor.left,
        height: monitor.bottom - monitor.top,
        thickness: (style.thickness as f32 * scale).round() as i32,
        dash: if style.dashed {
            (DASH as f32 * scale).round() as i32
        } else {
            0
        },
        rgb: style.rgb,
    };
    if *drawn != Some(wanted) {
        unsafe {
            let screen = GetDC(None);
            let places = strips(
                wanted.left,
                wanted.top,
                wanted.width,
                wanted.height,
                wanted.thickness,
            );
            for (hwnd, at) in windows.iter().zip(places) {
                draw_strip(*hwnd, screen, at, &wanted, alpha);
            }
            ReleaseDC(None, screen);
        }
        *drawn = Some(wanted);
        *last_alpha = alpha;
    } else if *last_alpha != alpha {
        // Only the strength changed: the compositor keeps the picture.
        for hwnd in windows {
            unsafe {
                let _ = UpdateLayeredWindow(
                    *hwnd,
                    None,
                    None,
                    None,
                    None,
                    None,
                    COLORREF(0),
                    Some(&blend(alpha)),
                    ULW_ALPHA,
                );
            }
        }
        *last_alpha = alpha;
    }
}
