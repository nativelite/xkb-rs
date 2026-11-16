//! A compiled keymap: what each key's levels hold and which modifiers pick
//! between them.

use crate::keysym::Keysym;
use crate::parse::{err, parse, Expr, Item, Merge, Placed, Pos, Section, Stmt};
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
    /// The smallest and largest keycode the keymap names, counting names a
    /// later statement took away again. `None` for a keymap without keys.
    range: Option<(Keycode, Keycode)>,
    aliases: Vec<(String, String)>,
    syms: Vec<Keysym>,
    pub(crate) leds: Vec<Option<Led>>,
    group_names: Vec<Option<String>>,
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

    /// The smallest keycode of the keymap (8 for a keymap without keys, as
    /// libxkbcommon answers).
    pub fn min_keycode(&self) -> Keycode {
        self.range.map_or(8, |(low, _)| low)
    }

    /// The largest keycode of the keymap (255 for a keymap without keys).
    pub fn max_keycode(&self) -> Keycode {
        self.range.map_or(255, |(_, high)| high)
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
        self.group_names.get(layout as usize)?.as_deref()
    }

    /// The index of the layout called `name`.
    pub fn layout_index(&self, name: &str) -> Option<u32> {
        self.group_names
            .iter()
            .position(|n| n.as_deref() == Some(name))
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
//
// Where a keymap says the same thing twice, or says something odd, this
// follows what libxkbcommon does with the same text; the notes below name
// the cases. One rule runs through the bodies of keys, types and
// interprets: a field that makes no sense is passed over, as libxkbcommon
// passes over it wherever another field follows.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Match {
    Exactly,
    All,
    NoneOf,
    AnyOf,
    AnyOrNone,
}

const INTERP_VMOD: u8 = 1;
const INTERP_REPEAT: u8 = 2;
const INTERP_LEVEL_ONE: u8 = 4;

#[derive(Clone, Copy, Debug)]
struct Interp {
    /// `NO_SYMBOL` matches any keysym.
    sym: Keysym,
    matching: Match,
    mods: u32,
    vmod: Option<u32>,
    repeat: bool,
    level_one_only: bool,
    /// Which of the three fields the keymap set (`INTERP_*`): what decides
    /// who wins when the same interpret is given twice.
    defined: u8,
}

/// One group of a key as its statements describe it.
#[derive(Clone, Debug, Default)]
struct GroupDraft {
    type_name: Option<String>,
    /// The keysyms of each level. A level may be there for an action alone;
    /// its keysyms are then empty.
    levels: Vec<Vec<Keysym>>,
    /// The group was given symbols (even an empty list).
    syms: bool,
    /// The group was given actions (even an empty list).
    actions: bool,
}

impl GroupDraft {
    /// Whether the keymap says anything about the group: such a group
    /// counts toward how many the key has.
    fn defined(&self) -> bool {
        self.syms || self.actions || self.type_name.is_some()
    }
}

/// A key as its statements describe it. `None` is "not said".
#[derive(Clone, Debug, Default)]
struct KeyDraft {
    groups: Vec<GroupDraft>,
    default_type: Option<String>,
    /// `Some(None)` is `repeat = default`.
    repeat: Option<Option<bool>>,
    vmodmap: Option<u32>,
    out_of_range: Option<OutOfRange>,
}

/// An LED's map as its statements describe it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LedDraft {
    mods: u32,
    which_mods: u8,
    groups: u32,
    which_groups: u8,
    has_mods: bool,
    has_groups: bool,
}

/// What a modifier map entry names: a key, or the key holding a keysym.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Mapped {
    Key(String),
    Sym(Keysym),
}

#[derive(Default)]
struct Builder {
    mods: Vec<Mod>,
    /// Keycode by key name, and the name of each keycode.
    names: HashMap<String, Keycode>,
    codes: HashMap<Keycode, String>,
    /// The smallest and largest keycode any statement named.
    range: Option<(Keycode, Keycode)>,
    aliases: HashMap<String, String>,
    leds: Vec<Option<Led>>,
    types: Vec<KeyType>,
    type_index: HashMap<String, usize>,
    interps: Vec<Interp>,
    group_names: Vec<Option<String>>,
}

static TRUE: Expr<'static> = Expr::Ident("true");
static FALSE: Expr<'static> = Expr::Ident("false");

fn word(e: &Expr<'_>) -> Option<String> {
    match e {
        Expr::Ident(s) => Some(s.to_ascii_lowercase()),
        _ => None,
    }
}

fn boolean(e: &Expr<'_>) -> Option<bool> {
    match e {
        Expr::Ident(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" => Some(true),
            "false" | "no" | "off" => Some(false),
            _ => None,
        },
        Expr::Not(inner) | Expr::Invert(inner) => boolean(inner).map(|b| !b),
        _ => None,
    }
}

/// An item of a body as a field, its index and its value: `field = value`,
/// `field[index] = value`, a bare `field` (set) or `!field` (cleared).
fn item_field<'e, 'a>(item: &'e Item<'a>) -> Option<(String, Option<&'e Expr<'a>>, &'e Expr<'a>)>
where
    'a: 'e,
{
    match &item.lhs {
        Some(lhs) => target(lhs).map(|(name, index)| (name, index, &item.value)),
        None => match &item.value {
            Expr::Ident(s) => Some((s.to_ascii_lowercase(), None, &TRUE)),
            Expr::Not(inner) => match &**inner {
                Expr::Ident(s) => Some((s.to_ascii_lowercase(), None, &FALSE)),
                _ => None,
            },
            _ => None,
        },
    }
}

