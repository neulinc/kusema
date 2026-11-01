//! What continuous dictation asks of the operating system. Windows only for
//! now; elsewhere a session refuses to start, because the window guard can't
//! keep its promise there.

use super::guard::{FocusToken, Foreground};

#[cfg(target_os = "windows")]
mod imp {
    use super::{FocusToken, Foreground};
    use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetAncestor, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GA_ROOT,
        GUITHREADINFO,
    };

    pub const SUPPORTED: bool = true;

    pub fn foreground() -> Option<Foreground> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() {
            return None;
        }
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        let hwnd = if root.is_invalid() { hwnd } else { root };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        Some(Foreground {
            window: hwnd.0 as isize,
            own: pid == std::process::id(),
        })
    }

    pub fn focus() -> Option<FocusToken> {
        let window = foreground()?.window;
        let thread = unsafe { GetWindowThreadProcessId(HWND(window as *mut _), None) };
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        // Some apps don't report it; the window alone still has to match then.
        let control = match unsafe { GetGUIThreadInfo(thread, &mut info) } {
            Ok(()) => info.hwndFocus.0 as isize,
            Err(_) => 0,
        };
        Some(FocusToken { window, control })
    }

    /// Whether Alt, Shift or Windows is down, which would change what a typed
    /// shortcut means. Ctrl isn't asked about: the paste sends Ctrl itself.
    pub fn modifier_held() -> bool {
        [VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0)
    }

    /// When the keyboard or mouse was last used, on the system's tick clock.
    /// Includes input Kusema itself injected, so read it after typing.
    pub fn last_input_tick() -> u32 {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        match unsafe { GetLastInputInfo(&mut info) }.as_bool() {
            true => info.dwTime,
            // Unknown: a fresh value every time refuses "scratch that", the safe way.
            false => unsafe { GetTickCount() },
        }
    }

    /// Puts `text` on the clipboard with the markers that keep it out of
    /// clipboard history and cloud clipboard sync (the ones Chrome uses for
    /// Incognito copies). All in one open, so nothing sees the text unmarked.
    pub fn write_clipboard_private(text: &str) -> Result<(), String> {
        unsafe {
            let mut opened = OpenClipboard(None);
            // Another app may hold it for a moment.
            for _ in 0..10 {
                if opened.is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
                opened = OpenClipboard(None);
            }
            opened.map_err(|e| format!("OpenClipboard failed: {e}"))?;
            let written = write_formats(text);
            let closed = CloseClipboard();
            written?;
            closed.map_err(|e| format!("CloseClipboard failed: {e}"))
        }
    }

    unsafe fn write_formats(text: &str) -> Result<(), String> {
        EmptyClipboard().map_err(|e| format!("EmptyClipboard failed: {e}"))?;
        for (name, value) in [
            (
                windows::core::w!("ExcludeClipboardContentFromMonitorProcessing"),
                1u32,
            ),
            (windows::core::w!("CanIncludeInClipboardHistory"), 0u32),
            (windows::core::w!("CanUploadToCloudClipboard"), 0u32),
        ] {
            let format = RegisterClipboardFormatW(name);
            if format != 0 {
                set_data(format, &value.to_ne_bytes())?;
            }
        }
        let wide: Vec<u8> = text
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_ne_bytes)
            .collect();
        set_data(CF_UNICODETEXT.0 as u32, &wide)
    }

    unsafe fn set_data(format: u32, bytes: &[u8]) -> Result<(), String> {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len())
            .map_err(|e| format!("GlobalAlloc failed: {e}"))?;
        let target = GlobalLock(memory) as *mut u8;
        if target.is_null() {
            let _ = GlobalFree(Some(memory));
            return Err("GlobalLock failed".to_string());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), target, bytes.len());
        let _ = GlobalUnlock(memory);
        // On success the clipboard owns the memory.
        SetClipboardData(format, Some(HANDLE(memory.0))).map_err(|e| {
            let _ = GlobalFree(Some(memory));
            format!("SetClipboardData failed: {e}")
        })?;
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{FocusToken, Foreground};

    pub const SUPPORTED: bool = false;

    pub fn foreground() -> Option<Foreground> {
        None
    }

    pub fn focus() -> Option<FocusToken> {
        None
    }

    pub fn modifier_held() -> bool {
        false
    }

    pub fn last_input_tick() -> u32 {
        0
    }

    pub fn write_clipboard_private(_text: &str) -> Result<(), String> {
        Err("Private clipboard writes are Windows only".to_string())
    }
}

pub use imp::{
    focus, foreground, last_input_tick, modifier_held, write_clipboard_private, SUPPORTED,
};
