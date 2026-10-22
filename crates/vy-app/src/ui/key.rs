//! Key event mapping from GPUI to editor keys.

use gpui::Keystroke;
use vy_app::editor::Key;

/// Converts a GPUI keystroke into an editor key.
pub fn convert_key(keystroke: &Keystroke) -> Option<Key> {
    if keystroke.modifiers.control {
        let mut characters = keystroke.key.chars();
        return match (characters.next(), characters.next()) {
            (Some(character), None) if character.is_ascii_alphabetic() => {
                Some(Key::Ctrl(character.to_ascii_lowercase()))
            }
            _ => None,
        };
    }

    if keystroke.modifiers.alt || keystroke.modifiers.platform {
        return None;
    }

    match keystroke.key.as_str() {
        "escape" => return Some(Key::Esc),
        "enter" => return Some(Key::Enter),
        "backspace" => return Some(Key::Backspace),
        "delete" => return Some(Key::Delete),
        "tab" => return Some(Key::Tab),
        "left" => return Some(Key::Left),
        "right" => return Some(Key::Right),
        "up" => return Some(Key::Up),
        "down" => return Some(Key::Down),
        "home" => return Some(Key::Home),
        "end" => return Some(Key::End),
        "space" => return Some(Key::Char(' ')),
        _ => {}
    }

    // The committed text reaches Insert mode through the input handler, so commands use
    // the key name. Shift is applied here because key names are unshifted.
    let mut characters = keystroke.key.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }

    if keystroke.modifiers.shift && character.is_ascii_lowercase() {
        return Some(Key::Char(character.to_ascii_uppercase()));
    }

    Some(Key::Char(character))
}
