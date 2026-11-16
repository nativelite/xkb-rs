//! Keyboard state on the client side: the modifiers and layout the
//! compositor reports, and what each key means under them.

use crate::keymap::{
    wrap_group, Component, Entry, Key, Keycode, Keymap, OutOfRange, WHICH_DEPRESSED,
    WHICH_EFFECTIVE, WHICH_LATCHED, WHICH_LOCKED,
};
use crate::keysym::Keysym;

/// The index of the `Lock` (Caps Lock) modifier, in every keymap.
const LOCK: u32 = 1;
/// The index of the `Control` modifier, in every keymap.
const CONTROL: u32 = 2;

/// A keymap and the modifiers and layout currently in effect.
///
/// A Wayland client feeds it every `wl_keyboard.modifiers` event through
/// [`State::update_mask`] and asks it what each `wl_keyboard.key` means. A
/// new keymap from the compositor means a new `State`.
#[derive(Debug, Clone)]
pub struct State {
    keymap: Keymap,
    depressed_mods: u32,
    latched_mods: u32,
    locked_mods: u32,
    mods: u32,
    depressed_group: i32,
    latched_group: i32,
    locked_group: i32,
    group: u32,
}

/// What a character becomes with Control held: `a` is 0x01, `[` is Escape,
/// and the few digits terminals have always mapped.
fn to_control(c: u8) -> u8 {
    match c {
        b'@'..=b'~' | b' ' => c & 0x1f,
        b'2' => 0,
        b'3'..=b'7' => c - (b'3' - 0x1b),
        b'8' => 0x7f,
        b'/' => b'_' & 0x1f,
        _ => c,
    }
}

impl State {
    /// A state for `keymap` with no modifier active and the first layout.
    pub fn new(keymap: Keymap) -> State {
        State {
            keymap,
            depressed_mods: 0,
            latched_mods: 0,
            locked_mods: 0,
            mods: 0,
            depressed_group: 0,
            latched_group: 0,
            locked_group: 0,
            group: 0,
        }
    }

    /// The keymap this state reads keys with.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Set the state to what the compositor reports. The arguments of
    /// `wl_keyboard.modifiers` are `mods_depressed`, `mods_latched`,
    /// `mods_locked` and `group`; pass `group` as `locked_layout` and 0 for
    /// the other two layouts. Returns whether anything changed.
    ///
    /// Bits for modifiers the keymap does not have are dropped, and a
    /// virtual modifier's bit also sets the real ones it stands for.
    pub fn update_mask(
        &mut self,
        depressed_mods: u32,
        latched_mods: u32,
        locked_mods: u32,
        depressed_layout: u32,
        latched_layout: u32,
        locked_layout: u32,
    ) -> bool {
        let before = (
            self.depressed_mods,
            self.latched_mods,
            self.locked_mods,
            self.depressed_group,
            self.latched_group,
            self.locked_group,
        );
        let known = ((1u64 << self.keymap.num_mods()) - 1) as u32;
        // A virtual modifier brings the real ones it stands for with it,
        // and stays set itself.
        let resolve = |m: u32| (m & known) | self.keymap.effective(m & known);
        self.depressed_mods = resolve(depressed_mods);
        self.latched_mods = resolve(latched_mods);
        self.locked_mods = resolve(locked_mods);
        self.mods = self.depressed_mods | self.latched_mods | self.locked_mods;
        let count = self.keymap.num_layouts();
        let wrap = |g: i64| wrap_group(g, count, OutOfRange::Wrap).unwrap_or(0);
        self.depressed_group = depressed_layout as i32;
        self.latched_group = latched_layout as i32;
        self.locked_group = wrap(i64::from(locked_layout as i32)) as i32;
        self.group = wrap(
            i64::from(self.depressed_group)
                + i64::from(self.latched_group)
                + i64::from(self.locked_group),
        );
        before
            != (
                self.depressed_mods,
                self.latched_mods,
                self.locked_mods,
                self.depressed_group,
                self.latched_group,
                self.locked_group,
            )
    }

    /// The modifier mask of one component of the state: the real
    /// modifiers, and any virtual ones that were set by their own bit.
    pub fn mods(&self, component: Component) -> u32 {
        match component {
            Component::Depressed => self.depressed_mods,
            Component::Latched => self.latched_mods,
            Component::Locked => self.locked_mods,
            Component::Effective => self.mods,
        }
    }