/// A whole number written as an expression: numbers and the names `names`
/// knows, with `+ - * /` (the last two binding tighter) and a leading `-`.
fn int(e: &Expr<'_>, names: &dyn Fn(&str) -> Option<i64>) -> Option<i64> {
    match e {
        Expr::Int(v) => i64::try_from(*v).ok(),
        Expr::Ident(s) => names(s),
        Expr::Neg(a) => int(a, names)?.checked_neg(),
        Expr::Invert(a) => Some(!int(a, names)?),
        Expr::Ops(first, rest) => {
            let mut sum: Vec<(u8, i64)> = vec![(b'+', int(first, names)?)];
            for (op, term) in rest {
                let v = int(term, names)?;
                match op {
                    b'*' => {
                        let last = sum.last_mut()?;
                        last.1 = last.1.checked_mul(v)?;
                    }
                    b'/' => {
                        let last = sum.last_mut()?;
                        last.1 = last.1.checked_div(v)?;
                    }
                    _ => sum.push((*op, v)),
                }
            }
            let mut total = 0i64;
            for (op, v) in sum {
                total = if op == b'-' {
                    total.checked_sub(v)?
                } else {
                    total.checked_add(v)?
                };
            }
            Some(total)
        }
        _ => None,
    }
}

/// `Level3` or `3`, or an expression of them: a level, from 1 in the text,
/// from 0 here.
fn level(e: &Expr<'_>) -> Option<u32> {
    let n = int(e, &|s| numbered(s, "level"))?;
    (1..=i64::from(MAX_LEVELS))
        .contains(&n)
        .then(|| n as u32 - 1)
}

/// `Group2` or `2`, or an expression of them: a layout, from 1 in the text,
/// from 0 here.
fn group(e: &Expr<'_>) -> Option<u32> {
    let n = int(e, &|s| numbered(s, "group"))?;
    (1..=i64::from(MAX_GROUPS))
        .contains(&n)
        .then(|| n as u32 - 1)
}

/// The number of `Level3`, `group2`.
fn numbered(s: &str, prefix: &str) -> Option<i64> {
    if s.len() <= prefix.len() || !s.is_char_boundary(prefix.len()) {
        return None;
    }
    let (head, tail) = s.split_at(prefix.len());
    if !head.eq_ignore_ascii_case(prefix) || !tail.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    tail.parse().ok()
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

/// The field of `scope.field = value` or `scope.field[index] = value`, in
/// lower case, with its index.
fn scoped<'e, 'a>(lhs: &'e Expr<'a>, scope: &str) -> Option<(String, Option<&'e Expr<'a>>)> {
    let (base, index) = match lhs {
        Expr::Index(base, index) => (&**base, Some(&**index)),
        other => (other, None),
    };
    match base {
        Expr::Field(s, f) if s.eq_ignore_ascii_case(scope) => Some((f.to_ascii_lowercase(), index)),
        _ => None,
    }
}

/// A keysym as a keymap writes it in a key or an interpret: a name, a digit
/// for the digit's key, or a number. `None` for a name that is no keysym.
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

/// A keysym as a modifier map writes it: a name, a digit for the digit's
/// key, or a number. The words for "no symbol" are no keysym here.
fn modmap_keysym(e: &Expr<'_>) -> Option<Keysym> {
    match e {
        Expr::Int(v) if *v < 10 => Some(Keysym(0x30 + *v as u32)),
        Expr::Int(v) if *v <= u64::from(Keysym::MAX.0) => Some(Keysym(*v as u32)),
        Expr::Ident(s) => Keysym::from_name(s),
        _ => None,
    }
    .filter(|s| *s != Keysym::NO_SYMBOL)
}

/// The type a group gets when the keymap names none, from how many levels
/// it has and what is on the first of them. `None` past four levels: the
/// keymap's first type is used then.
fn automatic_type(levels: &[Vec<Keysym>]) -> Option<&'static str> {
    let sym = |i: usize| {
        levels
            .get(i)
            .and_then(|l| l.first())
            .copied()
            .unwrap_or(Keysym::NO_SYMBOL)
    };
    let width = levels.len();
    if width <= 1 {
        return Some("ONE_LEVEL");
    }
    let alphabetic = sym(0).is_lower() && sym(1).is_upper();
    let keypad = sym(0).is_keypad() || sym(1).is_keypad();
    if width == 2 {
        return Some(if alphabetic {
            "ALPHABETIC"
        } else if keypad {
            "KEYPAD"
        } else {
            "TWO_LEVEL"
        });
    }
    if width > 4 {
        return None;
    }
    Some(if alphabetic {
        if sym(2).is_lower() && sym(3).is_upper() {
            "FOUR_LEVEL_ALPHABETIC"
        } else {
            "FOUR_LEVEL_SEMIALPHABETIC"
        }
    } else if keypad {
        "FOUR_LEVEL_KEYPAD"
    } else {
        "FOUR_LEVEL"
    })
}

/// Whether what is new replaces what is old, for one field: it does if the
/// old statement did not set the field, or if both did and the new one may
/// clobber.
fn takes(old: bool, new: bool, clobber: bool) -> bool {
    if !old {
        new
    } else {
        new && clobber
    }
}

