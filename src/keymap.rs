//! A compiled keymap: what each key's levels hold and which modifiers pick
//! between them.

use crate::keysym::Keysym;
use crate::parse::{err, parse, Expr, Item, Pos, Section, Stmt};
use crate::Error;
use std::collections::HashMap;

/// A key's number in the keymap. On Wayland this is the evdev scancode
/// plus 8.
pub type Keycode = u32;

/// The real modifiers, which every keymap has at these indices.
const REAL_MODS: [&str; 8] = [
    "Shift", "Lock", "Control", "Mod1", "Mod2", "Mod3", "Mod4", "Mod5",
];
const REAL_MASK: u32 = 0xff;
/// A keymap has at most this many modifiers, real and virtual: a modifier
/// mask is 32 bits.
const MAX_MODS: usize = 32;
/// The most layouts a key may have (a layout mask is 32 bits).
const MAX_GROUPS: u32 = 32;
/// The most levels a key type may have.
const MAX_LEVELS: u32 = 2048;
/// The most levels all keys of a keymap may have together.
const MAX_LEVEL_SLOTS: usize = 1 << 20;
/// The most LEDs a keymap may have.
const MAX_LEDS: usize = 32;

/// Which part of the keyboard state a question is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    /// Held down.
    Depressed,
    /// Latched: applies to the next key only.
    Latched,
    /// Locked, as by Caps Lock.
    Locked,
    /// The three together: what a key press is read with.
    Effective,
}

pub(crate) const WHICH_DEPRESSED: u8 = 1;
pub(crate) const WHICH_LATCHED: u8 = 2;
pub(crate) const WHICH_LOCKED: u8 = 4;
pub(crate) const WHICH_EFFECTIVE: u8 = 8;