    /// The layout in effect, in `0..keymap.num_layouts()` (0 for a keymap
    /// without layouts).
    pub fn layout(&self) -> u32 {
        self.group
    }

    /// Whether the modifier at `index` is active in `component`.
    pub fn mod_is_active(&self, index: u32, component: Component) -> bool {
        index < self.keymap.num_mods() && self.mods(component) & (1 << index) != 0
    }

    /// Whether the modifier called `name` (`"Shift"`, `"Control"`, `"Mod1"`)
    /// is active in `component`; `false` for a name the keymap does not have.
    pub fn mod_name_is_active(&self, name: &str, component: Component) -> bool {
        self.keymap
            .mod_index(name)
            .is_some_and(|i| self.mod_is_active(i, component))
    }

    /// Whether the LED at `index` is lit.
    pub fn led_is_active(&self, index: u32) -> bool {
        let Some(led) = self
            .keymap
            .leds
            .get(index as usize)
            .and_then(|l| l.as_ref())
        else {
            return false;
        };
        let mut mods = 0;
        let mut groups = 0u32;
        let bit = |g: i64| {
            if (0..32).contains(&g) {
                1u32 << g
            } else {
                0
            }
        };
        for (which, m, g) in [
            (WHICH_EFFECTIVE, self.mods, i64::from(self.group)),
            (
                WHICH_DEPRESSED,
                self.depressed_mods,
                i64::from(self.depressed_group),
            ),
            (
                WHICH_LATCHED,
                self.latched_mods,
                i64::from(self.latched_group),
            ),
            (WHICH_LOCKED, self.locked_mods, i64::from(self.locked_group)),
        ] {
            if led.which_mods & which != 0 {
                mods |= m;
            }
            if led.which_groups & which != 0 {
                groups |= bit(g);
            }
        }
        led.mask & mods != 0 || led.groups & groups != 0
    }

    /// Whether the LED called `name` (`"Caps Lock"`) is lit; `false` for a
    /// name the keymap does not have.
    pub fn led_name_is_active(&self, name: &str) -> bool {
        self.keymap
            .led_index(name)
            .is_some_and(|i| self.led_is_active(i))
    }

    /// The layout a key is read in: the state's, brought into the key's own
    /// range. `None` for a key without symbols.
    pub fn key_layout(&self, keycode: Keycode) -> Option<u32> {
        let key = self.keymap.key(keycode)?;
        wrap_group(
            i64::from(self.group),
            key.groups.len() as u32,
            key.out_of_range,
        )
    }

    /// The matching entry of the key's type in `group`, if any.
    fn entry(&self, key: &Key, group: u32) -> Option<&Entry> {
        let ty = &self.keymap.types[key.groups[group as usize].ty as usize];
        let active = self.mods & ty.mask;
        ty.entries.iter().find(|e| e.active() && e.mask == active)
    }

    /// The level of a key in `layout` under the current modifiers. `None`
    /// for a key without symbols and for a layout the key does not have:
    /// ask with what [`key_layout`](Self::key_layout) answers.
    pub fn key_level(&self, keycode: Keycode, layout: u32) -> Option<u32> {
        let key = self.keymap.key(keycode)?;
        if layout as usize >= key.groups.len() {
            return None;
        }
        Some(self.entry(key, layout).map_or(0, |e| e.level))
    }

    /// The keysyms a key gives now: usually one, none for an empty level or
    /// a key the keymap does not have.
    pub fn key_syms(&self, keycode: Keycode) -> &[Keysym] {
        let Some(layout) = self.key_layout(keycode) else {
            return &[];
        };
        let level = self.key_level(keycode, layout).unwrap_or(0);
        self.keymap.key_syms_by_level(keycode, layout, level)
    }

    /// The one keysym a key gives now, with Caps Lock applied where the
    /// key's type does not already handle it. [`Keysym::NO_SYMBOL`] if the
    /// key gives none, or several.
    pub fn key_one_sym(&self, keycode: Keycode) -> Keysym {
        match self.key_syms(keycode) {
            [sym] if self.transforms(keycode, LOCK) => sym.to_upper(),
            [sym] => *sym,
            _ => Keysym::NO_SYMBOL,
        }
    }