/// Merge the group `from` of a later statement for a key into `into`.
fn merge_group(into: &mut GroupDraft, from: GroupDraft, clobber: bool) {
    if let Some(name) = from.type_name {
        if into.type_name.is_none() || clobber {
            into.type_name = Some(name);
        }
    }
    if from.levels.is_empty() {
        return;
    }
    if into.levels.is_empty() {
        into.levels = from.levels;
        into.syms = from.syms;
        into.actions = from.actions;
        return;
    }
    let mut levels = from.levels.into_iter();
    for (old, new) in into.levels.iter_mut().zip(&mut levels) {
        if !new.is_empty() && (old.is_empty() || clobber) {
            *old = new;
        }
    }
    into.levels.extend(levels);
    into.syms |= from.syms;
    into.actions |= from.actions;
}

/// Merge a later statement for a key into the earlier one.
fn merge_key(into: &mut KeyDraft, from: KeyDraft, merge: Merge) {
    if merge == Merge::Replace {
        *into = from;
        return;
    }
    let clobber = merge != Merge::Augment;
    let mut groups = from.groups.into_iter();
    for (old, new) in into.groups.iter_mut().zip(&mut groups) {
        merge_group(old, new, clobber);
    }
    into.groups.extend(groups);
    if takes(into.vmodmap.is_some(), from.vmodmap.is_some(), clobber) {
        into.vmodmap = from.vmodmap;
    }
    if takes(into.repeat.is_some(), from.repeat.is_some(), clobber) {
        into.repeat = from.repeat;
    }
    if takes(
        into.default_type.is_some(),
        from.default_type.is_some(),
        clobber,
    ) {
        into.default_type = from.default_type;
    }
    if takes(
        into.out_of_range.is_some(),
        from.out_of_range.is_some(),
        clobber,
    ) {
        into.out_of_range = from.out_of_range;
    }
}

/// The group a key field with this index means, made if the key does not
/// have it yet. Without an index: the first group `has` does not hold for.
/// `None` for an index that is no group, or past the last a key may have.
fn group_slot<'k>(
    key: &'k mut KeyDraft,
    index: Option<&Expr<'_>>,
    has: impl Fn(&GroupDraft) -> bool,
) -> Option<&'k mut GroupDraft> {
    let g = match index {
        Some(e) => group(e)? as usize,
        None => key
            .groups
            .iter()
            .position(|g| !has(g))
            .unwrap_or(key.groups.len()),
    };
    if g >= MAX_GROUPS as usize {
        return None;
    }
    if key.groups.len() <= g {
        key.groups.resize_with(g + 1, GroupDraft::default);
    }
    Some(&mut key.groups[g])
}

impl Builder {
    fn mod_index(&self, name: &str) -> Option<usize> {
        self.mods
            .iter()
            .position(|m| m.name == name)
            .or_else(|| REAL_MODS.iter().position(|m| m.eq_ignore_ascii_case(name)))
    }