#[derive(Debug, Clone)]
struct Mod {
    name: String,
    /// For a virtual modifier, the real modifiers it stands for.
    mapping: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Entry {
    /// The modifiers as written, virtual ones included.
    mods: u32,
    /// The real modifiers they resolve to.
    pub(crate) mask: u32,
    pub(crate) level: u32,
    preserve_mods: u32,
    pub(crate) preserve: u32,
}

impl Entry {
    /// An entry on virtual modifiers that map to nothing can never match.
    pub(crate) fn active(&self) -> bool {
        self.mods == 0 || self.mask != 0
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct KeyType {
    name: String,
    mods: u32,
    pub(crate) mask: u32,
    pub(crate) entries: Vec<Entry>,
    num_levels: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutOfRange {
    Wrap,
    Clamp,
    Redirect(u32),
}

#[derive(Debug, Clone)]
pub(crate) struct Group {
    pub(crate) ty: u32,
    /// Each level's keysyms, as a range of the keymap's `syms`.
    levels: Vec<(u32, u32)>,
}

#[derive(Debug, Clone)]
pub(crate) struct Key {
    keycode: Keycode,
    name: String,
    pub(crate) groups: Vec<Group>,
    modmap: u32,
    vmodmap: u32,
    repeats: bool,
    pub(crate) out_of_range: OutOfRange,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Led {
    name: String,
    pub(crate) which_mods: u8,
    mods: u32,
    pub(crate) mask: u32,
    pub(crate) which_groups: u8,
    pub(crate) groups: u32,
}

/// A keymap: the keys of a keyboard, the keysyms on each of their levels,
/// and the modifiers that choose the level.
///
/// Read one from the text a compositor sends with [`Keymap::from_str`] or
/// [`Keymap::from_bytes`]; ask what a key means under the current modifiers
/// through a [`State`](crate::State).
#[derive(Debug, Clone)]
pub struct Keymap {
    mods: Vec<Mod>,
    pub(crate) types: Vec<KeyType>,
    /// Sorted by keycode.
    keys: Vec<Key>,
    aliases: Vec<(String, String)>,
    syms: Vec<Keysym>,
    pub(crate) leds: Vec<Option<Led>>,
    group_names: Vec<String>,
    num_groups: u32,
}

/// Bring `group` into `0..count` the way a key (or the keyboard) says to.
pub(crate) fn wrap_group(group: i64, count: u32, how: OutOfRange) -> Option<u32> {
    if count == 0 {
        return None;
    }
    if (0..i64::from(count)).contains(&group) {
        return Some(group as u32);
    }
    Some(match how {
        OutOfRange::Redirect(to) if to < count => to,
        OutOfRange::Redirect(_) => 0,
        OutOfRange::Clamp if group < 0 => 0,
        OutOfRange::Clamp => count - 1,
        OutOfRange::Wrap => group.rem_euclid(i64::from(count)) as u32,
    })
}

impl std::str::FromStr for Keymap {
    type Err = Error;

    /// Read a keymap from its text (the `xkb_keymap { ... };` a compositor
    /// sends, or `xkbcomp` writes).
    fn from_str(text: &str) -> Result<Keymap, Error> {
        Builder::default().build(text)
    }
}

impl Keymap {
    /// Read a keymap from the bytes of a `wl_keyboard.keymap` file: the text,
    /// with or without the NUL that ends it. Bytes that are not UTF-8 (only
    /// names can hold any) are replaced.
    pub fn from_bytes(bytes: &[u8]) -> Result<Keymap, Error> {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).parse()
    }

    pub(crate) fn key(&self, keycode: Keycode) -> Option<&Key> {
        self.keys
            .binary_search_by_key(&keycode, |k| k.keycode)
            .ok()
            .map(|i| &self.keys[i])
    }

    /// The smallest keycode of the keymap (0 for a keymap without keys).
    pub fn min_keycode(&self) -> Keycode {
        self.keys.first().map_or(0, |k| k.keycode)
    }

    /// The largest keycode of the keymap (0 for a keymap without keys).
    pub fn max_keycode(&self) -> Keycode {
        self.keys.last().map_or(0, |k| k.keycode)
    }

    /// Every keycode the keymap names, in increasing order.
    pub fn keycodes(&self) -> impl Iterator<Item = Keycode> + '_ {
        self.keys.iter().map(|k| k.keycode)
    }

    /// The name of a key (`"AE01"`, without the angle brackets).
    pub fn key_name(&self, keycode: Keycode) -> Option<&str> {
        self.key(keycode).map(|k| k.name.as_str())
    }

    /// The keycode of the key called `name`, which may be an alias.
    pub fn key_by_name(&self, name: &str) -> Option<Keycode> {
        let real = self
            .aliases
            .iter()
            .find(|(a, _)| a == name)
            .map_or(name, |(_, r)| r.as_str());
        self.keys.iter().find(|k| k.name == real).map(|k| k.keycode)
    }

    /// Whether holding the key repeats it. A key the keymap does not have
    /// does not repeat.
    pub fn key_repeats(&self, keycode: Keycode) -> bool {
        self.key(keycode).is_some_and(|k| k.repeats)
    }

    /// How many modifiers the keymap has: the eight real ones (`Shift`,
    /// `Lock`, `Control`, `Mod1` to `Mod5`) and its virtual ones after them.
    pub fn num_mods(&self) -> u32 {
        self.mods.len() as u32
    }

    /// The name of the modifier at `index`.
    pub fn mod_name(&self, index: u32) -> Option<&str> {
        self.mods.get(index as usize).map(|m| m.name.as_str())
    }

    /// The index of the modifier called `name`; its bit in a modifier mask
    /// is `1 << index`.
    pub fn mod_index(&self, name: &str) -> Option<u32> {
        self.mods
            .iter()
            .position(|m| m.name == name)
            .map(|i| i as u32)
    }

    /// The real modifiers the modifier at `index` stands for: its own bit
    /// for a real one, what the keymap maps it to for a virtual one (0 if
    /// nothing).
    pub fn mod_mask(&self, index: u32) -> u32 {
        match self.mods.get(index as usize) {
            Some(_) if index < 8 => 1 << index,
            Some(m) => m.mapping,
            None => 0,
        }
    }

    /// `mods` with every virtual modifier in it replaced by the real ones it
    /// stands for.
    pub(crate) fn effective(&self, mods: u32) -> u32 {
        let mut mask = mods & REAL_MASK;
        for (i, m) in self.mods.iter().enumerate().skip(8) {
            if mods & (1 << i) != 0 {
                mask |= m.mapping;
            }
        }
        mask
    }

    /// How many layouts (groups) the keymap has.
    pub fn num_layouts(&self) -> u32 {
        self.num_groups
    }

    /// The name of a layout (`"English (US)"`), if the keymap gives one.
    pub fn layout_name(&self, layout: u32) -> Option<&str> {
        self.group_names
            .get(layout as usize)
            .map(String::as_str)
            .filter(|n| !n.is_empty())
    }

    /// The index of the layout called `name`.
    pub fn layout_index(&self, name: &str) -> Option<u32> {
        if name.is_empty() {
            return None;
        }
        self.group_names
            .iter()
            .position(|n| n == name)
            .map(|i| i as u32)
    }

    /// How many layouts this key has (0 for a key without symbols).
    pub fn num_layouts_for_key(&self, keycode: Keycode) -> u32 {
        self.key(keycode).map_or(0, |k| k.groups.len() as u32)
    }

    /// How many levels the key has in `layout`. A layout past the key's own
    /// is brought into range the way the key says (wrapped, by default).
    pub fn num_levels_for_key(&self, keycode: Keycode, layout: u32) -> u32 {
        self.group(keycode, layout)
            .map_or(0, |g| g.levels.len() as u32)
    }

    fn group(&self, keycode: Keycode, layout: u32) -> Option<&Group> {
        let key = self.key(keycode)?;
        let g = wrap_group(i64::from(layout), key.groups.len() as u32, key.out_of_range)?;
        key.groups.get(g as usize)
    }

    /// The keysyms on one level of a key: usually one, none for an empty
    /// level, several where the keymap puts several on it.
    pub fn key_syms_by_level(&self, keycode: Keycode, layout: u32, level: u32) -> &[Keysym] {
        let Some(&(start, len)) = self
            .group(keycode, layout)
            .and_then(|g| g.levels.get(level as usize))
        else {
            return &[];
        };
        &self.syms[start as usize..(start + len) as usize]
    }

    /// How many LED slots the keymap has; some may be unused (no name).
    pub fn num_leds(&self) -> u32 {
        self.leds.len() as u32
    }

    /// The name of the LED at `index` (`"Caps Lock"`).
    pub fn led_name(&self, index: u32) -> Option<&str> {
        self.leds
            .get(index as usize)
            .and_then(|l| l.as_ref())
            .map(|l| l.name.as_str())
    }

    /// The index of the LED called `name`.
    pub fn led_index(&self, name: &str) -> Option<u32> {
        self.leds
            .iter()
            .position(|l| l.as_ref().is_some_and(|l| l.name == name))
            .map(|i| i as u32)
    }
}

// ---------------------------------------------------------------------------
// Compiling the statements.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Match {
    Exactly,
    All,
    NoneOf,
    AnyOf,
    AnyOrNone,
}

#[derive(Clone, Copy, Debug)]
struct Interp {
    /// `NO_SYMBOL` matches any keysym.
    sym: Keysym,
    matching: Match,
    mods: u32,
    vmod: Option<u32>,
    repeat: bool,
    level_one_only: bool,
}

#[derive(Default)]
struct GroupDraft {
    type_name: Option<String>,
    syms: Option<Vec<Vec<Keysym>>>,
    actions: bool,
}

struct KeyDraft {
    keycode: Keycode,
    groups: Vec<GroupDraft>,
    default_type: Option<String>,
    repeat: Option<bool>,
    vmodmap: Option<u32>,
    out_of_range: OutOfRange,
}

struct KeyExtra {
    explicit_actions: bool,
    explicit_vmodmap: bool,
    explicit_repeat: bool,
}

#[derive(Default)]
struct Builder {
    mods: Vec<Mod>,
    /// Keycode by key name.
    names: HashMap<String, Keycode>,
    aliases: Vec<(String, String)>,
    leds: Vec<Option<Led>>,
    types: Vec<KeyType>,
    interps: Vec<Interp>,
    group_names: Vec<String>,
}

fn word(e: &Expr<'_>) -> Option<String> {
    match e {
        Expr::Ident(s) => Some(s.to_ascii_lowercase()),
        _ => None,
    }
}

fn boolean(e: &Expr<'_>) -> Option<bool> {
    match e {
        Expr::Int(v) => Some(*v != 0),
        Expr::Ident(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" => Some(true),
            "false" | "no" | "off" => Some(false),
            _ => None,
        },
        Expr::Not(inner) => boolean(inner).map(|b| !b),
        _ => None,
    }
}

/// `Level3` or `3`: a level, from 1 in the text, from 0 here.
fn level(e: &Expr<'_>, pos: Pos) -> Result<u32, Error> {
    numbered(e, "level", MAX_LEVELS).map_or_else(|| err(pos, "expected a level"), Ok)
}

/// `Group2` or `2`: a layout, from 1 in the text, from 0 here.
fn group(e: &Expr<'_>, pos: Pos) -> Result<u32, Error> {
    numbered(e, "group", MAX_GROUPS).map_or_else(|| err(pos, "expected a group"), Ok)
}

fn numbered(e: &Expr<'_>, prefix: &str, max: u32) -> Option<u32> {
    let n: u64 = match e {
        Expr::Int(v) => *v,
        Expr::Ident(s) if s.len() > prefix.len() && s.is_char_boundary(prefix.len()) => {
            let (head, tail) = s.split_at(prefix.len());
            if !head.eq_ignore_ascii_case(prefix) || !tail.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            tail.parse().ok()?
        }
        _ => return None,
    };
    (1..=u64::from(max)).contains(&n).then(|| n as u32 - 1)
}

/// The name on the left of an item (`name`, or the `name` of `name[i]`),
/// in lower case, and its index if it has one.
fn target<'e, 'a>(lhs: &'e Expr<'a>) -> Option<(String, Option<&'e Expr<'a>>)> {
    match lhs {
        Expr::Ident(s) => Some((s.to_ascii_lowercase(), None)),
        Expr::Index(base, index) => match &**base {
            Expr::Ident(s) => Some((s.to_ascii_lowercase(), Some(&**index))),
            _ => None,
        },
        _ => None,
    }
}

/// A keysym as a keymap writes it: a name, a digit for the digit's key, or
/// a number. `None` for a name that is no keysym.
fn keysym(e: &Expr<'_>) -> Option<Keysym> {
    match e {
        Expr::Int(v) if *v < 10 => Some(Keysym(0x30 + *v as u32)),
        Expr::Int(v) if *v <= u64::from(Keysym::MAX.0) => Some(Keysym(*v as u32)),
        Expr::Ident(s) => {
            if s.eq_ignore_ascii_case("any") || s.eq_ignore_ascii_case("nosymbol") {
                Some(Keysym::NO_SYMBOL)
            } else if s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("voidsymbol") {
                Some(Keysym::VOID_SYMBOL)
            } else {
                Keysym::from_name(s)
            }
        }
        _ => None,
    }
}

/// The type a group gets when the keymap names none, from what is on its
/// first levels.
fn automatic_type(levels: &[Vec<Keysym>]) -> &'static str {
    let sym = |i: usize| {
        levels
            .get(i)
            .and_then(|l| l.first())
            .copied()
            .unwrap_or(Keysym::NO_SYMBOL)
    };
    let width = levels.len();
    if width <= 1 {
        return "ONE_LEVEL";
    }
    let alphabetic = sym(0).is_lower() && sym(1).is_upper();
    let keypad = sym(0).is_keypad() || sym(1).is_keypad();
    if width == 2 {
        return if alphabetic {
            "ALPHABETIC"
        } else if keypad {
            "KEYPAD"
        } else {
            "TWO_LEVEL"
        };
    }
    if alphabetic {
        if sym(2).is_lower() && sym(3).is_upper() {
            "FOUR_LEVEL_ALPHABETIC"
        } else {
            "FOUR_LEVEL_SEMIALPHABETIC"
        }
    } else if keypad {
        "FOUR_LEVEL_KEYPAD"
    } else {
        "FOUR_LEVEL"
    }
}

impl Builder {
    fn mod_index(&self, name: &str) -> Option<usize> {
        self.mods
            .iter()
            .position(|m| m.name == name)
            .or_else(|| REAL_MODS.iter().position(|m| m.eq_ignore_ascii_case(name)))
    }

