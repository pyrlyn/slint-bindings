//! WinUI `CoreTextEditContext` updates, turned into Slint composition calls.
//!
//! The edit context thinks it owns a text store. The store here is only the
//! current preedit: the committed text lives in the Slint `TextInput`, which
//! a custom platform cannot read back. While a composition is open, each
//! replacement is a preedit. When it closes, that preedit is committed or
//! dropped. A replacement with no composition is a direct insert (a character
//! the input method did not mark, or a multi-scalar string such as an emoji).

use crate::Error;

/// What the host should deliver to Slint for one input-method update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ImeUpdate {
    /// Nothing changed.
    Ignored,
    /// Show `text` as the preedit. Offsets are UTF-16 code units.
    Preedit {
        /// Marked text.
        text: String,
        /// Selection start, UTF-16. Negative means no selection.
        utf16_start: i32,
        /// Selection end, UTF-16.
        utf16_end: i32,
    },
    /// Insert `text` and clear the preedit.
    Commit {
        /// Committed text.
        text: String,
    },
    /// Drop the preedit without inserting.
    Clear,
    /// A single committed scalar, delivered as a key press and release.
    Character {
        /// The scalar, as UTF-8 text.
        text: String,
    },
}

/// The preedit the WinUI edit context is editing.
#[derive(Debug, Default)]
pub(crate) struct ImeSession {
    preedit: String,
    composing: bool,
    sel_start: i32,
    sel_end: i32,
}

impl ImeSession {
    pub(crate) fn composing(&self) -> bool {
        self.composing
    }

    /// Text the edit context may ask for. Empty when nothing is composing,
    /// so the next direct insert lands at offset 0.
    pub(crate) fn document(&self) -> &str {
        if self.composing { &self.preedit } else { "" }
    }

    pub(crate) fn selection(&self) -> (i32, i32) {
        if self.composing {
            (self.sel_start, self.sel_end)
        } else {
            (0, 0)
        }
    }

    pub(crate) fn started(&mut self) {
        self.composing = true;
    }

    /// Replace the UTF-16 range `[start, end)` inside the preedit with `text`.
    ///
    /// Offsets past either end clamp. An offset that falls inside a scalar is
    /// rejected so the store stays valid UTF-8.
    pub(crate) fn replace(
        &mut self,
        range_start: i32,
        range_end: i32,
        text: &str,
        sel_start: i32,
        sel_end: i32,
    ) -> Result<ImeUpdate, Error> {
        if self.composing {
            let next = replace_utf16(&self.preedit, range_start, range_end, text)?;
            self.preedit = next;
            self.sel_start = sel_start;
            self.sel_end = sel_end;
            return Ok(ImeUpdate::Preedit {
                text: self.preedit.clone(),
                utf16_start: sel_start,
                utf16_end: sel_end,
            });
        }
        if text.is_empty() {
            return Ok(ImeUpdate::Ignored);
        }
        self.preedit.clear();
        self.sel_start = 0;
        self.sel_end = 0;
        if utf16_units(text) == 1 {
            Ok(ImeUpdate::Character {
                text: text.to_owned(),
            })
        } else {
            Ok(ImeUpdate::Commit {
                text: text.to_owned(),
            })
        }
    }

    pub(crate) fn set_selection(&mut self, start: i32, end: i32) -> ImeUpdate {
        if !self.composing {
            return ImeUpdate::Ignored;
        }
        self.sel_start = start;
        self.sel_end = end;
        ImeUpdate::Preedit {
            text: self.preedit.clone(),
            utf16_start: start,
            utf16_end: end,
        }
    }

    /// End the composition. A cancel drops the preedit; otherwise it is committed.
    pub(crate) fn completed(&mut self, canceled: bool) -> ImeUpdate {
        let pending = std::mem::take(&mut self.preedit);
        let was_open = self.composing;
        self.composing = false;
        self.sel_start = 0;
        self.sel_end = 0;
        if !was_open && pending.is_empty() {
            return ImeUpdate::Ignored;
        }
        if canceled || pending.is_empty() {
            ImeUpdate::Clear
        } else {
            ImeUpdate::Commit { text: pending }
        }
    }
}

