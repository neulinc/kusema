//! What voice editing asks of Windows: UI Automation's text pattern, to read
//! the selection and to select a sentence, paragraph or phrase without the
//! clipboard, and which app and control has the focus.
//!
//! UI Automation calls go to the other app, so each runs on its own thread
//! with COM set up there, and with short timeouts: an app that doesn't answer
//! counts as one without the text pattern.

/// Why text couldn't be read or selected through UI Automation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiaError {
    /// The focused control doesn't offer the text pattern.
    NoTextPattern,
    /// Nothing to work on before the caret.
    NothingBefore,
    NotFound,
    Failed(String),
}

/// What to select.
#[derive(Clone, Debug)]
pub enum Selection {
    LastWords(u32),
    /// The caret's line, without its line break.
    Line,
    LastSentence,
    LastParagraph,
    Phrase(String),
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{Selection, UiaError};
    use crate::voice_edit::text;
    use windows::core::{Interface, BSTR, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationTextPattern,
        IUIAutomationTextPattern2, IUIAutomationTextRange, TextPatternRangeEndpoint_End,
        TextPatternRangeEndpoint_Start, TextUnit_Line, TextUnit_Paragraph, UIA_TextPatternId,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetWindowThreadProcessId};

    /// Whether Ctrl, Alt, Shift or Windows is down: a key sent now would mean
    /// something else (Shift+Left with Ctrl held selects words, not letters).
    pub fn any_modifier_held() -> bool {
        [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
            .iter()
            .any(|key| unsafe { GetAsyncKeyState(i32::from(key.0)) } < 0)
    }

    /// Paragraphs before the caret read for "select last sentence/paragraph".
    const PARAGRAPHS_BACK: i32 = 3;
    /// Milliseconds an app has to answer.
    const TIMEOUT_MS: u32 = 2000;
    /// How long a browser gets to switch on its accessibility tree.
    const ACCESSIBILITY_WAKE_MS: u64 = 400;

    /// Runs `f` on a fresh COM thread with the focused control's text pattern.
    fn with_text_pattern<T: Send + 'static>(
        f: impl FnOnce(&IUIAutomationTextPattern) -> Result<T, UiaError> + Send + 'static,
    ) -> Result<T, UiaError> {
        std::thread::spawn(move || unsafe {
            if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
                return Err(UiaError::Failed("COM".into()));
            }
            let result = (|| {
                let automation: IUIAutomation =
                    CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                        .map_err(|e| UiaError::Failed(e.to_string()))?;
                if let Ok(automation2) = automation.cast::<IUIAutomation2>() {
                    let _ = automation2.SetConnectionTimeout(TIMEOUT_MS);
                    let _ = automation2.SetTransactionTimeout(TIMEOUT_MS);
                }
                let pattern = || -> Option<IUIAutomationTextPattern> {
                    automation
                        .GetFocusedElement()
                        .ok()?
                        .GetCurrentPatternAs(UIA_TextPatternId)
                        .ok()
                };
                // Chrome and Edge build their accessibility tree only once
                // something like this asks, so the first try can come up empty.
                let pattern = pattern()
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(ACCESSIBILITY_WAKE_MS));
                        pattern()
                    })
                    .ok_or(UiaError::NoTextPattern)?;
                f(&pattern)
            })();
            CoUninitialize();
            result
        })
        .join()
        .unwrap_or_else(|_| Err(UiaError::Failed("UI Automation thread panicked".into())))
    }

    fn failed(e: windows::core::Error) -> UiaError {
        UiaError::Failed(e.message())
    }

    unsafe fn first_selection(
        pattern: &IUIAutomationTextPattern,
    ) -> Result<Option<IUIAutomationTextRange>, UiaError> {
        let ranges = pattern.GetSelection().map_err(failed)?;
        if ranges.Length().map_err(failed)? < 1 {
            return Ok(None);
        }
        Ok(Some(ranges.GetElement(0).map_err(failed)?))
    }

    /// A range at the caret: the text pattern 2 caret if there is one, else
    /// the end of the selection.
    unsafe fn caret(
        pattern: &IUIAutomationTextPattern,
    ) -> Result<IUIAutomationTextRange, UiaError> {
        if let Ok(pattern2) = pattern.cast::<IUIAutomationTextPattern2>() {
            let mut active = windows::core::BOOL::default();
            if let Ok(range) = pattern2.GetCaretRange(&mut active) {
                return Ok(range);
            }
        }
        let range = first_selection(pattern)?.ok_or(UiaError::NoTextPattern)?;
        range
            .MoveEndpointByRange(
                TextPatternRangeEndpoint_Start,
                &range,
                TextPatternRangeEndpoint_End,
            )
            .map_err(failed)?;
        Ok(range)
    }

    /// The selected text: `Some("")` when the control has the text pattern
    /// but nothing is selected.
    pub fn selected_text() -> Result<String, UiaError> {
        with_text_pattern(|pattern| unsafe {
            match first_selection(pattern)? {
                Some(range) => Ok(range.GetText(-1).map_err(failed)?.to_string()),
                None => Ok(String::new()),
            }
        })
    }

    pub fn select(what: Selection) -> Result<(), UiaError> {
        with_text_pattern(move |pattern| unsafe {
            let caret = caret(pattern)?;
            let document = pattern.DocumentRange().map_err(failed)?;
            // From the start of the document to the caret.
            let before = document.Clone().map_err(failed)?;
            before
                .MoveEndpointByRange(
                    TextPatternRangeEndpoint_End,
                    &caret,
                    TextPatternRangeEndpoint_Start,
                )
                .map_err(failed)?;
            let found = match what {
                Selection::Line => {
                    let line = caret.Clone().map_err(failed)?;
                    line.ExpandToEnclosingUnit(TextUnit_Line).map_err(failed)?;
                    let line_text = line.GetText(-1).map_err(failed)?.to_string();
                    let wanted = line_text.trim_end_matches(['\r', '\n']);
                    if wanted.trim().is_empty() {
                        return Err(UiaError::NothingBefore);
                    }
                    find(&line, wanted, false, false)
                }
                Selection::LastWords(_) | Selection::LastSentence | Selection::LastParagraph => {
                    // Only the last few paragraphs are read, not the document.
                    let region = caret.Clone().map_err(failed)?;
                    region
                        .MoveEndpointByUnit(
                            TextPatternRangeEndpoint_Start,
                            TextUnit_Paragraph,
                            -PARAGRAPHS_BACK,
                        )
                        .map_err(failed)?;
                    let region_text = region.GetText(-1).map_err(failed)?.to_string();
                    let wanted = match what {
                        Selection::LastWords(n) => text::last_words(&region_text, n as usize),
                        Selection::LastSentence => text::last_sentence(&region_text),
                        _ => text::last_paragraph(&region_text),
                    }
                    .ok_or(UiaError::NothingBefore)?
                    .to_string();
                    find(&region, &wanted, true, false)
                }
                Selection::Phrase(phrase) => {
                    let phrase = phrase.trim();
                    let trimmed = phrase
                        .strip_prefix("the ")
                        .or_else(|| phrase.strip_prefix("The "))
                        .unwrap_or(phrase);
                    // Nearest before the caret, else the first anywhere.
                    find(&before, phrase, true, true)
                        .or_else(|| find(&before, trimmed, true, true))
                        .or_else(|| find(&document, phrase, false, true))
                        .or_else(|| find(&document, trimmed, false, true))
                }
            };
            let range = found.ok_or(UiaError::NotFound)?;
            range.Select().map_err(failed)
        })
    }

    unsafe fn find(
        within: &IUIAutomationTextRange,
        needle: &str,
        backward: bool,
        ignore_case: bool,
    ) -> Option<IUIAutomationTextRange> {
        if needle.is_empty() {
            return None;
        }
        within
            .FindText(&BSTR::from(needle), backward, ignore_case)
            .ok()
    }

    /// The lowercase file name of the process that owns `window`, and the
    /// class of the control focused in it.
    pub fn app_of(window: isize, control: isize) -> (String, String) {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(HWND(window as *mut _), Some(&mut pid)) };
        let process = unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                .ok()
                .and_then(|handle| {
                    let mut buffer = [0u16; 1024];
                    let mut size = buffer.len() as u32;
                    let ok = QueryFullProcessImageNameW(
                        handle,
                        PROCESS_NAME_WIN32,
                        PWSTR(buffer.as_mut_ptr()),
                        &mut size,
                    );
                    let _ = CloseHandle(handle);
                    ok.ok()?;
                    let path = String::from_utf16_lossy(&buffer[..size as usize]);
                    path.rsplit(['\\', '/']).next().map(str::to_lowercase)
                })
                .unwrap_or_default()
        };
        let class = if control == 0 {
            String::new()
        } else {
            let mut buffer = [0u16; 256];
            let len = unsafe { GetClassNameW(HWND(control as *mut _), &mut buffer) };
            String::from_utf16_lossy(&buffer[..len.max(0) as usize])
        };
        (process, class)
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{Selection, UiaError};

    pub fn selected_text() -> Result<String, UiaError> {
        Err(UiaError::NoTextPattern)
    }

    pub fn select(_what: Selection) -> Result<(), UiaError> {
        Err(UiaError::NoTextPattern)
    }

    pub fn app_of(_window: isize, _control: isize) -> (String, String) {
        (String::new(), String::new())
    }

    pub fn any_modifier_held() -> bool {
        false
    }
}

pub use imp::{any_modifier_held, app_of, select, selected_text};
