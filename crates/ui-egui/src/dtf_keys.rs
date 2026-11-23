// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! Shortcuts named in the DTF windows' texts, in the platform's notation. The texts (tooltips, help
//! sheets, in every language) write them the Windows way: "Ctrl+G", "Ctrl+Maj+G", "Strg 0",
//! "Ctrl+molette". The keys themselves are Cmd-based (`Modifiers::COMMAND`, egui's zoom modifier),
//! so on macOS they are ⌘G, ⌘⇧G, ⌘0, ⌘+molette: [`keys`] rewrites them there and leaves the text
//! as it is elsewhere.

/// `text` with its Ctrl shortcuts in this platform's notation (unchanged outside macOS).
pub fn keys(text: &str) -> String {
    for_platform(text, cfg!(target_os = "macos"))
}

/// [`keys`] for macOS (`mac`) or another platform.
pub fn for_platform(text: &str, mac: bool) -> String {
    if !mac {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while let Some((at, len)) = next_ctrl(rest) {
        out.push_str(&rest[..at]);
        out.push('⌘');
        let mut after = &rest[at + len..];
        // Further modifiers: a word followed by '+' ("Ctrl+Alt+Z", "Ctrl+Maj+G").
        while let Some(next) = after.strip_prefix('+') {
            let word_len = next.find('+').unwrap_or(0);
            let word = &next[..word_len];
            if word.chars().count() < 2 || !word.chars().all(char::is_alphabetic) {
                break;
            }
            out.push_str(modifier(word));
            after = &next[word_len..];
        }
        // The key: one character joins the symbols ("⌘G", "⌘−", "⌘0"), a word keeps its '+'
        // ("⌘+molette").
        if let Some(next) = after.strip_prefix('+').or_else(|| after.strip_prefix(' ')) {
            let word: String = next.chars().take_while(|c| c.is_alphanumeric()).collect();
            match word.chars().count() {
                0 => {
                    if let Some(c) = next.chars().next().filter(|c| matches!(c, '+' | '−' | '-' | '=')) {
                        out.push(c);
                        after = &next[c.len_utf8()..];
                    }
                }
                1 => {
                    out.push_str(&word);
                    after = &next[word.len()..];
                }
                _ => {
                    out.push('+');
                    out.push_str(&word);
                    after = &next[word.len()..];
                }
            }
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// The next "Ctrl" (or German "Strg") that starts a word: its byte offset and length.
fn next_ctrl(text: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    loop {
        let tail = text.get(from..)?;
        let (at, len) = [("Ctrl", 4), ("Strg", 4)].iter().filter_map(|(w, l)| tail.find(w).map(|i| (from + i, *l))).min_by_key(|(i, _)| *i)?;
        let word_start = !text[..at].chars().next_back().is_some_and(char::is_alphanumeric);
        if word_start {
            return Some((at, len));
        }
        from = at + len;
    }
}

/// The macOS symbol of a modifier named in a shortcut (Alt in every language, else Shift:
/// "Maj", "Umschalt", "Mayús"…).
fn modifier(word: &str) -> &'static str {
    match word.to_lowercase().as_str() {
        "alt" | "opt" | "option" => "⌥",
        _ => "⇧",
    }
}

#[cfg(test)]
mod tests {
    use super::for_platform;

    #[test]
    fn shortcut_text_unchanged_outside_macos() {
        for s in ["Group (Ctrl+G)", "Zoom In (Ctrl +)", "Ctrl+Alt+Z annule", "Auszoomen (Strg −)"] {
            assert_eq!(for_platform(s, false), s);
        }
    }

    #[test]
    fn shortcut_text_on_macos() {
        let mac = |s| for_platform(s, true);
        assert_eq!(mac("Group (Ctrl+G)"), "Group (⌘G)");
        assert_eq!(mac("Undo (Ctrl+Z)"), "Undo (⌘Z)");
        assert_eq!(mac("Ungroup (Ctrl+Shift+G)"), "Ungroup (⌘⇧G)");
        assert_eq!(mac("Dégrouper (Ctrl+Maj+G)"), "Dégrouper (⌘⇧G)");
        assert_eq!(mac("Desagrupar (Ctrl+Mayús+G)"), "Desagrupar (⌘⇧G)");
        assert_eq!(mac("Gruppierung aufheben (Strg+Umschalt+G)"), "Gruppierung aufheben (⌘⇧G)");
        assert_eq!(mac("Zoom arrière (Ctrl −)"), "Zoom arrière (⌘−)");
        assert_eq!(mac("Zoom avant (Ctrl +)"), "Zoom avant (⌘+)");
        assert_eq!(mac("Le visuel entier (Ctrl 0)"), "Le visuel entier (⌘0)");
        assert_eq!(mac("全体（Ctrl 0）"), "全体（⌘0）");
        assert_eq!(mac("Avant d'appliquer, Ctrl+Alt+Z annule"), "Avant d'appliquer, ⌘⌥Z annule");
        assert_eq!(mac("zoomez (Ctrl+molette, jusqu'à 800 %)"), "zoomez (⌘+molette, jusqu'à 800 %)");
        assert_eq!(mac("Ctrl+V puis Ctrl+D"), "⌘V puis ⌘D");
        // Not a shortcut: left as it is.
        assert_eq!(mac("Maj+clic, Alt+clic"), "Maj+clic, Alt+clic");
        assert_eq!(mac("XCtrl+G"), "XCtrl+G");
        assert_eq!(mac(""), "");
    }
}
