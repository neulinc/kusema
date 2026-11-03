//! The keyboard hook that sees the Kusema key first and swallows it, so the
//! app in front never gets it: not the press, the repeats, or the release.
//!
//! What the hook does with each key event is decided by `classify`, which is
//! pure; the Windows part only feeds it and acts on the answer.

use super::keys::{vk, KusemaKey};

/// Stamped on the menu mask we inject ("KUSM"), so the hook lets it pass.
pub const MASK_MARKER: usize = 0x4B55_534D;
/// An unassigned virtual key, injected while Win is held so that releasing
/// Win after a swallowed Copilot key doesn't open the Start menu.
pub const MASK_VK: u16 = 0xE8;
/// enigo's `EVENT_MARKER`: Kusema's own typing.
pub const ENIGO_MARKER: usize = 100;

/// One key event as the hook sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKey {
    pub vk: u16,
    pub down: bool,
    /// Sent by a program (SendInput), not the keyboard.
    pub injected: bool,
    pub extra_info: usize,
    /// Win and Shift are held: how the Copilot key arrives.
    pub win_shift: bool,
}

/// An edge of the Kusema key, for the gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Down,
    Up,
    /// Space was pressed while the key is held: this take is for the Assistant.
    Space,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Action {
    /// Keep the event from every app.
    pub swallow: bool,
    pub edge: Option<Edge>,
    /// The key pressed while capturing a new Kusema key.
    pub captured: Option<Captured>,
    /// Inject the menu mask (Win is held over a swallowed key).
    pub mask: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Captured {
    pub vk: u16,
    /// It came as Win+Shift+F23: the Copilot key.
    pub copilot: bool,
}

#[derive(Debug, Clone, Default)]
pub struct HookState {
    /// `None`: the Kusema key is off.
    pub key: Option<KusemaKey>,
    /// Debug builds, for tests: treat injected keys like real ones (except
    /// Kusema's own typing and mask).
    pub accept_injected: bool,
    /// The next key pressed becomes the Kusema key.
    pub capturing: bool,
    /// The Kusema key's virtual key while it's down, so its release is
    /// swallowed even if the setting changed in between.
    held: Option<u16>,
    /// The key that was captured, while it's still down.
    captured_down: Option<u16>,
    /// A Space pressed while the key was held, until it's let go: all of it
    /// is swallowed, the release too.
    space_down: bool,
    /// What the keyboard has shown so far.
    pub seen_right_ctrl: bool,
    pub seen_copilot: bool,
}

impl HookState {
    /// The Kusema key is down now.
    pub fn key_held(&self) -> bool {
        self.held.is_some()
    }
}

/// What to do with `key`.
pub fn classify(state: &mut HookState, key: RawKey) -> Action {
    if key.extra_info == MASK_MARKER {
        return Action::default();
    }
    if key.injected && (!state.accept_injected || key.extra_info == ENIGO_MARKER) {
        return Action::default();
    }
    if key.down {
        if key.vk == vk::RCONTROL {
            state.seen_right_ctrl = true;
        }
        if key.vk == vk::F23 && key.win_shift {
            state.seen_copilot = true;
        }
    }

    // Choosing a new key: the next press is it, and nobody else gets it.
    if state.capturing && key.down {
        state.capturing = false;
        state.captured_down = Some(key.vk);
        return Action {
            swallow: true,
            captured: Some(Captured {
                vk: key.vk,
                copilot: key.vk == vk::F23 && key.win_shift,
            }),
            mask: key.win_shift,
            ..Default::default()
        };
    }
    if state.captured_down == Some(key.vk) {
        if !key.down {
            state.captured_down = None;
        }
        return Action {
            swallow: true,
            ..Default::default()
        };
    }

    // Space while the key is held switches the take to the Assistant. It never
    // reaches the app (or a classic Ctrl+Space shortcut behind this hook).
    if key.vk == vk::SPACE {
        if key.down && state.held.is_some() {
            let first = !state.space_down;
            state.space_down = true;
            return Action {
                swallow: true,
                edge: first.then_some(Edge::Space),
                ..Default::default()
            };
        }
        if !key.down && state.space_down {
            state.space_down = false;
            return Action {
                swallow: true,
                ..Default::default()
            };
        }
    }

    if let Some(held) = state.held {
        if key.vk == held {
            if key.down {
                // Auto-repeat.
                return Action {
                    swallow: true,
                    edge: Some(Edge::Down),
                    ..Default::default()
                };
            }
            state.held = None;
            return Action {
                swallow: true,
                edge: Some(Edge::Up),
                ..Default::default()
            };
        }
        return Action::default();
    }

    match &state.key {
        Some(ours) if key.down && ours.matches(key.vk, key.win_shift) => {
            state.held = Some(key.vk);
            Action {
                swallow: true,
                edge: Some(Edge::Down),
                // Win is down (the Copilot key): its release would read as a
                // lone tap and open Start without a stand-in keystroke.
                mask: key.win_shift,
                ..Default::default()
            }
        }
        _ => Action::default(),
    }
}

#[cfg(target_os = "windows")]
pub mod windows {
    use super::*;
    use ::windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use ::windows::Win32::System::Threading::GetCurrentThreadId;
    use ::windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_LSHIFT, VK_LWIN, VK_RSHIFT, VK_RWIN,
    };
    use ::windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetTimer,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
        LLKHF_INJECTED, MSG, WH_KEYBOARD_LL, WM_APP, WM_KEYDOWN, WM_SYSKEYDOWN, WM_TIMER,
    };
    use log::{error, info};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    /// What the hook sends on: an edge of the Kusema key, or a captured key.
    #[derive(Debug, Clone, Copy)]
    pub enum Event {
        Edge(Edge, Instant),
        Captured(Captured),
    }

    static STATE: Mutex<Option<HookState>> = Mutex::new(None);
    static SENDER: OnceLock<Mutex<Sender<Event>>> = OnceLock::new();

    pub fn with_state<R>(f: impl FnOnce(&mut HookState) -> R) -> R {
        let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
        f(guard.get_or_insert_with(HookState::default))
    }

    fn held(vk: VIRTUAL_KEY) -> bool {
        (unsafe { GetAsyncKeyState(vk.0 as i32) } as u16) & 0x8000 != 0
    }

    fn send_mask() {
        let input = |flags: KEYBD_EVENT_FLAGS| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(MASK_VK),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: MASK_MARKER,
                },
            },
        };
        let inputs = [input(KEYBD_EVENT_FLAGS(0)), input(KEYEVENTF_KEYUP)];
        unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    }

    unsafe extern "system" fn proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code < 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let raw = RawKey {
            vk: info.vkCode as u16,
            down: matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN),
            injected: info.flags.0 & LLKHF_INJECTED.0 != 0,
            extra_info: info.dwExtraInfo,
            win_shift: (held(VK_LWIN) || held(VK_RWIN)) && (held(VK_LSHIFT) || held(VK_RSHIFT)),
        };
        let action = with_state(|state| classify(state, raw));
        if action.edge.is_some() || action.captured.is_some() {
            if let Some(sender) = SENDER.get() {
                let sender = sender.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(edge) = action.edge {
                    let _ = sender.send(Event::Edge(edge, Instant::now()));
                }
                if let Some(captured) = action.captured {
                    let _ = sender.send(Event::Captured(captured));
                }
            }
        }
        if action.mask {
            send_mask();
        }
        if action.swallow {
            LRESULT(1)
        } else {
            CallNextHookEx(None, code, wparam, lparam)
        }
    }

    /// Asks the hook's thread to install it afresh, ahead of any hook
    /// installed since (a classic shortcut fired before Kusema's hook saw
    /// its keys).
    pub fn reinstall_soon() {
        let thread = HOOK_THREAD.load(Ordering::SeqCst);
        if thread != 0 {
            unsafe {
                let _ = PostThreadMessageW(thread, REINSTALL_NOW, WPARAM(0), LPARAM(0));
            }
        }
    }

    static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
    const REINSTALL_NOW: u32 = WM_APP + 1;

    /// Hands the gesture an event from elsewhere (a classic shortcut's Space).
    pub fn inject(event: Event) {
        if let Some(sender) = SENDER.get() {
            let _ = sender.lock().unwrap_or_else(|e| e.into_inner()).send(event);
        }
    }

    /// How often the hook is installed afresh: Windows silently drops a
    /// low-level hook that once answered too slowly, and after sleep.
    const REINSTALL_MS: u32 = 30_000;

    unsafe fn install() -> Option<HHOOK> {
        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(proc), None, 0) {
            Ok(hook) => Some(hook),
            Err(e) => {
                error!("Couldn't install the Kusema key's keyboard hook: {e}");
                None
            }
        }
    }

    /// Installs the hook on its own thread, with its message loop.
    pub fn start(sender: Sender<Event>) {
        if SENDER.set(Mutex::new(sender)).is_err() {
            return;
        }
        std::thread::Builder::new()
            .name("kusema-key".into())
            .spawn(|| unsafe {
                let Some(mut hook) = install() else {
                    return;
                };
                info!("Kusema key hook installed");
                HOOK_THREAD.store(GetCurrentThreadId(), Ordering::SeqCst);
                SetTimer(None, 0, REINSTALL_MS, None);
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    if msg.message == WM_TIMER || msg.message == REINSTALL_NOW {
                        // The new one first, so there's never a gap.
                        if let Some(fresh) = install() {
                            let _ = UnhookWindowsHookEx(hook);
                            hook = fresh;
                        }
                        continue;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                let _ = UnhookWindowsHookEx(hook);
            })
            .ok();
    }

    /// The key's name on the keyboard, as Windows gives it ("F13", "Num 0").
    pub fn key_name(vk_code: u16) -> String {
        // Keys Windows' name lookup gets wrong (Pause reads as Right Ctrl).
        let fixed = match vk_code {
            0x13 => Some("Pause"),
            0x2C => Some("Print Screen"),
            0x5D => Some("Menu"),
            0x90 => Some("Num Lock"),
            0x91 => Some("Scroll Lock"),
            _ => None,
        };
        if let Some(name) = fixed {
            return name.to_string();
        }
        use ::windows::Win32::UI::Input::KeyboardAndMouse::{
            GetKeyNameTextW, MapVirtualKeyW, MAPVK_VK_TO_VSC_EX,
        };
        unsafe {
            let scan = MapVirtualKeyW(vk_code as u32, MAPVK_VK_TO_VSC_EX);
            let extended = (scan & 0xE000) == 0xE000;
            let mut lparam = ((scan & 0xFF) as i32) << 16;
            if extended {
                lparam |= 1 << 24;
            }
            let mut buffer = [0u16; 64];
            let len = GetKeyNameTextW(lparam, &mut buffer);
            let name = String::from_utf16_lossy(&buffer[..len.max(0) as usize]);
            if name.is_empty() {
                format!("Key {vk_code:#04x}")
            } else {
                name
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(key: KusemaKey) -> HookState {
        HookState {
            key: Some(key),
            ..Default::default()
        }
    }

    fn press(vk: u16, down: bool) -> RawKey {
        RawKey {
            vk,
            down,
            injected: false,
            extra_info: 0,
            win_shift: false,
        }
    }

    #[test]
    fn the_key_is_swallowed_down_repeat_and_up() {
        let mut s = state(KusemaKey::RightCtrl);
        let down = classify(&mut s, press(vk::RCONTROL, true));
        assert!(down.swallow);
        assert_eq!(down.edge, Some(Edge::Down));
        // Auto-repeat never reaches the app either (a repeat would make the
        // app think Ctrl is held).
        let repeat = classify(&mut s, press(vk::RCONTROL, true));
        assert!(repeat.swallow);
        let up = classify(&mut s, press(vk::RCONTROL, false));
        assert!(up.swallow);
        assert_eq!(up.edge, Some(Edge::Up));
    }

    #[test]
    fn space_while_held_is_swallowed_and_reported_once() {
        let mut s = state(KusemaKey::RightCtrl);
        classify(&mut s, press(vk::RCONTROL, true));
        let space = classify(&mut s, press(vk::SPACE, true));
        assert!(space.swallow);
        assert_eq!(space.edge, Some(Edge::Space));
        // Its repeats and its release are swallowed, reported no more.
        let repeat = classify(&mut s, press(vk::SPACE, true));
        assert!(repeat.swallow && repeat.edge.is_none());
        // Let go of the Kusema key first: Space's release is still swallowed.
        classify(&mut s, press(vk::RCONTROL, false));
        assert!(classify(&mut s, press(vk::SPACE, false)).swallow);
        // Without the key held, Space is just Space.
        assert_eq!(classify(&mut s, press(vk::SPACE, true)), Action::default());
        assert_eq!(classify(&mut s, press(vk::SPACE, false)), Action::default());
    }

    #[test]
    fn other_keys_pass() {
        let mut s = state(KusemaKey::RightCtrl);
        for vk_code in [vk::LCONTROL, b'C' as u16, vk::RMENU, vk::ADD] {
            assert_eq!(classify(&mut s, press(vk_code, true)), Action::default());
            assert_eq!(classify(&mut s, press(vk_code, false)), Action::default());
        }
        // While the Kusema key is held, other keys still pass.
        classify(&mut s, press(vk::RCONTROL, true));
        assert!(!classify(&mut s, press(b'A' as u16, true)).swallow);
    }

    #[test]
    fn off_means_nothing_is_swallowed() {
        let mut s = HookState::default();
        assert!(!classify(&mut s, press(vk::RCONTROL, true)).swallow);
        assert!(!classify(&mut s, press(vk::RCONTROL, false)).swallow);
    }

    #[test]
    fn the_release_is_swallowed_even_if_the_key_changed_meanwhile() {
        let mut s = state(KusemaKey::RightCtrl);
        classify(&mut s, press(vk::RCONTROL, true));
        s.key = Some(KusemaKey::NumpadPlus);
        assert!(classify(&mut s, press(vk::RCONTROL, false)).swallow);
        // And the old key is free afterwards.
        assert!(!classify(&mut s, press(vk::RCONTROL, true)).swallow);
    }

    #[test]
    fn the_copilot_key_is_f23_with_win_and_shift_and_gets_a_menu_mask() {
        let mut s = state(KusemaKey::Copilot);
        let copilot = RawKey {
            win_shift: true,
            ..press(vk::F23, true)
        };
        let down = classify(&mut s, copilot);
        assert!(down.swallow && down.mask);
        assert!(s.seen_copilot);
        // Its release may come after Win is let go: still ours.
        let up = classify(&mut s, press(vk::F23, false));
        assert!(up.swallow);
        assert_eq!(up.edge, Some(Edge::Up));
    }

    #[test]
    fn kusemas_own_typing_and_mask_are_never_taken() {
        let mut s = state(KusemaKey::RightCtrl);
        s.accept_injected = true;
        let enigo = RawKey {
            injected: true,
            extra_info: ENIGO_MARKER,
            ..press(vk::RCONTROL, true)
        };
        assert_eq!(classify(&mut s, enigo), Action::default());
        let mask = RawKey {
            injected: true,
            extra_info: MASK_MARKER,
            ..press(MASK_VK, true)
        };
        assert_eq!(classify(&mut s, mask), Action::default());
        // Other programs' keys count only when a test asks for it.
        let other = RawKey {
            injected: true,
            ..press(vk::RCONTROL, true)
        };
        assert!(classify(&mut s, other).swallow);
        let mut real = state(KusemaKey::RightCtrl);
        assert_eq!(classify(&mut real, other), Action::default());
    }

    #[test]
    fn capturing_takes_the_next_key_and_its_release() {
        let mut s = state(KusemaKey::RightCtrl);
        s.capturing = true;
        let down = classify(&mut s, press(0x13, true));
        assert!(down.swallow);
        assert_eq!(
            down.captured,
            Some(Captured {
                vk: 0x13,
                copilot: false
            })
        );
        assert!(classify(&mut s, press(0x13, false)).swallow);
        assert!(!s.capturing);
        // Afterwards it's an ordinary key again until it's saved.
        assert!(!classify(&mut s, press(0x13, true)).swallow);
    }

    #[test]
    fn the_keyboard_s_keys_are_noted() {
        let mut s = HookState::default();
        assert!(!s.seen_right_ctrl);
        classify(&mut s, press(vk::RCONTROL, true));
        assert!(s.seen_right_ctrl);
    }
}
