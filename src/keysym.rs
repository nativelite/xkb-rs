//! Keysyms: the numbers a keymap gives its keys, their names, and the
//! characters they stand for.

use crate::keysym_table::{BY_NAME, BY_VALUE, TO_UCS};

/// A keysym: the symbol on a key (`a`, `Return`, `dead_acute`, `XF86AudioMute`).
///
/// The number is the X11 keysym value. Latin-1 characters are their own
/// keysym, other characters have a legacy value or `0x0100_0000 + code point`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub struct Keysym(pub u32);

/// The first keysym of the range that encodes a code point directly.
const UNICODE_BASE: u32 = 0x0100_0000;
/// The last keysym of that range.
const UNICODE_LAST: u32 = 0x0110_ffff;

fn hex(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 8 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

/// The one character of a case mapping, if it is one character.
fn single(mut mapped: impl Iterator<Item = char>) -> Option<char> {
    match (mapped.next(), mapped.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

/// Unicode's simple upper-case mapping. The standard library has the full
/// mapping, which differs where the upper case is several characters: there
/// the simple mapping is the single-character form if Unicode has one (the
/// Greek letters with iota subscript), and nothing otherwise - except for
/// the sharp s, which gets its capital form as keyboards have it.
fn simple_upper(c: char) -> Option<char> {
    let cp = c as u32;
    let special = match cp {
        0xdf => 0x1e9e,
        0x1f80..=0x1f87 | 0x1f90..=0x1f97 | 0x1fa0..=0x1fa7 => cp + 8,
        0x1fb3 | 0x1fc3 | 0x1ff3 => cp + 9,
        _ => return single(c.to_uppercase()),
    };
    char::from_u32(special)
}

/// Unicode's simple lower-case mapping (see [`simple_upper`]): the capital
/// I with dot is the one letter whose full lower case is two characters.
fn simple_lower(c: char) -> Option<char> {
    if c == '\u{130}' {
        Some('i')
    } else {
        single(c.to_lowercase())
    }
}

impl Keysym {
    /// No symbol: an empty level, or "none" where a keysym is expected.
    pub const NO_SYMBOL: Keysym = Keysym(0);
    /// The symbol a keymap uses for "a key with deliberately nothing on it".
    pub const VOID_SYMBOL: Keysym = Keysym(0x00ff_ffff);
    /// The largest valid keysym.
    pub const MAX: Keysym = Keysym(0x1fff_ffff);

    /// The keysym called `name`, case-sensitively: a name from the keysym
    /// list (`"Return"`, `"eacute"`), `U` and a code point in hex
    /// (`"U20AC"`), or `0x` and the number in hex (`"0x1008ff11"`).
    ///
    /// `"NoSymbol"` is a name like any other and gives [`Keysym::NO_SYMBOL`];
    /// a name that is not one gives `None`.
    pub fn from_name(name: &str) -> Option<Keysym> {
        if let Ok(i) = BY_NAME.binary_search_by(|(n, _)| n.as_bytes().cmp(name.as_bytes())) {
            return Some(Keysym(BY_NAME[i].1));
        }
        if let Some(digits) = name.strip_prefix('U') {
            let cp = hex(digits)?;
            return match cp {
                0x20..=0x7e | 0xa0..=0xff => Some(Keysym(cp)),
                0x100..=0x10_ffff => Some(Keysym(cp | UNICODE_BASE)),
                _ => None,
            };
        }
        if let Some(digits) = name.strip_prefix("0x") {
            return hex(digits).filter(|&v| v <= Self::MAX.0).map(Keysym);
        }
        // Older keymaps wrote some XF86 names with an underscore.
        if let Some(rest) = name.strip_prefix("XF86_") {
            let joined = format!("XF86{rest}");
            if let Ok(i) = BY_NAME.binary_search_by(|(n, _)| n.as_bytes().cmp(joined.as_bytes())) {
                return Some(Keysym(BY_NAME[i].1));
            }
        }
        None
    }

    /// The keysym's name: its name in the keysym list if it has one, else
    /// `U` and the code point for a directly encoded character, else `0x`
    /// and the number. [`Keysym::from_name`] reads all three back.
    pub fn name(self) -> String {
        if let Ok(i) = BY_VALUE.binary_search_by_key(&self.0, |&(v, _)| v) {
            return BY_VALUE[i].1.to_string();
        }
        if (0x0100_0100..=UNICODE_LAST).contains(&self.0) {
            let cp = self.0 & 0x00ff_ffff;
            return if cp > 0xffff {
                format!("U{cp:08X}")
            } else {
                format!("U{cp:04X}")
            };
        }
        format!("0x{:08x}", self.0)
    }

    /// The code point the keysym stands for, or 0 if it has none. Keys that
    /// send a control character have one: `Return` is 0x0d, `Tab` is 0x09,
    /// `KP_1` is `'1'`.
    pub fn to_utf32(self) -> u32 {
        let ks = self.0;
        if (0x20..=0x7e).contains(&ks) || (0xa0..=0xff).contains(&ks) {
            return ks;
        }
        if (UNICODE_BASE..=UNICODE_LAST).contains(&ks) {
            let cp = ks - UNICODE_BASE;
            // A surrogate is not a character.
            return if (0xd800..=0xdfff).contains(&cp) {
                0
            } else {
                cp
            };
        }
        match TO_UCS.binary_search_by_key(&ks, |&(k, _)| k) {
            Ok(i) => TO_UCS[i].1,
            Err(_) => 0,
        }
    }

    /// The character the keysym stands for, if any (see
    /// [`Keysym::to_utf32`]).
    pub fn to_char(self) -> Option<char> {
        match self.to_utf32() {
            0 => None,
            cp => char::from_u32(cp),
        }
    }

    /// The keysym for a character: the legacy keysym where the keysym list
    /// has one (`'é'` is `eacute`, `'я'` is `Cyrillic_ya`), else the directly
    /// encoded one. Unicode noncharacters give [`Keysym::NO_SYMBOL`].
    pub fn from_char(c: char) -> Keysym {
        let cp = c as u32;
        if (0x20..=0x7e).contains(&cp) || (0xa0..=0xff).contains(&cp) {
            return Keysym(cp);
        }
        // BackSpace, Tab, Linefeed, Clear, Return, Escape; Delete.
        if (0x08..=0x0b).contains(&cp) || cp == 0x0d || cp == 0x1b {
            return Keysym(cp | 0xff00);
        }
        if cp == 0x7f {
            return Keysym(0xffff);
        }
        if (0xfdd0..=0xfdef).contains(&cp) || cp & 0xfffe == 0xfffe {
            return Keysym::NO_SYMBOL;
        }
        if cp >= 0x100 {
            if let Some(&(ks, _)) = TO_UCS.iter().find(|&&(_, u)| u == cp) {
                return Keysym(ks);
            }
        }
        Keysym(cp | UNICODE_BASE)
    }

    /// `to`, as a keysym in the same encoding as `self`.
    fn recased(self, to: char) -> Keysym {
        if (UNICODE_BASE..=UNICODE_LAST).contains(&self.0) {
            Keysym(to as u32 | UNICODE_BASE)
        } else {
            Keysym::from_char(to)
        }
    }

    /// Whether the keysym stands for a cased letter at all: a control key
    /// such as `Return` has a code point but no case.
    fn letter(self) -> Option<char> {
        let ks = self.0;
        if (0xfd00..=0xffff).contains(&ks) {
            return None;
        }
        self.to_char()
    }

    /// The upper-case keysym of a lower-case letter, by Unicode's simple
    /// (one character to one character) case mapping; any other keysym
    /// unchanged. `ssharp` becomes the capital sharp s, U+1E9E.
    pub fn to_upper(self) -> Keysym {
        match self
            .letter()
            .and_then(|c| simple_upper(c).filter(|u| *u != c))
        {
            Some(u) => self.recased(u),
            None => self,
        }
    }

    /// The lower-case keysym of an upper-case letter; any other keysym
    /// unchanged.
    pub fn to_lower(self) -> Keysym {
        match self
            .letter()
            .and_then(|c| simple_lower(c).filter(|l| *l != c))
        {
            Some(l) => self.recased(l),
            None => self,
        }
    }

    /// Whether this is a lower-case letter (one with an upper case).
    pub fn is_lower(self) -> bool {
        self.to_upper() != self
    }

    /// Whether this is an upper-case letter (one with a lower case).
    pub fn is_upper(self) -> bool {
        self.to_lower() != self
    }

    /// Whether this is a keypad keysym (`KP_Space` to `KP_Equal`).
    pub fn is_keypad(self) -> bool {
        (0xff80..=0xffbd).contains(&self.0)
    }
}

impl From<char> for Keysym {
    fn from(c: char) -> Keysym {
        Keysym::from_char(c)
    }
}

impl std::fmt::Display for Keysym {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_numbers_go_both_ways() {
        assert_eq!(Keysym::from_name("Return"), Some(Keysym(0xff0d)));
        assert_eq!(Keysym(0xff0d).name(), "Return");
        assert_eq!(Keysym::from_name("a"), Some(Keysym(0x61)));
        assert_eq!(Keysym::from_name("NoSymbol"), Some(Keysym::NO_SYMBOL));
        assert_eq!(Keysym::from_name("return"), None, "case matters");
        assert_eq!(Keysym::from_name(""), None);
        // An alias reads, and the value names itself canonically.
        assert_eq!(Keysym::from_name("dead_perispomeni"), Some(Keysym(0xfe53)));
        assert_eq!(Keysym(0xfe53).name(), "dead_tilde");
        // The table is sorted the way the lookups need.
        assert!(BY_NAME
            .windows(2)
            .all(|w| w[0].0.as_bytes() < w[1].0.as_bytes()));
        assert!(BY_VALUE.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(TO_UCS.windows(2).all(|w| w[0].0 < w[1].0));
        for (name, value) in BY_NAME.iter() {
            assert_eq!(Keysym::from_name(name), Some(Keysym(*value)), "{name}");
        }
        for (value, name) in BY_VALUE.iter() {
            assert_eq!(Keysym(*value).name(), *name);
        }
    }

    #[test]
    fn unnamed_keysyms_use_the_u_and_0x_forms() {
        assert_eq!(Keysym(0x0100_20b9).name(), "U20B9");
        assert_eq!(Keysym(0x0101_f600).name(), "U0001F600");
        assert_eq!(Keysym(0x1234_5678).name(), "0x12345678");
        assert_eq!(Keysym::from_name("U20B9"), Some(Keysym(0x0100_20b9)));
        assert_eq!(Keysym::from_name("U0001F600"), Some(Keysym(0x0101_f600)));
        assert_eq!(Keysym::from_name("U41"), Some(Keysym(0x41)));
        assert_eq!(Keysym::from_name("U1F"), None, "a control character");
        assert_eq!(Keysym::from_name("U110000"), None);
        assert_eq!(Keysym::from_name("U"), Some(Keysym(0x55)), "the letter");
        assert_eq!(Keysym::from_name("U+41"), None);
        assert_eq!(Keysym::from_name("Uxyz"), None);
        assert_eq!(Keysym::from_name("0x12345678"), Some(Keysym(0x1234_5678)));
        assert_eq!(Keysym::from_name("0xffffffff"), None, "past the largest");
        assert_eq!(Keysym::from_name("0x"), None);
        assert_eq!(
            Keysym::from_name("XF86_AudioMute"),
            Keysym::from_name("XF86AudioMute")
        );
        assert!(Keysym::from_name("XF86AudioMute").is_some());
    }

    #[test]
    fn keysyms_and_characters() {
        assert_eq!(Keysym(0x61).to_char(), Some('a'));
        assert_eq!(Keysym::from_name("eacute").unwrap().to_char(), Some('é'));
        assert_eq!(
            Keysym::from_name("Cyrillic_ya").unwrap().to_char(),
            Some('я')
        );
        assert_eq!(Keysym::from_name("EuroSign").unwrap().to_char(), Some('€'));
        assert_eq!(Keysym::from_name("Return").unwrap().to_char(), Some('\r'));
        assert_eq!(Keysym::from_name("KP_7").unwrap().to_char(), Some('7'));
        assert_eq!(Keysym::from_name("Shift_L").unwrap().to_char(), None);
        assert_eq!(Keysym(0x0101_f600).to_char(), Some('😀'));
        assert_eq!(Keysym(0x0100_d800).to_char(), None, "a surrogate");
        assert_eq!(Keysym::NO_SYMBOL.to_char(), None);

        assert_eq!(Keysym::from_char('a'), Keysym(0x61));
        assert_eq!(Keysym::from_char('é').name(), "eacute");
        assert_eq!(Keysym::from_char('я').name(), "Cyrillic_ya");
        assert_eq!(Keysym::from_char('\r').name(), "Return");
        assert_eq!(Keysym::from_char('\x7f').name(), "Delete");
        assert_eq!(Keysym::from_char('😀'), Keysym(0x0101_f600));
        assert_eq!(Keysym::from_char('\u{fffe}'), Keysym::NO_SYMBOL);
        assert_eq!(Keysym::from('x'), Keysym(0x78));
        assert_eq!(Keysym(0xff0d).to_string(), "Return");
    }

    #[test]
    fn case() {
        let ks = |n: &str| Keysym::from_name(n).unwrap();
        assert_eq!(ks("a").to_upper(), ks("A"));
        assert_eq!(ks("A").to_lower(), ks("a"));
        assert_eq!(ks("eacute").to_upper(), ks("Eacute"));
        assert_eq!(ks("Cyrillic_ya").to_upper(), ks("Cyrillic_YA"));
        assert_eq!(ks("Greek_alpha").to_upper(), ks("Greek_ALPHA"));
        assert_eq!(ks("ssharp").to_upper(), Keysym(0x0100_1e9e));
        assert_eq!(Keysym(0x0100_1e9e).to_lower(), Keysym(0x0100_00df));
        assert_eq!(Keysym(0x0100_0130).to_lower(), Keysym(0x0100_0069));
        assert_eq!(Keysym(0x0100_1f80).to_upper(), Keysym(0x0100_1f88));
        assert_eq!(
            Keysym(0x0100_fb00).to_upper(),
            Keysym(0x0100_fb00),
            "no single upper case"
        );
        assert_eq!(ks("1").to_upper(), ks("1"));
        assert_eq!(ks("Return").to_upper(), ks("Return"));
        assert_eq!(Keysym(0x0100_0430).to_upper(), Keysym(0x0100_0410));
        assert!(ks("a").is_lower() && !ks("a").is_upper());
        assert!(ks("A").is_upper() && !ks("A").is_lower());
        assert!(!ks("1").is_lower() && !ks("1").is_upper());
        assert!(ks("KP_1").is_keypad() && !ks("1").is_keypad());
    }
}
