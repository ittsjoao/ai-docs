/// Como o agregador trata uma tecla pressionada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyKind {
    /// Caractere digitado: só é contado, nunca gravado.
    Typing,
    /// Tecla com significado ("Enter", "Ctrl+S"), gravada como evento `key`.
    Combo(String),
    Ignore,
}

fn is_modifier(vk: u32) -> bool {
    matches!(
        vk,
        0x10 | 0x11 | 0x12 | 0x14 | 0x5B | 0x5C | 0x90 | 0x91 | 0xA0..=0xA5
    )
}

fn is_printable(vk: u32) -> bool {
    matches!(vk, 0x20 | 0x30..=0x39 | 0x41..=0x5A | 0x60..=0x6F | 0xBA..=0xC2 | 0xDB..=0xDF | 0xE2)
}

fn key_name(vk: u32) -> Option<String> {
    let name = match vk {
        0x0D => "Enter",
        0x09 => "Tab",
        0x1B => "Esc",
        0x2E => "Delete",
        0x20 => "Space",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x70..=0x7B => return Some(format!("F{}", vk - 0x6F)),
        0x30..=0x39 | 0x41..=0x5A => return char::from_u32(vk).map(String::from),
        _ => return None,
    };
    Some(name.to_string())
}

pub(crate) fn classify_key(vk: u32, ctrl: bool, alt: bool, shift: bool) -> KeyKind {
    if is_modifier(vk) || vk == 0x08 {
        return KeyKind::Ignore; // Backspace edita o texto; não é passo nem caractere novo
    }
    let printable = is_printable(vk);
    // Ctrl+Alt+imprimível = AltGr no ABNT2: é digitação
    if printable && (ctrl == alt) {
        return KeyKind::Typing;
    }
    let Some(name) = key_name(vk) else {
        return KeyKind::Ignore;
    };
    let mut combo = String::new();
    if ctrl {
        combo += "Ctrl+";
    }
    if alt {
        combo += "Alt+";
    }
    if shift && (ctrl || alt || !printable) {
        combo += "Shift+";
    }
    combo += &name;
    KeyKind::Combo(combo)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn combo(s: &str) -> KeyKind {
        KeyKind::Combo(s.to_string())
    }

    #[test]
    fn printable_without_ctrl_alt_is_typing() {
        assert_eq!(classify_key(0x41, false, false, false), KeyKind::Typing); // A
        assert_eq!(classify_key(0x41, false, false, true), KeyKind::Typing); // Shift+A
        assert_eq!(classify_key(0x20, false, false, false), KeyKind::Typing); // espaço
        assert_eq!(classify_key(0xBA, false, false, false), KeyKind::Typing); // OEM (ç no ABNT2)
    }

    #[test]
    fn altgr_is_typing() {
        // AltGr chega como Ctrl+Alt no hook (ex.: AltGr+Q = '/' no ABNT2)
        assert_eq!(classify_key(0x51, true, true, false), KeyKind::Typing);
    }

    #[test]
    fn named_keys_and_shortcuts_are_combos() {
        assert_eq!(classify_key(0x0D, false, false, false), combo("Enter"));
        assert_eq!(classify_key(0x09, false, false, true), combo("Shift+Tab"));
        assert_eq!(classify_key(0x1B, false, false, false), combo("Esc"));
        assert_eq!(classify_key(0x2E, false, false, false), combo("Delete"));
        assert_eq!(classify_key(0x74, false, false, false), combo("F5"));
        assert_eq!(classify_key(0x28, false, false, false), combo("Down"));
        assert_eq!(classify_key(0x53, true, false, false), combo("Ctrl+S"));
        assert_eq!(classify_key(0x5A, true, false, false), combo("Ctrl+Z"));
        assert_eq!(classify_key(0x73, false, true, false), combo("Alt+F4"));
        assert_eq!(
            classify_key(0x1B, true, false, true),
            combo("Ctrl+Shift+Esc")
        );
        assert_eq!(classify_key(0x20, true, false, false), combo("Ctrl+Space"));
    }

    #[test]
    fn modifiers_backspace_and_unnamed_are_ignored() {
        for vk in [0x10, 0x11, 0x12, 0xA0, 0xA2, 0xA4, 0x5B, 0x14, 0x08] {
            assert_eq!(
                classify_key(vk, false, false, false),
                KeyKind::Ignore,
                "vk {vk:#x}"
            );
        }
        assert_eq!(classify_key(0xBA, true, false, false), KeyKind::Ignore); // Ctrl+OEM sem nome
    }
}