    /// A modifier mask: names joined with `+` (and `-`, `!`), `all`, `none`,
    /// or a number. Virtual modifiers are allowed only where `virtual_ok`.
    fn mask(&self, e: &Expr<'_>, pos: Pos, virtual_ok: bool) -> Result<u32, Error> {
        let all = if virtual_ok {
            ((1u64 << self.mods.len()) - 1) as u32
        } else {
            REAL_MASK
        };
        Ok(match e {
            Expr::Int(v) => *v as u32 & all,
            Expr::Ident(s) if s.eq_ignore_ascii_case("all") => REAL_MASK,
            Expr::Ident(s) if s.eq_ignore_ascii_case("none") => 0,
            Expr::Ident(s) => match self.mod_index(s) {
                Some(i) if i < 8 || virtual_ok => 1 << i,
                _ => {
                    return err(
                        pos,
                        format!("`{s}` is not a modifier that can be used here"),
                    )
                }
            },
            Expr::Bin(b'+', a, b) => {
                self.mask(a, pos, virtual_ok)? | self.mask(b, pos, virtual_ok)?
            }
            Expr::Bin(b'-', a, b) => {
                self.mask(a, pos, virtual_ok)? & !self.mask(b, pos, virtual_ok)?
            }
            Expr::Not(a) => !self.mask(a, pos, virtual_ok)? & all,
            _ => return err(pos, "expected a modifier mask"),
        })
    }