fn utf16_units(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

fn replace_utf16(text: &str, start: i32, end: i32, insert: &str) -> Result<String, Error> {
    let start = start.max(0);
    let end = end.max(start);
    let start_byte = utf16_to_byte(text, start)?;
    let end_byte = utf16_to_byte(text, end)?;
    let mut out = String::with_capacity(text.len() + insert.len());
    out.push_str(&text[..start_byte]);
    out.push_str(insert);
    out.push_str(&text[end_byte..]);
    Ok(out)
}

/// Byte index of a UTF-16 caret. Past-the-end clamps to `text.len()`.
/// A caret inside a scalar is an error.
fn utf16_to_byte(text: &str, utf16_offset: i32) -> Result<usize, Error> {
    if utf16_offset <= 0 {
        return Ok(0);
    }
    let target = utf16_offset as usize;
    let mut units = 0usize;
    for (byte, ch) in text.char_indices() {
        if units == target {
            return Ok(byte);
        }
        units += ch.len_utf16();
        if units > target {
            return Err(Error::InvalidArgument(
                "the IME caret falls inside a Unicode scalar",
            ));
        }
    }
    Ok(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composition_replaces_the_preedit_until_it_commits() -> Result<(), Error> {
        let mut ime = ImeSession::default();
        ime.started();
        let update = ime.replace(0, 0, "あ", 0, 1)?;
        assert_eq!(
            update,
            ImeUpdate::Preedit {
                text: "あ".to_owned(),
                utf16_start: 0,
                utf16_end: 1,
            }
        );
        assert_eq!(ime.document(), "あ");
        let update = ime.replace(0, 1, "日", 0, 1)?;
        assert!(matches!(update, ImeUpdate::Preedit { ref text, .. } if text == "日"));
        assert_eq!(
            ime.completed(false),
            ImeUpdate::Commit {
                text: "日".to_owned()
            }
        );
        assert!(!ime.composing());
        assert_eq!(ime.document(), "");
        Ok(())
    }

    #[test]
    fn cancel_clears_and_a_direct_insert_does_not_stick_in_the_store() -> Result<(), Error> {
        let mut ime = ImeSession::default();
        ime.started();
        ime.replace(0, 0, "か", 0, 1)?;
        assert_eq!(ime.completed(true), ImeUpdate::Clear);
        assert_eq!(ime.document(), "");

        let update = ime.replace(0, 0, "A", 1, 1)?;
        assert_eq!(
            update,
            ImeUpdate::Character {
                text: "A".to_owned()
            }
        );
        assert_eq!(ime.document(), "");

        let update = ime.replace(0, 0, "😀", 2, 2)?;
        assert_eq!(
            update,
            ImeUpdate::Commit {
                text: "😀".to_owned()
            }
        );
        Ok(())
    }

    #[test]
    fn closing_twice_is_ignored() {
        let mut ime = ImeSession::default();
        assert_eq!(ime.completed(false), ImeUpdate::Ignored);
        assert_eq!(ime.set_selection(0, 1), ImeUpdate::Ignored);
    }

    #[test]
    fn replacement_rejects_a_caret_inside_the_final_surrogate_pair() -> Result<(), Error> {
        let mut ime = ImeSession::default();
        ime.started();
        ime.replace(0, 0, "a😀", 1, 3)?;
        for (start, end) in [(2, 2), (1, 2), (2, 3)] {
            assert!(ime.replace(start, end, "x", 0, 0).is_err());
            assert_eq!(ime.document(), "a😀");
            assert_eq!(ime.selection(), (1, 3));
        }
        ime.replace(1, 3, "b", 2, 2)?;
        assert_eq!(ime.document(), "ab");
        ime.replace(99, 99, "c", 3, 3)?;
        assert_eq!(ime.document(), "abc");
        Ok(())
    }
}
