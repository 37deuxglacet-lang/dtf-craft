// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! The DTF windows' own wording. Some English strings of DTF Craft are PhotoCraft's too ("Cutout",
//! "Outline", "Input"…), with another meaning there; `build.rs` puts DTF Craft's translation of
//! those under the `dtf` context. While a DTF window draws ([`scope`]), `tl!` takes that row first,
//! so the window reads "Détourage" where PhotoCraft's own filter keeps "Découpage".

use std::cell::Cell;

/// The context of the DTF rows that PhotoCraft's catalog would otherwise shadow.
pub const CONTEXT: &str = "dtf";

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// While the returned guard lives on this thread, `tl!` prefers the `dtf` rows.
#[must_use]
pub fn scope() -> impl Drop {
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            DEPTH.set(DEPTH.get().saturating_sub(1));
        }
    }
    DEPTH.set(DEPTH.get() + 1);
    Leave
}

/// Whether a DTF window is drawing on this thread.
pub fn active() -> bool {
    DEPTH.get() > 0
}

/// Whether `command` is one of DTF Craft's (its dialogs draw in the DTF [`scope`]).
pub fn is_dtf_command(command: &str) -> bool {
    command.starts_with("dtf.") || command.starts_with("filter.dtf.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scope_nests_and_ends() {
        assert!(!active());
        {
            let _a = scope();
            let _b = scope();
            assert!(active());
        }
        assert!(!active());
        assert!(is_dtf_command("filter.dtf.halftone") && is_dtf_command("dtf.vectorize") && !is_dtf_command("filter.blur.gaussianBlur"));
    }

    /// In French, "Cutout" is PhotoCraft's "Découpage", and DTF Craft's "Détourage" in its windows.
    #[test]
    fn dtf_windows_read_their_own_wording() {
        if !crate::i18n::needs_catalogs() {
            return;
        }
        let fr = crate::i18n::Lang::from_pref("fr");
        assert_eq!(crate::i18n::tr(fr, "Cutout"), "Découpage");
        let _dtf = scope();
        assert_eq!(crate::i18n::tr(fr, "Cutout"), "Détourage");
        assert_eq!(crate::i18n::tr(fr, "Outline"), "Contour");
        assert_eq!(crate::i18n::tr(fr, "Image Size…"), crate::i18n::tr_ctx(fr, "", "Image Size…"), "the other strings as before");
    }
}