    fn add_vmods(&mut self, names: &[(&str, Option<Expr<'_>>)], pos: Pos) -> Result<(), Error> {
        for (name, value) in names {
            let mapping = match value {
                Some(e) => Some(self.mask(e, pos, false)?),
                None => None,
            };
            let at = match self.mods.iter().position(|m| m.name == *name) {
                Some(i) if i < 8 => {
                    return err(
                        pos,
                        format!("`{name}` is a real modifier, not a virtual one"),
                    )
                }
                Some(i) => i,
                None => {
                    if self.mods.len() >= MAX_MODS {
                        return err(pos, "too many virtual modifiers");
                    }
                    self.mods.push(Mod {
                        name: name.to_string(),
                        mapping: 0,
                    });
                    self.mods.len() - 1
                }
            };
            if let Some(mapping) = mapping {
                self.mods[at].mapping = mapping;
            }
        }
        Ok(())
    }

    fn keycodes(&mut self, stmts: &[(Pos, Stmt<'_>)]) -> Result<(), Error> {
        let mut aliases: Vec<(String, String)> = Vec::new();
        for (pos, stmt) in stmts {
            match stmt {
                Stmt::Item(Item {
                    lhs: Some(Expr::Key(name)),
                    value,
                    ..
                }) => {
                    let Expr::Int(code) = value else {
                        return err(*pos, "expected a keycode");
                    };
                    if *code > 0xffff_fffe {
                        return err(*pos, "keycode out of range");
                    }
                    // A later definition wins, by name and by number.
                    self.names.retain(|_, c| u64::from(*c) != *code);
                    self.names.insert(name.to_string(), *code as Keycode);
                }
                Stmt::Alias(alias, real) => {
                    aliases.retain(|(a, _)| a != alias);
                    aliases.push((alias.to_string(), real.to_string()));
                }
                Stmt::LedName(index, name) => {
                    if !(1..=MAX_LEDS as u64).contains(index) {
                        return err(*pos, "indicator number out of range");
                    }
                    let i = *index as usize - 1;
                    for led in self.leds.iter_mut() {
                        if led.as_ref().is_some_and(|l| l.name == *name) {
                            *led = None;
                        }
                    }
                    if self.leds.len() <= i {
                        self.leds.resize(i + 1, None);
                    }
                    self.leds[i] = Some(Led {
                        name: name.clone(),
                        ..Led::default()
                    });
                }
                _ => {}
            }
        }
        // An alias stands only for a key that exists, and never hides one.
        self.aliases = aliases
            .into_iter()
            .filter(|(a, r)| !self.names.contains_key(a) && self.names.contains_key(r))
            .collect();
        Ok(())
    }

    fn types(&mut self, stmts: &[(Pos, Stmt<'_>)]) -> Result<(), Error> {
        for (pos, stmt) in stmts {
            match stmt {
                Stmt::VMods(names) => self.add_vmods(names, *pos)?,
                Stmt::Type(name, body) => {
                    let ty = self.key_type(name, body)?;
                    match self.types.iter_mut().find(|t| t.name == *name) {
                        Some(old) => *old = ty,
                        None => self.types.push(ty),
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn key_type(&self, name: &str, body: &[Item<'_>]) -> Result<KeyType, Error> {
        let mut ty = KeyType {
            name: name.to_string(),
            num_levels: 1,
            ..KeyType::default()
        };
        for item in body {
            let pos = item.pos;
            let Some((field, index)) = item.lhs.as_ref().and_then(target) else {
                continue;
            };
            match (field.as_str(), index) {
                ("modifiers", None) => ty.mods = self.mask(&item.value, pos, true)?,
                ("map", Some(index)) => {
                    // Modifiers the type does not use cannot pick a level.
                    let mods = self.mask(index, pos, true)? & ty.mods;
                    let level = level(&item.value, pos)?;
                    ty.num_levels = ty.num_levels.max(level + 1);
                    match ty.entries.iter_mut().find(|e| e.mods == mods) {
                        Some(e) => e.level = level,
                        None => ty.entries.push(Entry {
                            mods,
                            level,
                            ..Entry::default()
                        }),
                    }
                }
                ("preserve", Some(index)) => {
                    let mods = self.mask(index, pos, true)? & ty.mods;
                    let preserve = self.mask(&item.value, pos, true)? & mods;
                    match ty.entries.iter_mut().find(|e| e.mods == mods) {
                        Some(e) => e.preserve_mods = preserve,
                        None => ty.entries.push(Entry {
                            mods,
                            preserve_mods: preserve,
                            ..Entry::default()
                        }),
                    }
                }
                ("level_name" | "levelname", Some(index)) => {
                    ty.num_levels = ty.num_levels.max(level(index, pos)? + 1);
                }
                _ => {}
            }
        }
        Ok(ty)
    }

    fn compat(&mut self, stmts: &[(Pos, Stmt<'_>)]) -> Result<(), Error> {
        let mut default = Interp {
            sym: Keysym::NO_SYMBOL,
            matching: Match::AnyOrNone,
            mods: REAL_MASK,
            vmod: None,
            repeat: false,
            level_one_only: false,
        };
        let mut found: Vec<Interp> = Vec::new();
        for (pos, stmt) in stmts {
            match stmt {
                Stmt::VMods(names) => self.add_vmods(names, *pos)?,
                Stmt::Item(Item {
                    lhs: Some(Expr::Field(scope, field)),
                    value,
                    ..
                }) if scope.eq_ignore_ascii_case("interpret") => {
                    self.interp_field(&mut default, field, value, *pos)?;
                }
                Stmt::Interpret(sym, matching, body) => {
                    // An interpret for a keysym this library does not know
                    // can match no key.
                    let Some(sym) = keysym(sym) else { continue };
                    let (matching, mods) = match matching {
                        None => (Match::AnyOrNone, REAL_MASK),
                        Some(Expr::Call(name, args)) => {
                            let m = match name.to_ascii_lowercase().as_str() {
                                "noneof" => Match::NoneOf,
                                "anyofornone" => Match::AnyOrNone,
                                "anyof" => Match::AnyOf,
                                "allof" => Match::All,
                                "exactly" => Match::Exactly,
                                _ => return err(*pos, "unknown interpret match"),
                            };
                            let mods = match args.first() {
                                Some(e) => self.mask(e, *pos, false)?,
                                None => 0,
                            };
                            (m, mods)
                        }
                        Some(Expr::Ident(s)) if s.eq_ignore_ascii_case("any") => {
                            (Match::AnyOf, REAL_MASK)
                        }
                        Some(e) => (Match::Exactly, self.mask(e, *pos, false)?),
                    };
                    let mut interp = Interp {
                        sym,
                        matching,
                        mods,
                        ..default
                    };
                    for item in body {
                        if let Some(Expr::Ident(field)) = &item.lhs {
                            self.interp_field(&mut interp, field, &item.value, item.pos)?;
                        }
                    }
                    // The same keysym and match again replaces the earlier one.
                    match found
                        .iter_mut()
                        .find(|i| i.sym == sym && i.matching == matching && i.mods == mods)
                    {
                        Some(old) => *old = interp,
                        None => found.push(interp),
                    }
                }
                Stmt::LedMap(name, body) => self.led_map(name, body)?,
                _ => {}
            }
        }
        // The order they are tried in: a named keysym before "any keysym",
        // and within each the most specific match first.
        let order = [
            Match::Exactly,
            Match::All,
            Match::NoneOf,
            Match::AnyOf,
            Match::AnyOrNone,
        ];
        for named in [true, false] {
            for m in order {
                self.interps.extend(
                    found
                        .iter()
                        .filter(|i| (i.sym != Keysym::NO_SYMBOL) == named && i.matching == m),
                );
            }
        }
        Ok(())
    }

    fn interp_field(
        &self,
        interp: &mut Interp,
        field: &str,
        value: &Expr<'_>,
        pos: Pos,
    ) -> Result<(), Error> {
        match field.to_ascii_lowercase().as_str() {
            "virtualmodifier" | "virtualmod" => {
                let Expr::Ident(name) = value else {
                    return err(pos, "expected a virtual modifier");
                };
                match self.mods.iter().position(|m| m.name == *name) {
                    Some(i) if i >= 8 => interp.vmod = Some(i as u32),
                    _ => return err(pos, format!("`{name}` is not a virtual modifier")),
                }
            }
            "repeat" => {
                interp.repeat =
                    boolean(value).map_or_else(|| err(pos, "expected yes or no"), Ok)?;
            }
            "usemodmapmods" | "usemodmap" => {
                interp.level_one_only = match word(value).as_deref() {
                    Some("level1" | "levelone") => true,
                    Some("anylevel" | "any") => false,
                    _ => return err(pos, "expected level1 or anylevel"),
                };
            }
            _ => {}
        }
        Ok(())
    }

    fn led_map(&mut self, name: &str, body: &[Item<'_>]) -> Result<(), Error> {
        let mut led = Led {
            name: name.to_string(),
            ..Led::default()
        };
        let (mut which_mods, mut which_groups) = (None, None);
        for item in body {
            let pos = item.pos;
            let Some(Expr::Ident(field)) = &item.lhs else {
                continue;
            };
            match field.to_ascii_lowercase().as_str() {
                "modifiers" | "mods" => led.mods = self.mask(&item.value, pos, true)?,
                "groups" => led.groups = group_mask(&item.value, pos)?,
                "whichmodstate" | "whichmodifierstate" => {
                    which_mods = Some(which(&item.value, pos)?)
                }
                "whichgroupstate" => which_groups = Some(which(&item.value, pos)?),
                _ => {}
            }
        }
        // Mods or groups without a "which" mean the effective ones.
        led.which_mods = which_mods.unwrap_or(if led.mods != 0 { WHICH_EFFECTIVE } else { 0 });
        led.which_groups =
            which_groups.unwrap_or(if led.groups != 0 { WHICH_EFFECTIVE } else { 0 });
        // The LED of that name from the keycodes section, else a free slot.
        let slot = self
            .leds
            .iter()
            .position(|l| l.as_ref().is_some_and(|l| l.name == name))
            .or_else(|| self.leds.iter().position(Option::is_none));
        match slot {
            Some(i) => self.leds[i] = Some(led),
            None if self.leds.len() < MAX_LEDS => self.leds.push(Some(led)),
            None => {}
        }
        Ok(())
    }

    fn key_draft(&self, keycode: Keycode, body: &[Item<'_>]) -> Result<KeyDraft, Error> {
        let mut key = KeyDraft {
            keycode,
            groups: Vec::new(),
            default_type: None,
            repeat: None,
            vmodmap: None,
            out_of_range: OutOfRange::Wrap,
        };
        fn slot(groups: &mut Vec<GroupDraft>, i: u32) -> &mut GroupDraft {
            if groups.len() <= i as usize {
                groups.resize_with(i as usize + 1, GroupDraft::default);
            }
            &mut groups[i as usize]
        }
        for item in body {
            let pos = item.pos;
            let (field, index) = match &item.lhs {
                Some(lhs) => match target(lhs) {
                    Some(t) => t,
                    None => continue,
                },
                // A bare list is the next group's symbols (or actions); a
                // bare name is that field set, `!name` that field cleared.
                None => match &item.value {
                    Expr::List(list) if is_actions(list) => ("actions".to_string(), None),
                    Expr::List(_) => ("symbols".to_string(), None),
                    Expr::Ident(s) => (s.to_ascii_lowercase(), None),
                    Expr::Not(inner) => match &**inner {
                        Expr::Ident(s) => (s.to_ascii_lowercase(), None),
                        _ => continue,
                    },
                    _ => continue,
                },
            };
            let flag = || match (&item.lhs, &item.value) {
                (None, Expr::Ident(_)) => Some(true),
                (None, Expr::Not(_)) => Some(false),
                (_, v) => boolean(v),
            };
            match field.as_str() {
                "type" => {
                    let Expr::Str(name) = &item.value else {
                        return err(pos, "expected a type name");
                    };
                    match index {
                        Some(g) => {
                            slot(&mut key.groups, group(g, pos)?).type_name = Some(name.clone())
                        }
                        None => key.default_type = Some(name.clone()),
                    }
                }
                "symbols" => {
                    let Expr::List(list) = &item.value else {
                        return err(pos, "expected a list of keysyms");
                    };
                    let g = match index {
                        Some(g) => group(g, pos)?,
                        None => next_free(&key.groups, |g| g.syms.is_some(), pos)?,
                    };
                    if list.len() > MAX_LEVELS as usize {
                        return err(pos, "too many levels");
                    }
                    let levels = list
                        .iter()
                        .map(|e| match e {
                            Expr::Set(set) => set
                                .iter()
                                .map(|s| keysym(s).unwrap_or(Keysym::NO_SYMBOL))
                                .collect(),
                            // A level holding only "no symbol" is empty; a
                            // name that is no keysym counts as none.
                            other => match keysym(other).unwrap_or(Keysym::NO_SYMBOL) {
                                Keysym::NO_SYMBOL => Vec::new(),
                                sym => vec![sym],
                            },
                        })
                        .collect();
                    slot(&mut key.groups, g).syms = Some(levels);
                }
                "actions" => {
                    let g = match index {
                        Some(g) => group(g, pos)?,
                        None => next_free(&key.groups, |g| g.actions, pos)?,
                    };
                    slot(&mut key.groups, g).actions = true;
                }
                "repeat" | "repeats" | "repeating" => {
                    key.repeat = match word(&item.value).as_deref() {
                        Some("default") => None,
                        _ => Some(flag().map_or_else(|| err(pos, "expected yes or no"), Ok)?),
                    };
                }
                "virtualmods" | "vmods" | "virtualmodifiers" => {
                    key.vmodmap = Some(self.mask(&item.value, pos, true)? & !REAL_MASK);
                }
                "groupswrap" | "wrapgroups" => {
                    if flag() == Some(true) {
                        key.out_of_range = OutOfRange::Wrap;
                    }
                }
                "groupsclamp" | "clampgroups" => {
                    key.out_of_range = if flag() == Some(true) {
                        OutOfRange::Clamp
                    } else {
                        OutOfRange::Wrap
                    };
                }
                "groupsredirect" | "redirectgroups" => {
                    key.out_of_range = OutOfRange::Redirect(group(&item.value, pos)?);
                }
                _ => {}
            }
        }
        Ok(key)
    }

    fn build(mut self, text: &str) -> Result<Keymap, Error> {
        let file = parse(text)?;
        self.mods = REAL_MODS
            .iter()
            .map(|n| Mod {
                name: n.to_string(),
                mapping: 0,
            })
            .collect();
        let section = |s: Section, name: &str| {
            file.sections
                .iter()
                .find(|(k, _)| *k == s)
                .map(|(_, stmts)| stmts.as_slice())
                .map_or_else(
                    || {
                        err(
                            Pos { line: 1, col: 1 },
                            format!("the keymap has no {name} section"),
                        )
                    },
                    Ok,
                )
        };
        let keycodes = section(Section::Keycodes, "xkb_keycodes")?;
        let types = section(Section::Types, "xkb_types")?;
        let compat = section(Section::Compat, "xkb_compatibility")?;
        let symbols = section(Section::Symbols, "xkb_symbols")?;
        self.keycodes(keycodes)?;
        self.types(types)?;
        self.compat(compat)?;
        if self.types.is_empty() {
            self.types.push(KeyType {
                name: "default".into(),
                num_levels: 1,
                ..KeyType::default()
            });
        }

        // Symbols: the keys, then what needs all of them (the modifier map).
        let mut drafts: HashMap<Keycode, KeyDraft> = HashMap::new();
        for (pos, stmt) in symbols {
            match stmt {
                Stmt::VMods(names) => self.add_vmods(names, *pos)?,
                Stmt::Item(Item {
                    lhs: Some(lhs),
                    value: Expr::Str(name),
                    ..
                }) => {
                    if let Some((field, Some(index))) = target(lhs) {
                        if field == "name" || field == "groupname" {
                            let g = group(index, *pos)? as usize;
                            if self.group_names.len() <= g {
                                self.group_names.resize(g + 1, String::new());
                            }
                            self.group_names[g] = name.clone();
                        }
                    }
                }
                Stmt::Key(name, body) => {
                    // A key the keycodes section does not have is left out.
                    if let Some(keycode) = self.keycode(name) {
                        drafts.insert(keycode, self.key_draft(keycode, body)?);
                    }
                }
                _ => {}
            }
        }

        let mut syms: Vec<Keysym> = Vec::new();
        let mut slots = 0usize;
        let mut keys: Vec<Key> = Vec::with_capacity(self.names.len());
        let mut extras: Vec<KeyExtra> = Vec::with_capacity(self.names.len());
        let mut named: Vec<(&String, &Keycode)> = self.names.iter().collect();
        named.sort_by_key(|(_, c)| **c);
        for (name, &keycode) in named {
            let mut key = Key {
                keycode,
                name: name.clone(),
                groups: Vec::new(),
                modmap: 0,
                vmodmap: 0,
                repeats: false,
                out_of_range: OutOfRange::Wrap,
            };
            let mut extra = KeyExtra {
                explicit_actions: false,
                explicit_vmodmap: false,
                explicit_repeat: false,
            };
            if let Some(mut draft) = drafts.remove(&keycode) {
                debug_assert_eq!(draft.keycode, keycode);
                key.out_of_range = draft.out_of_range;
                if let Some(v) = draft.vmodmap {
                    key.vmodmap = v;
                    extra.explicit_vmodmap = true;
                }
                if let Some(r) = draft.repeat {
                    key.repeats = r;
                    extra.explicit_repeat = true;
                }
                extra.explicit_actions = draft.groups.iter().any(|g| g.actions);
                // The groups the key needs: up to the last with symbols or
                // a type of its own. A gap below it copies the first group.
                let count = draft
                    .groups
                    .iter()
                    .rposition(|g| {
                        g.syms.as_ref().is_some_and(|s| !s.is_empty()) || g.type_name.is_some()
                    })
                    .map_or(0, |i| i + 1);
                draft.groups.truncate(count);
                for g in 0..count {
                    let draft_group = &draft.groups[g];
                    let empty = draft_group.syms.is_none()
                        && draft_group.type_name.is_none()
                        && !draft_group.actions;
                    let source = if empty { &draft.groups[0] } else { draft_group };
                    let mut levels: Vec<Vec<Keysym>> = source.syms.clone().unwrap_or_default();
                    let type_name = source
                        .type_name
                        .as_deref()
                        .or(draft.default_type.as_deref())
                        .unwrap_or_else(|| automatic_type(&levels));
                    // A type the keymap does not define falls back to its
                    // first one.
                    let ty = self
                        .types
                        .iter()
                        .position(|t| t.name == type_name)
                        .unwrap_or(0);
                    levels.resize(self.types[ty].num_levels as usize, Vec::new());
                    // Text from another process must not be able to ask
                    // for memory out of all proportion to its size.
                    slots += levels.len();
                    if slots > MAX_LEVEL_SLOTS {
                        return err(Pos { line: 1, col: 1 }, "the keymap is too large");
                    }
                    let levels = levels
                        .into_iter()
                        .map(|l| {
                            let start = syms.len() as u32;
                            syms.extend_from_slice(&l);
                            (start, l.len() as u32)
                        })
                        .collect();
                    key.groups.push(Group {
                        ty: ty as u32,
                        levels,
                    });
                }
            }
            keys.push(key);
            extras.push(extra);
        }

        // The modifier map: a key (or the key holding a keysym) to a real
        // modifier; the last word on a key wins.
        let mut modmap: HashMap<usize, u32> = HashMap::new();
        for (pos, stmt) in symbols {
            let Stmt::ModMap(name, entries) = stmt else {
                continue;
            };
            if name.eq_ignore_ascii_case("none") {
                continue;
            }
            let index = match self.mod_index(name) {
                Some(i) if i < 8 => i as u32,
                _ => return err(*pos, format!("`{name}` is not a real modifier")),
            };
            for entry in entries {
                let at = match entry {
                    Expr::Key(k) => self
                        .keycode(k)
                        .and_then(|c| keys.binary_search_by_key(&c, |k| k.keycode).ok()),
                    other => keysym(other)
                        .filter(|s| *s != Keysym::NO_SYMBOL)
                        .and_then(|s| key_holding(&keys, &syms, s)),
                };
                if let Some(at) = at {
                    modmap.insert(at, index);
                }
            }
        }
        for (at, index) in modmap {
            keys[at].modmap = 1 << index;
        }

        // What the compatibility section's interprets give each key: its
        // virtual modifiers and whether it repeats.
        for (key, extra) in keys.iter_mut().zip(&extras) {
            if extra.explicit_actions {
                continue;
            }
            let mut vmodmap = 0u32;
            let mut repeats = false;
            for (g, grp) in key.groups.iter().enumerate() {
                for (l, &(start, len)) in grp.levels.iter().enumerate() {
                    let level_syms = &syms[start as usize..(start + len) as usize];
                    let Some(interp) = self.interp_for(level_syms, l, key.modmap) else {
                        continue;
                    };
                    // The key's own behaviour comes from its base level.
                    if g == 0 && l == 0 {
                        repeats = interp.repeat;
                    }
                    if (g == 0 && l == 0) || !interp.level_one_only {
                        if let Some(v) = interp.vmod {
                            vmodmap |= 1 << v;
                        }
                    }
                }
            }
            if !extra.explicit_repeat {
                key.repeats = repeats;
            }
            if !extra.explicit_vmodmap {
                key.vmodmap = vmodmap;
            }
        }
        for key in &keys {
            for (i, m) in self.mods.iter_mut().enumerate().skip(8) {
                if key.vmodmap & (1 << i) != 0 {
                    m.mapping |= key.modmap;
                }
            }
        }

        let num_groups = keys
            .iter()
            .map(|k| k.groups.len() as u32)
            .max()
            .unwrap_or(0);
        let mut keymap = Keymap {
            mods: self.mods,
            types: self.types,
            keys,
            aliases: self.aliases,
            syms,
            leds: self.leds,
            group_names: self.group_names,
            num_groups,
        };
        // With the virtual modifiers mapped, every mask can be made real.
        let mut types = std::mem::take(&mut keymap.types);
        for ty in &mut types {
            ty.mask = keymap.effective(ty.mods);
            for e in &mut ty.entries {
                e.mask = keymap.effective(e.mods);
                e.preserve = keymap.effective(e.preserve_mods);
            }
        }
        keymap.types = types;
        let mut leds = std::mem::take(&mut keymap.leds);
        for led in leds.iter_mut().flatten() {
            led.mask = keymap.effective(led.mods);
        }
        keymap.leds = leds;
        Ok(keymap)
    }

    fn keycode(&self, name: &str) -> Option<Keycode> {
        self.names.get(name).copied().or_else(|| {
            let (_, real) = self.aliases.iter().find(|(a, _)| a == name)?;
            self.names.get(real).copied()
        })
    }

    /// The first interpret that matches a level's keysyms on a key with
    /// this modifier map; for a level no interpret matches, the default one
    /// (an ordinary key: it repeats). `None` for an empty level.
    fn interp_for(&self, syms: &[Keysym], level: usize, modmap: u32) -> Option<Interp> {
        if syms.is_empty() {
            return None;
        }
        let found = self.interps.iter().find(|i| {
            if i.sym != Keysym::NO_SYMBOL && (syms.len() > 1 || i.sym != syms[0]) {
                return false;
            }
            let mods = if i.level_one_only && level != 0 {
                0
            } else {
                modmap
            };
            match i.matching {
                Match::NoneOf => i.mods & mods == 0,
                Match::AnyOrNone => mods == 0 || i.mods & mods != 0,
                Match::AnyOf => i.mods & mods != 0,
                Match::All => i.mods & mods == i.mods,
                Match::Exactly => i.mods == mods,
            }
        });
        Some(found.copied().unwrap_or(Interp {
            sym: Keysym::NO_SYMBOL,
            matching: Match::AnyOrNone,
            mods: 0,
            vmod: None,
            repeat: true,
            level_one_only: false,
        }))
    }
}

/// Whether a bare list in a key is its actions rather than its keysyms.
fn is_actions(list: &[Expr<'_>]) -> bool {
    list.iter().any(|e| match e {
        Expr::Call(..) => true,
        Expr::Set(set) => set.iter().any(|e| matches!(e, Expr::Call(..))),
        _ => false,
    })
}

/// The first group a bare list fills: the first without what `has` tests.
fn next_free(
    groups: &[GroupDraft],
    has: impl Fn(&GroupDraft) -> bool,
    pos: Pos,
) -> Result<u32, Error> {
    let i = groups.iter().position(|g| !has(g)).unwrap_or(groups.len());
    if i as u32 >= MAX_GROUPS {
        return err(pos, "too many groups");
    }
    Ok(i as u32)
}

/// The key a modifier map means by a keysym: the one holding it on the
/// lowest group, then level, then keycode.
fn key_holding(keys: &[Key], syms: &[Keysym], sym: Keysym) -> Option<usize> {
    let mut best: Option<(usize, usize, usize)> = None;
    for (at, key) in keys.iter().enumerate() {
        for (g, grp) in key.groups.iter().enumerate() {
            for (l, &(start, len)) in grp.levels.iter().enumerate() {
                if syms[start as usize..(start + len) as usize] == [sym]
                    && best.map_or(true, |b| (g, l, at) < b)
                {
                    best = Some((g, l, at));
                }
            }
        }
    }
    best.map(|(_, _, at)| at)
}

/// An LED's `groups`: a number, or `Group2`, `all`, `none`, joined with
/// `+`, `-`, `!`.
fn group_mask(e: &Expr<'_>, pos: Pos) -> Result<u32, Error> {
    Ok(match e {
        Expr::Int(v) => *v as u32,
        Expr::Ident(s) if s.eq_ignore_ascii_case("all") => 0xff,
        Expr::Ident(s) if s.eq_ignore_ascii_case("none") => 0,
        Expr::Ident(_) => 1 << group(e, pos)?,
        Expr::Bin(b'+', a, b) => group_mask(a, pos)? | group_mask(b, pos)?,
        Expr::Bin(b'-', a, b) => group_mask(a, pos)? & !group_mask(b, pos)?,
        Expr::Not(a) => !group_mask(a, pos)?,
        _ => return err(pos, "expected a group mask"),
    })
}

/// An LED's `whichModState` / `whichGroupState`.
fn which(e: &Expr<'_>, pos: Pos) -> Result<u8, Error> {
    Ok(match e {
        Expr::Int(v) => *v as u8 & 0xf,
        Expr::Ident(s) => match s.to_ascii_lowercase().as_str() {
            "base" | "depressed" => WHICH_DEPRESSED,
            "latched" => WHICH_LATCHED,
            "locked" => WHICH_LOCKED,
            "effective" | "compat" => WHICH_EFFECTIVE,
            "any" => WHICH_DEPRESSED | WHICH_LATCHED | WHICH_LOCKED | WHICH_EFFECTIVE,
            "none" => 0,
            _ => return err(pos, "expected a state component"),
        },
        Expr::Bin(b'+', a, b) => which(a, pos)? | which(b, pos)?,
        Expr::Bin(b'-', a, b) => which(a, pos)? & !which(b, pos)?,
        Expr::Not(a) => !which(a, pos)? & 0xf,
        _ => return err(pos, "expected a state component"),
    })
}