    /// A modifier mask: names joined with `+` and `-`, `all`, `none`, a
    /// number (its bits taken as they are), or `~` before any of them for
    /// the other bits. `!` is for a yes or no and is no mask. Virtual
    /// modifiers are allowed only where `virtual_ok`.
    fn mask(&self, e: &Expr<'_>, pos: Pos, virtual_ok: bool) -> Result<u32, Error> {
        Ok(match e {
            Expr::Int(v) => *v as u32,
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
            Expr::Ops(first, rest) => {
                let mut mask = self.mask(first, pos, virtual_ok)?;
                for (op, term) in rest {
                    let v = self.mask(term, pos, virtual_ok)?;
                    match op {
                        b'+' => mask |= v,
                        b'-' => mask &= !v,
                        _ => return err(pos, "expected a modifier mask"),
                    }
                }
                mask
            }
            Expr::Invert(a) => !self.mask(a, pos, virtual_ok)?,
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

    /// Give `name` the keycode `code`. A keycode has one name and a name
    /// one keycode: the later statement wins both ways, unless it says
    /// `augment`.
    fn name_key(&mut self, name: &str, code: Keycode, merge: Merge) {
        self.range = Some(match self.range {
            Some((low, high)) => (low.min(code), high.max(code)),
            None => (code, code),
        });
        if let Some(old) = self.codes.get(&code) {
            if old == name {
                return;
            }
            if merge == Merge::Augment {
                return;
            }
            let old = old.clone();
            self.names.remove(&old);
            self.codes.remove(&code);
        }
        if let Some(&old) = self.names.get(name) {
            if merge == Merge::Augment {
                return;
            }
            self.codes.remove(&old);
        }
        self.names.insert(name.to_string(), code);
        self.codes.insert(code, name.to_string());
    }

    fn keycodes(&mut self, stmts: &[Placed<'_>]) -> Result<(), Error> {
        let mut aliases: HashMap<String, String> = HashMap::new();
        for Placed { pos, merge, stmt } in stmts {
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
                    self.name_key(name, *code as Keycode, *merge);
                }
                Stmt::Alias(alias, real) => {
                    if *merge != Merge::Augment || !aliases.contains_key(*alias) {
                        aliases.insert(alias.to_string(), real.to_string());
                    }
                }
                Stmt::LedName(index, name) => {
                    if !(1..=MAX_LEDS as u64).contains(index) {
                        return err(*pos, "indicator number out of range");
                    }
                    let i = *index as usize - 1;
                    // A name stays with the first number it was given.
                    if self
                        .leds
                        .iter()
                        .any(|l| l.as_ref().is_some_and(|l| l.name == *name))
                    {
                        continue;
                    }
                    if self.leds.len() <= i {
                        self.leds.resize(i + 1, None);
                    }
                    // A number takes the last name it was given.
                    if self.leds[i].is_none() || *merge != Merge::Augment {
                        self.leds[i] = Some(Led {
                            name: name.clone(),
                            ..Led::default()
                        });
                    }
                }
                _ => {}
            }
        }
        // An alias stands only for a key that exists, and never hides one.
        aliases.retain(|a, r| !self.names.contains_key(a) && self.names.contains_key(r));
        self.aliases = aliases;
        Ok(())
    }

    fn types(&mut self, stmts: &[Placed<'_>]) -> Result<(), Error> {
        for Placed { pos, merge, stmt } in stmts {
            match stmt {
                Stmt::VMods(names) => self.add_vmods(names, *pos)?,
                Stmt::Type(name, body) => {
                    let ty = self.key_type(body);
                    match self.type_index.get(name) {
                        // The later type of a name replaces the earlier
                        // one, unless it says `augment`.
                        Some(&at) => {
                            if *merge != Merge::Augment {
                                self.types[at] = ty;
                            }
                        }
                        None => {
                            self.type_index.insert(name.clone(), self.types.len());
                            self.types.push(ty);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn key_type(&self, body: &[Item<'_>]) -> KeyType {
        let mut ty = KeyType {
            num_levels: 1,
            ..KeyType::default()
        };
        let mut has_mods = false;
        // Where the entry for each mask is.
        let mut at: HashMap<u32, usize> = HashMap::new();
        for item in body {
            let pos = item.pos;
            let Some((field, index, value)) = item_field(item) else {
                continue;
            };
            match (field.as_str(), index) {
                ("modifiers", None) => {
                    // Given twice, the first counts.
                    if let (false, Ok(mods)) = (has_mods, self.mask(value, pos, true)) {
                        ty.mods = mods;
                        has_mods = true;
                    }
                }
                ("map", Some(index)) => {
                    let (Ok(mods), Some(level)) = (self.mask(index, pos, true), level(value))
                    else {
                        continue;
                    };
                    // Modifiers the type does not use cannot pick a level.
                    let mods = mods & ty.mods;
                    ty.num_levels = ty.num_levels.max(level + 1);
                    match at.get(&mods) {
                        Some(&i) => ty.entries[i].level = level,
                        None => {
                            at.insert(mods, ty.entries.len());
                            ty.entries.push(Entry {
                                mods,
                                level,
                                ..Entry::default()
                            });
                        }
                    }
                }
                ("preserve", Some(index)) => {
                    let (Ok(mods), Ok(preserve)) =
                        (self.mask(index, pos, true), self.mask(value, pos, true))
                    else {
                        continue;
                    };
                    let mods = mods & ty.mods;
                    let preserve = preserve & mods;
                    match at.get(&mods) {
                        Some(&i) => ty.entries[i].preserve_mods = preserve,
                        None => {
                            at.insert(mods, ty.entries.len());
                            ty.entries.push(Entry {
                                mods,
                                preserve_mods: preserve,
                                ..Entry::default()
                            });
                        }
                    }
                }
                // A level's name gives the type no level: only `map` does.
                _ => {}
            }
        }
        ty
    }

    fn compat(&mut self, stmts: &[Placed<'_>]) -> Result<(), Error> {
        let mut default = Interp {
            sym: Keysym::NO_SYMBOL,
            matching: Match::AnyOrNone,
            mods: REAL_MASK,
            vmod: None,
            repeat: false,
            level_one_only: false,
            defined: 0,
        };
        let mut default_led = LedDraft::default();
        let mut found: Vec<Interp> = Vec::new();
        let mut found_at: HashMap<(Keysym, Match, u32), usize> = HashMap::new();
        let mut led_maps: Vec<(String, LedDraft)> = Vec::new();
        let mut led_at: HashMap<String, usize> = HashMap::new();
        for Placed { pos, merge, stmt } in stmts {
            match stmt {
                Stmt::VMods(names) => self.add_vmods(names, *pos)?,
                Stmt::Item(Item {
                    lhs: Some(lhs),
                    value,
                    ..
                }) => {
                    if let Some((field, None)) = scoped(lhs, "interpret") {
                        self.interp_field(&mut default, &field, value, *pos)?;
                    } else if let Some((field, None)) = scoped(lhs, "indicator") {
                        self.led_field(&mut default_led, &field, value, *pos)?;
                    }
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
                        // A field that makes no sense is passed over.
                        if let Some((field, None, value)) = item_field(item) {
                            let _ = self.interp_field(&mut interp, &field, value, item.pos);
                        }
                    }
                    // The same keysym and match again: field by field, the
                    // later statement wins what it sets (and what the
                    // earlier one did not), unless it says `augment`.
                    match found_at.get(&(sym, matching, mods)) {
                        Some(&at) if *merge == Merge::Replace => found[at] = interp,
                        Some(&at) => {
                            let clobber = *merge != Merge::Augment;
                            let old = &mut found[at];
                            let new = interp;
                            let wins = |bit: u8, old: &Interp| {
                                old.defined & bit == 0 || (new.defined & bit != 0 && clobber)
                            };
                            if wins(INTERP_VMOD, old) {
                                old.vmod = new.vmod;
                                old.defined |= INTERP_VMOD;
                            }
                            if wins(INTERP_REPEAT, old) {
                                old.repeat = new.repeat;
                                old.defined |= INTERP_REPEAT;
                            }
                            if wins(INTERP_LEVEL_ONE, old) {
                                old.level_one_only = new.level_one_only;
                                old.defined |= INTERP_LEVEL_ONE;
                            }
                        }
                        None => {
                            found_at.insert((sym, matching, mods), found.len());
                            found.push(interp);
                        }
                    }
                }
                Stmt::LedMap(name, body) => {
                    let mut led = default_led;
                    for item in body {
                        if let Some((field, None, value)) = item_field(item) {
                            self.led_field(&mut led, &field, value, item.pos)?;
                        }
                    }
                    match led_at.get(name) {
                        Some(&at) => {
                            let old = &mut led_maps[at].1;
                            if *merge == Merge::Replace {
                                *old = led;
                            } else if (old.mods, old.which_mods, old.groups, old.which_groups)
                                != (led.mods, led.which_mods, led.groups, led.which_groups)
                            {
                                // Half by half (the modifiers, the groups),
                                // the later map wins what it sets and what
                                // the earlier one did not.
                                let clobber = *merge != Merge::Augment;
                                if !old.has_mods || (led.has_mods && clobber) {
                                    old.mods = led.mods;
                                    old.which_mods = led.which_mods;
                                    old.has_mods = true;
                                }
                                if !old.has_groups || (led.has_groups && clobber) {
                                    old.groups = led.groups;
                                    old.which_groups = led.which_groups;
                                    old.has_groups = true;
                                }
                            } else {
                                old.has_mods |= led.has_mods;
                                old.has_groups |= led.has_groups;
                            }
                        }
                        None => {
                            led_at.insert(name.clone(), led_maps.len());
                            led_maps.push((name.clone(), led));
                        }
                    }
                }
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
        for (name, draft) in led_maps {
            // The LED of that name from the keycodes section, else the
            // first free slot, else a new one.
            let slot = self
                .leds
                .iter()
                .position(|l| l.as_ref().is_some_and(|l| l.name == name))
                .or_else(|| self.leds.iter().position(Option::is_none));
            let led = Led {
                name,
                mods: draft.mods,
                mask: 0,
                groups: draft.groups,
                // Modifiers or groups without a "which" (or with `none`)
                // mean the effective ones.
                which_mods: match (draft.which_mods, draft.mods) {
                    (0, m) if m != 0 => WHICH_EFFECTIVE,
                    (which, _) => which,
                },
                which_groups: match (draft.which_groups, draft.groups) {
                    (0, g) if g != 0 => WHICH_EFFECTIVE,
                    (which, _) => which,
                },
            };
            match slot {
                Some(i) => self.leds[i] = Some(led),
                None if self.leds.len() < MAX_LEDS => self.leds.push(Some(led)),
                None => {}
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
        match field {
            "virtualmodifier" | "virtualmod" => {
                let Expr::Ident(name) = value else {
                    return err(pos, "expected a virtual modifier");
                };
                match self.mods.iter().position(|m| m.name == *name) {
                    Some(i) if i >= 8 => interp.vmod = Some(i as u32),
                    _ => return err(pos, format!("`{name}` is not a virtual modifier")),
                }
                interp.defined |= INTERP_VMOD;
            }
            "repeat" => {
                interp.repeat =
                    boolean(value).map_or_else(|| err(pos, "expected yes or no"), Ok)?;
                interp.defined |= INTERP_REPEAT;
            }
            "usemodmapmods" | "usemodmap" => {
                interp.level_one_only = match word(value).as_deref() {
                    Some("level1" | "levelone") => true,
                    Some("anylevel" | "any") => false,
                    _ => return err(pos, "expected level1 or anylevel"),
                };
                interp.defined |= INTERP_LEVEL_ONE;
            }
            _ => {}
        }
        Ok(())
    }

    fn led_field(
        &self,
        led: &mut LedDraft,
        field: &str,
        value: &Expr<'_>,
        pos: Pos,
    ) -> Result<(), Error> {
        match field {
            "modifiers" | "mods" => {
                led.mods = self.mask(value, pos, true)?;
                led.has_mods = true;
            }
            "groups" => {
                led.groups = group_mask(value, pos)?;
                led.has_groups = true;
            }
            "whichmodstate" | "whichmodifierstate" => led.which_mods = which(value, pos, false)?,
            "whichgroupstate" => led.which_groups = which(value, pos, true)?,
            _ => {}
        }
        Ok(())
    }

    /// Set one field of a key from an item of its body (or of a `key.field
    /// = value` default). A field that makes no sense is passed over.
    fn key_field(
        &self,
        key: &mut KeyDraft,
        field: &str,
        index: Option<&Expr<'_>>,
        value: &Expr<'_>,
        pos: Pos,
    ) -> Result<(), Error> {
        match field {
            "type" => {
                let Expr::Str(name) = value else {
                    return Ok(());
                };
                match index {
                    Some(_) => {
                        if let Some(g) = group_slot(key, index, |_| false) {
                            g.type_name = Some(name.clone());
                        }
                    }
                    None => key.default_type = Some(name.clone()),
                }
            }
            "symbols" => {
                let Expr::List(list) = value else {
                    return Ok(());
                };
                if list.len() > MAX_LEVELS as usize {
                    return err(pos, "too many levels");
                }
                let Some(g) = group_slot(key, index, |g| g.syms) else {
                    return Ok(());
                };
                // An empty list still says the group is there. Symbols for
                // a group that has them already are passed over.
                if list.is_empty() || g.syms {
                    g.syms = true;
                    return Ok(());
                }
                g.syms = true;
                if g.levels.len() < list.len() {
                    g.levels.resize(list.len(), Vec::new());
                }
                for (level, e) in g.levels.iter_mut().zip(list) {
                    // A name that is no keysym counts as none; a level
                    // holding only "no symbol" is empty.
                    let one = |e: &Expr<'_>| match keysym(e).unwrap_or(Keysym::NO_SYMBOL) {
                        Keysym::NO_SYMBOL => Vec::new(),
                        sym => vec![sym],
                    };
                    *level = match e {
                        Expr::Set(set) if set.len() == 1 => one(&set[0]),
                        Expr::Set(set) => set
                            .iter()
                            .map(|s| keysym(s).unwrap_or(Keysym::NO_SYMBOL))
                            .collect(),
                        other => one(other),
                    };
                }
            }
            "actions" => {
                let Expr::List(list) = value else {
                    return Ok(());
                };
                if list.len() > MAX_LEVELS as usize {
                    return err(pos, "too many levels");
                }
                let Some(g) = group_slot(key, index, |g| g.actions) else {
                    return Ok(());
                };
                // Each action is a level of the group, whatever it does.
                if !g.actions && g.levels.len() < list.len() {
                    g.levels.resize(list.len(), Vec::new());
                }
                g.actions = true;
            }
            "repeat" | "repeats" | "repeating" => match word(value).as_deref() {
                Some("default") => key.repeat = Some(None),
                _ => {
                    if let Some(yes) = boolean(value) {
                        key.repeat = Some(Some(yes));
                    }
                }
            },
            "virtualmods" | "vmods" | "virtualmodifiers" => {
                if let Ok(mask) = self.mask(value, pos, true) {
                    key.vmodmap = Some(mask & !REAL_MASK);
                }
            }
            "groupswrap" | "wrapgroups" => {
                if let Some(yes) = boolean(value) {
                    key.out_of_range = Some(if yes {
                        OutOfRange::Wrap
                    } else {
                        OutOfRange::Clamp
                    });
                }
            }
            "groupsclamp" | "clampgroups" => {
                if let Some(yes) = boolean(value) {
                    key.out_of_range = Some(if yes {
                        OutOfRange::Clamp
                    } else {
                        OutOfRange::Wrap
                    });
                }
            }
            "groupsredirect" | "redirectgroups" => {
                if let Some(g) = group(value) {
                    key.out_of_range = Some(OutOfRange::Redirect(g));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A key as one statement describes it, starting from the defaults the
    /// `key.field = value` statements before it set.
    fn key_draft(&self, default: &KeyDraft, body: &[Item<'_>]) -> Result<KeyDraft, Error> {
        let mut key = default.clone();
        for item in body {
            let (field, index, value) = match (&item.lhs, &item.value) {
                // A bare list is the next group's symbols, or its actions.
                (None, Expr::List(list)) if is_actions(list) => {
                    ("actions".to_string(), None, &item.value)
                }
                (None, Expr::List(_)) => ("symbols".to_string(), None, &item.value),
                _ => match item_field(item) {
                    Some(f) => f,
                    None => continue,
                },
            };
            self.key_field(&mut key, &field, index, value, item.pos)?;
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
                num_levels: 1,
                ..KeyType::default()
            });
        }

        // Symbols: the keys, then what needs all of them (the modifier map).
        let mut default_key = KeyDraft::default();
        let mut drafts: HashMap<Keycode, KeyDraft> = HashMap::new();
        // The modifier map's entries in the order first given, each with the
        // modifier it ends up with (`None` for `none`).
        let mut modmap: Vec<(Mapped, Option<u32>)> = Vec::new();
        let mut modmap_at: HashMap<Mapped, usize> = HashMap::new();
        for Placed { pos, merge, stmt } in symbols {
            match stmt {
                Stmt::VMods(names) => self.add_vmods(names, *pos)?,
                Stmt::Item(Item {
                    lhs: Some(lhs),
                    value,
                    ..
                }) => {
                    if let Some((field, index)) = scoped(lhs, "key") {
                        self.key_field(&mut default_key, &field, index, value, *pos)?;
                    } else if let (Some((field, Some(index))), Expr::Str(name)) =
                        (target(lhs), value)
                    {
                        if field == "name" || field == "groupname" {
                            let Some(g) = group(index) else {
                                return err(*pos, "expected a group");
                            };
                            let g = g as usize;
                            if self.group_names.len() <= g {
                                self.group_names.resize(g + 1, None);
                            }
                            self.group_names[g] = Some(name.clone());
                        }
                    }
                }
                Stmt::Key(name, body) => {
                    let draft = self.key_draft(&default_key, body)?;
                    // A key the keycodes section does not have is left out.
                    // A key given again is merged into what it was.
                    if let Some(keycode) = self.keycode(name) {
                        match drafts.get_mut(&keycode) {
                            Some(old) => merge_key(old, draft, *merge),
                            None => {
                                drafts.insert(keycode, draft);
                            }
                        }
                    }
                }
                Stmt::ModMap(name, entries) => {
                    let modifier = if name.eq_ignore_ascii_case("none") {
                        None
                    } else {
                        match self.mod_index(name) {
                            Some(i) if i < 8 => Some(i as u32),
                            _ => return err(*pos, format!("`{name}` is not a real modifier")),
                        }
                    };
                    for entry in entries {
                        let mapped = match entry {
                            Expr::Key(k) => Mapped::Key(k.to_string()),
                            other => match modmap_keysym(other) {
                                Some(sym) => Mapped::Sym(sym),
                                None => continue,
                            },
                        };
                        // The same key or keysym again changes its
                        // modifier, unless the statement says `augment`.
                        match modmap_at.get(&mapped) {
                            Some(&at) => {
                                if *merge != Merge::Augment {
                                    modmap[at].1 = modifier;
                                }
                            }
                            None => {
                                modmap_at.insert(mapped.clone(), modmap.len());
                                modmap.push((mapped, modifier));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        let mut syms: Vec<Keysym> = Vec::new();
        let mut slots = 0usize;
        let mut keys: Vec<Key> = Vec::with_capacity(self.names.len());
        let mut explicit_actions: Vec<bool> = Vec::with_capacity(self.names.len());
        let mut explicit_repeat: Vec<bool> = Vec::with_capacity(self.names.len());
        let mut explicit_vmodmap: Vec<bool> = Vec::with_capacity(self.names.len());
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
            let (mut actions, mut repeat, mut vmodmap) = (false, false, false);
            // The groups the key needs: up to the last given symbols,
            // actions or a type. A key with none is as if it had no
            // statement.
            let draft = drafts.remove(&keycode).and_then(|mut draft| {
                let count = draft.groups.iter().rposition(GroupDraft::defined)? + 1;
                draft.groups.truncate(count);
                Some(draft)
            });
            if let Some(mut draft) = draft {
                key.out_of_range = draft.out_of_range.unwrap_or(OutOfRange::Wrap);
                if let Some(v) = draft.vmodmap {
                    key.vmodmap = v;
                    vmodmap = true;
                }
                if let Some(Some(r)) = draft.repeat {
                    key.repeats = r;
                    repeat = true;
                }
                // A gap below the last group copies the first group.
                for g in 1..draft.groups.len() {
                    if !draft.groups[g].defined() {
                        draft.groups[g] = draft.groups[0].clone();
                    }
                }
                actions = draft.groups.iter().any(|g| g.actions);
                for group in draft.groups {
                    let mut levels = group.levels;
                    // A type the keymap does not define, or none that can
                    // be worked out, falls back to its first one.
                    let ty = group
                        .type_name
                        .as_deref()
                        .or(draft.default_type.as_deref())
                        .or_else(|| automatic_type(&levels))
                        .and_then(|name| self.type_index.get(name).copied())
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
            explicit_actions.push(actions);
            explicit_repeat.push(repeat);
            explicit_vmodmap.push(vmodmap);
        }

        // The modifier map: each entry's key (the key named, or the key
        // holding the keysym on the lowest group, then level, then keycode)
        // gets the entry's modifier, on top of what other entries gave it.
        let mut holder: Option<HashMap<Keysym, (usize, usize, usize)>> = None;
        for (mapped, modifier) in &modmap {
            let at = match mapped {
                Mapped::Key(name) => self
                    .keycode(name)
                    .and_then(|c| keys.binary_search_by_key(&c, |k| k.keycode).ok()),
                Mapped::Sym(sym) => holder
                    .get_or_insert_with(|| holders(&keys, &syms))
                    .get(sym)
                    .map(|&(_, _, at)| at),
            };
            if let (Some(at), Some(modifier)) = (at, modifier) {
                keys[at].modmap |= 1 << modifier;
            }
        }

        // What the compatibility section's interprets give each key: its
        // virtual modifiers and whether it repeats.
        let mut finder = InterpFinder::new(&self.interps);
        for (i, key) in keys.iter_mut().enumerate() {
            if explicit_actions[i] {
                continue;
            }
            let mut vmodmap = 0u32;
            let mut repeats = false;
            for (g, grp) in key.groups.iter().enumerate() {
                for (l, &(start, len)) in grp.levels.iter().enumerate() {
                    let level_syms = &syms[start as usize..(start + len) as usize];
                    let Some(interp) = finder.find(level_syms, l, key.modmap) else {
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
            if !explicit_repeat[i] {
                key.repeats = repeats;
            }
            if !explicit_vmodmap[i] {
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
        let mut aliases: Vec<(String, String)> = self.aliases.into_iter().collect();
        aliases.sort();
        let mut keymap = Keymap {
            mods: self.mods,
            types: self.types,
            keys,
            range: self.range,
            aliases,
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
        self.names
            .get(name)
            .or_else(|| self.names.get(self.aliases.get(name)?))
            .copied()
    }
}

/// Finds the interpret for a level, in time that does not grow with the
/// number of interprets for other keysyms.
struct InterpFinder<'i> {
    interps: &'i [Interp],
    /// The interprets for each named keysym, in the order they are tried.
    by_sym: HashMap<Keysym, Vec<usize>>,
    /// The interprets for any keysym, in the order they are tried.
    any: Vec<usize>,
    /// Answers already worked out, by keysym (none for "any"), whether the
    /// level is the first, and the key's modifier map.
    memo: HashMap<(Option<Keysym>, bool, u32), Option<usize>>,
}

impl<'i> InterpFinder<'i> {
    fn new(interps: &'i [Interp]) -> Self {
        let mut by_sym: HashMap<Keysym, Vec<usize>> = HashMap::new();
        let mut any = Vec::new();
        for (i, interp) in interps.iter().enumerate() {
            if interp.sym == Keysym::NO_SYMBOL {
                any.push(i);
            } else {
                by_sym.entry(interp.sym).or_default().push(i);
            }
        }
        InterpFinder {
            interps,
            by_sym,
            any,
            memo: HashMap::new(),
        }
    }

    fn first(&mut self, sym: Option<Keysym>, first_level: bool, modmap: u32) -> Option<usize> {
        if let Some(&known) = self.memo.get(&(sym, first_level, modmap)) {
            return known;
        }
        let candidates: &[usize] = match sym {
            Some(sym) => self.by_sym.get(&sym).map_or(&[], Vec::as_slice),
            None => &self.any,
        };
        let found = candidates.iter().copied().find(|&at| {
            let i = &self.interps[at];
            let mods = if i.level_one_only && !first_level {
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
        self.memo.insert((sym, first_level, modmap), found);
        found
    }

    /// The first interpret that matches a level's keysyms on a key with
    /// this modifier map; for a level no interpret matches, the default one
    /// (an ordinary key: it repeats). `None` for an empty level.
    fn find(&mut self, syms: &[Keysym], level: usize, modmap: u32) -> Option<Interp> {
        if syms.is_empty() {
            return None;
        }
        // An interpret for a named keysym is for a level holding only it.
        let named = match syms {
            [one] => self.first(Some(*one), level == 0, modmap),
            _ => None,
        };
        let found = named.or_else(|| self.first(None, level == 0, modmap));
        Some(found.map_or(
            Interp {
                sym: Keysym::NO_SYMBOL,
                matching: Match::AnyOrNone,
                mods: 0,
                vmod: None,
                repeat: true,
                level_one_only: false,
                defined: 0,
            },
            |at| self.interps[at],
        ))
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

/// For each keysym alone on a level, the key a modifier map means by it:
/// the one holding it on the lowest group, then level, then keycode. One
/// pass over the keymap, however many entries ask.
fn holders(keys: &[Key], syms: &[Keysym]) -> HashMap<Keysym, (usize, usize, usize)> {
    let mut best: HashMap<Keysym, (usize, usize, usize)> = HashMap::new();
    for (at, key) in keys.iter().enumerate() {
        for (g, grp) in key.groups.iter().enumerate() {
            for (l, &(start, len)) in grp.levels.iter().enumerate() {
                if len != 1 {
                    continue;
                }
                let here = (g, l, at);
                best.entry(syms[start as usize])
                    .and_modify(|b| *b = (*b).min(here))
                    .or_insert(here);
            }
        }
    }
    best
}

/// An LED's `groups`: a number (its bits are the groups), or `Group2`,
/// `all`, `none`, joined with `+` and `-`, or turned over with `~`.
fn group_mask(e: &Expr<'_>, pos: Pos) -> Result<u32, Error> {
    Ok(match e {
        Expr::Int(v) => *v as u32,
        Expr::Ident(s) if s.eq_ignore_ascii_case("all") => 0xff,
        Expr::Ident(s) if s.eq_ignore_ascii_case("none") => 0,
        Expr::Ident(s) => match numbered(s, "group") {
            Some(n) if (1..=i64::from(MAX_GROUPS)).contains(&n) => 1 << (n - 1),
            _ => return err(pos, "expected a group"),
        },
        Expr::Ops(first, rest) => {
            let mut mask = group_mask(first, pos)?;
            for (op, term) in rest {
                let v = group_mask(term, pos)?;
                match op {
                    b'+' => mask |= v,
                    b'-' => mask &= !v,
                    _ => return err(pos, "expected a group mask"),
                }
            }
            mask
        }
        Expr::Invert(a) => !group_mask(a, pos)?,
        _ => return err(pos, "expected a group mask"),
    })
}

/// An LED's `whichModState` / `whichGroupState`. A number is the mask the
/// X protocol's state components have: its low four bits are the modifier
/// components, the next four the group components.
fn which(e: &Expr<'_>, pos: Pos, groups: bool) -> Result<u8, Error> {
    Ok(match e {
        Expr::Int(v) if groups => (*v >> 4) as u8 & 0xf,
        Expr::Int(v) => *v as u8 & 0xf,
        Expr::Ident(s) => match s.to_ascii_lowercase().as_str() {
            "base" | "depressed" => WHICH_DEPRESSED,
            "latched" => WHICH_LATCHED,
            "locked" => WHICH_LOCKED,
            "effective" | "compat" | "any" => WHICH_EFFECTIVE,
            "none" => 0,
            _ => return err(pos, "expected a state component"),
        },
        Expr::Ops(first, rest) => {
            let mut mask = which(first, pos, groups)?;
            for (op, term) in rest {
                let v = which(term, pos, groups)?;
                match op {
                    b'+' => mask |= v,
                    b'-' => mask &= !v,
                    _ => return err(pos, "expected a state component"),
                }
            }
            mask
        }
        Expr::Invert(a) => !which(a, pos, groups)? & 0xf,
        _ => return err(pos, "expected a state component"),
    })
}
