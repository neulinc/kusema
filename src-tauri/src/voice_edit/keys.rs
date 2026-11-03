//! The standard Windows keystrokes behind the keyboard commands, so they work
//! in most apps: Ctrl+Shift+Left selects a word, Home and Shift+End a line,
//! Ctrl+Z undoes, and so on.

use super::commands::EditCommand;
use crate::clipboard::OnTarget;
use enigo::{Direction, Enigo, Key, Keyboard};
use tauri::AppHandle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavKey {
    Left,
    Right,
    Home,
    End,
    Delete,
    A,
    C,
    X,
    V,
    Z,
}

/// A key with its modifiers, pressed `times` times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub key: NavKey,
    pub times: u32,
}

const fn key(key: NavKey) -> Chord {
    Chord {
        ctrl: false,
        shift: false,
        key,
        times: 1,
    }
}

const fn ctrl(key: NavKey) -> Chord {
    Chord {
        ctrl: true,
        ..self::key(key)
    }
}

/// The keystrokes for a command, if it's done with keys alone.
pub fn chords(command: &EditCommand) -> Option<Vec<Chord>> {
    use EditCommand::*;
    use NavKey::*;
    Some(match command {
        SelectLastWords(n) => vec![Chord {
            ctrl: true,
            shift: true,
            key: Left,
            times: *n,
        }],
        SelectLine => vec![
            key(Home),
            Chord {
                shift: true,
                ..key(End)
            },
        ],
        SelectAll => vec![ctrl(A)],
        LineStart => vec![key(Home)],
        LineEnd => vec![key(End)],
        DocumentStart => vec![ctrl(Home)],
        DocumentEnd => vec![ctrl(End)],
        DeleteThat => vec![key(Delete)],
        UndoThat => vec![ctrl(Z)],
        CopyThat => vec![ctrl(C)],
        CutThat => vec![ctrl(X)],
        PasteThat => vec![ctrl(V)],
        NewParagraph | NewLine | SelectLastSentence | SelectLastParagraph | SelectPhrase(_) => {
            return None
        }
    })
}

/// Shift+Left `steps` times: selects that many characters before the caret.
pub fn select_back(steps: usize) -> Chord {
    Chord {
        shift: true,
        times: steps as u32,
        ..key(NavKey::Left)
    }
}

/// Right: collapses a selection to its end, where the caret was.
pub fn collapse_to_end() -> Chord {
    key(NavKey::Right)
}

fn enigo_key(key: NavKey) -> Key {
    // Virtual-key codes for letters, so a shortcut means the same on any
    // keyboard layout (as the paste does).
    #[cfg(target_os = "windows")]
    let letter = |vk: u32, _: char| Key::Other(vk);
    #[cfg(not(target_os = "windows"))]
    let letter = |_: u32, c: char| Key::Unicode(c);
    match key {
        NavKey::Left => Key::LeftArrow,
        NavKey::Right => Key::RightArrow,
        NavKey::Home => Key::Home,
        NavKey::End => Key::End,
        NavKey::Delete => Key::Delete,
        NavKey::A => letter(0x41, 'a'),
        NavKey::C => letter(0x43, 'c'),
        NavKey::X => letter(0x58, 'x'),
        NavKey::V => letter(0x56, 'v'),
        NavKey::Z => letter(0x5A, 'z'),
    }
}

fn press(enigo: &mut Enigo, chord: Chord, times: u32) -> Result<(), String> {
    let fail = |e: enigo::InputError| format!("Couldn't press a key: {e}");
    if chord.ctrl {
        enigo.key(Key::Control, Direction::Press).map_err(fail)?;
    }
    if chord.shift {
        enigo.key(Key::Shift, Direction::Press).map_err(fail)?;
    }
    let mut result = Ok(());
    for _ in 0..times {
        if let Err(e) = enigo.key(enigo_key(chord.key), Direction::Click) {
            result = Err(fail(e));
            break;
        }
    }
    // Always let go of the modifiers.
    if chord.shift {
        let _ = enigo.key(Key::Shift, Direction::Release);
    }
    if chord.ctrl {
        let _ = enigo.key(Key::Control, Direction::Release);
    }
    result
}

/// How many repeats are sent between checks of the window.
const PER_CHECK: u32 = 25;

/// Presses `chords` in the focused app. Stops, returning `false`, as soon as
/// `on_target` says the window changed.
pub fn send(app: &AppHandle, chords: &[Chord], on_target: OnTarget) -> Result<bool, String> {
    for &chord in chords {
        let mut left = chord.times;
        while left > 0 {
            let batch = left.min(PER_CHECK);
            let sent = crate::clipboard::input_on_target(app, on_target.clone(), move |enigo| {
                press(enigo, chord, batch)
            })?;
            if !sent {
                return Ok(false);
            }
            left -= batch;
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use EditCommand::*;

    #[test]
    fn word_selection_is_ctrl_shift_left_per_word() {
        assert_eq!(
            chords(&SelectLastWords(3)),
            Some(vec![Chord {
                ctrl: true,
                shift: true,
                key: NavKey::Left,
                times: 3
            }])
        );
    }

    #[test]
    fn standard_shortcuts() {
        let single = |c: &EditCommand| chords(c).unwrap()[0];
        assert_eq!(single(&SelectAll), ctrl(NavKey::A));
        assert_eq!(single(&UndoThat), ctrl(NavKey::Z));
        assert_eq!(single(&CopyThat), ctrl(NavKey::C));
        assert_eq!(single(&CutThat), ctrl(NavKey::X));
        assert_eq!(single(&PasteThat), ctrl(NavKey::V));
        assert_eq!(single(&DocumentStart), ctrl(NavKey::Home));
        assert_eq!(single(&DocumentEnd), ctrl(NavKey::End));
        assert_eq!(single(&LineStart), key(NavKey::Home));
        assert_eq!(single(&LineEnd), key(NavKey::End));
        assert_eq!(single(&DeleteThat), key(NavKey::Delete));
        let line = chords(&SelectLine).unwrap();
        assert_eq!(line[0], key(NavKey::Home));
        assert!(line[1].shift && line[1].key == NavKey::End);
    }

    #[test]
    fn text_and_ui_automation_commands_are_not_keystrokes() {
        for command in [
            NewParagraph,
            NewLine,
            SelectLastSentence,
            SelectLastParagraph,
            SelectPhrase("x".into()),
        ] {
            assert_eq!(chords(&command), None);
        }
    }

    #[test]
    fn selecting_back_counts_characters() {
        let chord = select_back(12);
        assert!(chord.shift && !chord.ctrl);
        assert_eq!((chord.key, chord.times), (NavKey::Left, 12));
    }
}