    /// Whether `modifier` is active and the key's type leaves it for the
    /// application: then the conventional transformation applies.
    fn transforms(&self, keycode: Keycode, modifier: u32) -> bool {
        self.mods & (1 << modifier) != 0 && !self.mod_is_consumed(keycode, modifier)
    }

    /// The keysym a key's text is made from: its one keysym, with the
    /// Control and Caps Lock transformations.
    fn sym_for_text(&self, keycode: Keycode) -> Keysym {
        let Some(layout) = self.key_layout(keycode) else {
            return Keysym::NO_SYMBOL;
        };
        let level = self.key_level(keycode, layout).unwrap_or(0);
        let [mut sym] = *self.keymap.key_syms_by_level(keycode, layout, level) else {
            return Keysym::NO_SYMBOL;
        };
        // Control on a layout without Latin letters: use the first layout
        // of the key that has an ASCII symbol there, so Ctrl+C works on a
        // Cyrillic layout.
        if sym.0 > 127 && self.transforms(keycode, CONTROL) {
            let count = self.keymap.num_layouts_for_key(keycode);
            for other in 0..count {
                let level = self.key_level(keycode, other).unwrap_or(0);
                if let [ascii] = *self.keymap.key_syms_by_level(keycode, other, level) {
                    if ascii.0 <= 127 {
                        sym = ascii;
                        break;
                    }
                }
            }
        }
        if self.transforms(keycode, LOCK) {
            sym = sym.to_upper();
        }
        sym
    }

    /// The text a key press types now: empty for a key that types nothing
    /// (a modifier, an arrow). Keys that send a control character type it
    /// (`Return` is `"\r"`), and Control turns a letter into its control
    /// character (`Ctrl+C` is `"\x03"`).
    pub fn key_utf8(&self, keycode: Keycode) -> String {
        let one = self.sym_for_text(keycode);
        let syms = if one == Keysym::NO_SYMBOL {
            self.key_syms(keycode)
        } else {
            std::slice::from_ref(&one)
        };
        let mut out = String::new();
        for sym in syms {
            match sym.to_char() {
                Some(c) => out.push(c),
                None => return String::new(),
            }
        }
        if out.len() == 1 && self.transforms(keycode, CONTROL) {
            let c = to_control(out.as_bytes()[0]);
            out.clear();
            out.push(c as char);
        }
        out
    }

    /// The character a key press types now, if it types exactly one; with
    /// the same transformations as [`State::key_utf8`].
    pub fn key_char(&self, keycode: Keycode) -> Option<char> {
        let text = self.key_utf8(keycode);
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        }
    }

    /// The code point a key press types now, 0 if none; with the same
    /// transformations as [`State::key_utf8`].
    pub fn key_utf32(&self, keycode: Keycode) -> u32 {
        let cp = self.sym_for_text(keycode).to_utf32();
        if cp != 0 && cp <= 127 && self.transforms(keycode, CONTROL) {
            u32::from(to_control(cp as u8))
        } else {
            cp
        }
    }

    /// The modifiers the key's type uses up to pick the current level, as a
    /// mask of real modifiers: an application matching shortcuts should
    /// ignore them (`Shift` is consumed by `Shift+1 = !`, so `Ctrl+!` is
    /// Control alone).
    pub fn key_consumed_mods(&self, keycode: Keycode) -> u32 {
        let Some(key) = self.keymap.key(keycode) else {
            return 0;
        };
        let Some(group) = self.key_layout(keycode) else {
            return 0;
        };
        let ty = &self.keymap.types[key.groups[group as usize].ty as usize];
        let preserve = self.entry(key, group).map_or(0, |e| e.preserve);
        ty.mask & !preserve
    }

    /// Whether the key's type consumes the modifier at `index` (see
    /// [`State::key_consumed_mods`]).
    pub fn mod_is_consumed(&self, keycode: Keycode, index: u32) -> bool {
        index < 32 && self.key_consumed_mods(keycode) & (1 << index) != 0
    }

    /// Whether holding the key repeats it.
    pub fn key_repeats(&self, keycode: Keycode) -> bool {
        self.keymap.key_repeats(keycode)
    }
}
