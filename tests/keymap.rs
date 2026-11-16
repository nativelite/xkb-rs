//! Keymaps written by hand: each rule of the format on its own, the
//! errors, and damaged text.

use xkb::{Component, Keymap, Keysym, State};

const KEYMAP: &str = r#"
xkb_keymap {
xkb_keycodes "test" {
    minimum = 8; maximum = 255;
    <ESC> = 9; <AE01> = 10; <AD01> = 24; <AD02> = 25; <AD03> = 26; <RTRN> = 36;
    <LCTL> = 37; <LFSH> = 50; <LALT> = 64; <SPCE> = 65; <CAPS> = 66;
    <NMLK> = 77; <KP1> = 87; <RALT> = 108; <MULT> = 200; <NONE> = 201;
    alias <LatQ> = <AD01>;
    alias <Bad> = <Nope>;
    indicator 1 = "Caps Lock";
    indicator 2 = "Num Lock";
    indicator 4 = "Group 2";
};
xkb_types "test" {
    virtual_modifiers NumLock, Alt, LevelThree;
    type "ONE_LEVEL" { modifiers = none; level_name[Level1] = "Any"; };
    type "TWO_LEVEL" { modifiers = Shift; map[Shift] = Level2; };
    type "ALPHABETIC" { modifiers = Shift+Lock; map[Shift] = Level2; map[Lock] = Level2; };
    type "KEYPAD" { modifiers = Shift+NumLock; map[Shift] = Level2; map[NumLock] = Level2; };
    type "FOUR_LEVEL" {
        modifiers = Shift+LevelThree;
        map[Shift] = Level2; map[LevelThree] = Level3; map[Shift+LevelThree] = Level4;
    };
    type "PRESERVE" {
        modifiers = Shift+Lock;
        map[Shift] = Level2;
        preserve[Lock] = Lock;
    };
};
xkb_compatibility "test" {
    virtual_modifiers NumLock, Alt, LevelThree;
    interpret.repeat = False;
    interpret Num_Lock+AnyOf(all) { virtualModifier = NumLock; action = LockMods(modifiers=NumLock); };
    interpret ISO_Level3_Shift+AnyOf(all) {
        virtualModifier = LevelThree;
        useModMapMods = level1;
        action = SetMods(modifiers=LevelThree,clearLocks);
    };
    interpret Alt_L+AnyOf(all) { virtualModifier = Alt; action = SetMods(modifiers=modMapMods,clearLocks); };
    interpret Any+AnyOf(all) { action = SetMods(modifiers=modMapMods,clearLocks); };
    indicator "Caps Lock" { whichModState = locked; modifiers = Lock; };
    indicator "Num Lock" { whichModState = locked; modifiers = NumLock; };
    indicator "Group 2" { groups = 0xfe; };
};
xkb_symbols "test" {
    name[Group1] = "English";
    name[Group2] = "Russian";
    key <ESC> { [ Escape ] };
    key <AE01> { [ 1, exclam ] };
    key <AD01> { [ q, Q ], [ Cyrillic_shorti, Cyrillic_SHORTI ] };
    key <AD02> { type = "FOUR_LEVEL", symbols[Group1] = [ w, W, lstroke, Lstroke ] };
    key <AD03> { type[Group1] = "PRESERVE", symbols[Group1] = [ e, E ] };
    key <RTRN> { [ Return ] };
    key <LCTL> { [ Control_L ] };
    key <LFSH> { [ Shift_L ] };
    key <LALT> { [ Alt_L ] };
    key <SPCE> { [ space ] };
    key <CAPS> { [ Caps_Lock ] };
    key <NMLK> { [ Num_Lock ] };
    key <KP1> { [ KP_End, KP_1 ] };
    key <RALT> { type = "ONE_LEVEL", symbols[Group1] = [ ISO_Level3_Shift ] };
    key <MULT> { repeat = No, groupsClamp, [ { H, i }, NoSymbol ] };
    key <Missing> { [ x ] };
    modifier_map Shift { <LFSH> };
    modifier_map Lock { <CAPS> };
    modifier_map Control { <LCTL> };
    modifier_map Mod1 { <LALT> };
    modifier_map Mod2 { Num_Lock };
    modifier_map Mod5 { <RALT> };
};
};
"#;

const SHIFT: u32 = 1;
const LOCK: u32 = 2;
const CONTROL: u32 = 4;
const MOD2: u32 = 0x10;
const MOD5: u32 = 0x80;

fn state() -> State {
    State::new(KEYMAP.parse().unwrap())
}

fn sym(name: &str) -> Keysym {
    Keysym::from_name(name).unwrap()
}

#[test]
fn keys_names_and_aliases() {
    let k: Keymap = KEYMAP.parse().unwrap();
    assert_eq!((k.min_keycode(), k.max_keycode()), (9, 201));
    assert_eq!(k.keycodes().count(), 16);
    assert_eq!(k.key_name(24), Some("AD01"));
    assert_eq!(k.key_name(23), None);
    assert_eq!(k.key_by_name("AD01"), Some(24));
    assert_eq!(k.key_by_name("LatQ"), Some(24));
    assert_eq!(
        k.key_by_name("Bad"),
        None,
        "an alias for a key that is not there"
    );
    assert_eq!(k.key_by_name("Missing"), None);
    assert_eq!(k.num_layouts(), 2);
    assert_eq!(k.layout_name(0), Some("English"));
    assert_eq!(k.layout_name(1), Some("Russian"));
    assert_eq!(k.layout_name(2), None);
    assert_eq!(k.layout_index("Russian"), Some(1));
    assert_eq!(k.num_layouts_for_key(24), 2);
    assert_eq!(k.num_layouts_for_key(9), 1);
    assert_eq!(k.num_layouts_for_key(201), 0, "a key without symbols");
    assert_eq!(k.num_levels_for_key(25, 0), 4);
    assert_eq!(k.key_syms_by_level(25, 0, 2), [sym("lstroke")]);
    assert_eq!(k.key_syms_by_level(25, 0, 4), []);
    assert_eq!(k.key_syms_by_level(24, 1, 1), [sym("Cyrillic_SHORTI")]);
    assert_eq!(
        k.key_syms_by_level(9, 1, 0),
        [sym("Escape")],
        "its one layout serves for the second"
    );
    assert_eq!(k.key_syms_by_level(200, 0, 0), [sym("H"), sym("i")]);
    assert_eq!(k.key_syms_by_level(200, 0, 1), []);
    assert_eq!(k.key_syms_by_level(7, 0, 0), []);
}

#[test]
fn modifiers_real_and_virtual() {
    let k: Keymap = KEYMAP.parse().unwrap();
    assert_eq!(k.num_mods(), 11);
    assert_eq!(k.mod_name(0), Some("Shift"));
    assert_eq!(k.mod_name(7), Some("Mod5"));
    assert_eq!(k.mod_name(8), Some("NumLock"));
    assert_eq!(k.mod_name(11), None);
    assert_eq!(k.mod_index("Control"), Some(2));
    assert_eq!(k.mod_index("LevelThree"), Some(10));
    assert_eq!(k.mod_index("Nope"), None);
    // Each virtual modifier is the real one of the key that carries it.
    assert_eq!(k.mod_mask(2), CONTROL);
    assert_eq!(
        k.mod_mask(8),
        MOD2,
        "NumLock, through a keysym in the modifier map"
    );
    assert_eq!(k.mod_mask(9), 0x08, "Alt");
    assert_eq!(k.mod_mask(10), MOD5, "LevelThree");
    assert_eq!(k.mod_mask(11), 0);
}

#[test]
fn a_virtual_modifier_can_be_given_its_real_one_outright() {
    let k: Keymap = r#"xkb_keymap {
        xkb_keycodes { <KP1> = 87; };
        xkb_types {
            virtual_modifiers NumLock = Mod2, Odd = 0x60;
            type "KEYPAD" { modifiers = Shift+NumLock; map[NumLock] = Level2; };
        };
        xkb_compatibility { };
        xkb_symbols { key <KP1> { type = "KEYPAD", [ KP_End, KP_1 ] }; };
    };"#
    .parse()
    .unwrap();
    assert_eq!(k.mod_mask(8), MOD2);
    assert_eq!(k.mod_mask(9), 0x60);
    let mut s = State::new(k);
    assert_eq!(s.key_one_sym(87), sym("KP_End"));
    s.update_mask(0, 0, MOD2, 0, 0, 0);
    assert_eq!(s.key_one_sym(87), sym("KP_1"));
    assert_eq!(s.key_utf8(87), "1");
}

#[test]
fn types_are_inferred_where_a_key_names_none() {
    let mut s = state();
    // A letter pair is ALPHABETIC: Caps Lock picks the second level.
    s.update_mask(0, 0, LOCK, 0, 0, 0);
    assert_eq!(s.key_level(24, 0), Some(1));
    assert_eq!(s.key_syms(24), [sym("Q")]);
    // Digit and punctuation are TWO_LEVEL: Caps Lock does nothing.
    assert_eq!(s.key_level(10, 0), Some(0));
    assert_eq!(s.key_utf8(10), "1");
    // Keypad symbols are KEYPAD: Num Lock picks the digit.
    assert_eq!(s.key_one_sym(87), sym("KP_End"));
    s.update_mask(0, 0, MOD2, 0, 0, 0);
    assert_eq!(s.key_one_sym(87), sym("KP_1"));
    s.update_mask(SHIFT, 0, 0, 0, 0, 0);
    assert_eq!(s.key_utf8(10), "!");
    assert_eq!(s.key_utf8(24), "Q");
}

#[test]
fn levels_consumed_modifiers_and_caps_lock() {
    let mut s = state();
    assert_eq!(s.key_consumed_mods(10), SHIFT);
    assert_eq!(s.key_consumed_mods(24), SHIFT | LOCK);
    assert_eq!(
        s.key_consumed_mods(25),
        SHIFT | MOD5,
        "the virtual modifier, as its real one"
    );
    assert_eq!(s.key_consumed_mods(201), 0);
    assert!(s.mod_is_consumed(10, 0) && !s.mod_is_consumed(10, 2));

    s.update_mask(MOD5, 0, 0, 0, 0, 0);
    assert_eq!(s.key_utf8(25), "ł");
    s.update_mask(MOD5 | SHIFT, 0, 0, 0, 0, 0);
    assert_eq!(s.key_utf8(25), "Ł");
    // A virtual modifier's own bit works like the real one.
    s.update_mask(1 << 10, 0, 0, 0, 0, 0);
    assert_eq!(s.mods(Component::Effective), 1 << 10 | MOD5);
    assert_eq!(s.key_utf8(25), "ł");
    // A bit the keymap has no modifier for is dropped.
    s.update_mask(1 << 20, 0, 0, 0, 0, 0);
    assert_eq!(s.mods(Component::Effective), 0);

    // FOUR_LEVEL does not use Lock, so Caps Lock upper-cases the keysym.
    s.update_mask(0, 0, LOCK, 0, 0, 0);
    assert_eq!(s.key_syms(25), [sym("w")]);
    assert_eq!(s.key_one_sym(25), sym("W"));
    assert_eq!(s.key_utf8(25), "W");
    // PRESERVE uses Lock to pick the level and hands it back.
    assert_eq!(s.key_level(26, 0), Some(0));
    assert_eq!(s.key_consumed_mods(26), SHIFT);
    assert_eq!(s.key_one_sym(26), sym("E"));
    s.update_mask(0, 0, 0, 0, 0, 0);
    assert_eq!(s.key_consumed_mods(26), SHIFT | LOCK);
    assert_eq!(s.key_one_sym(26), sym("e"));
}

#[test]
fn control_makes_control_characters() {
    let mut s = state();
    s.update_mask(CONTROL, 0, 0, 0, 0, 0);
    assert_eq!(s.key_utf8(24), "\u{11}", "Ctrl+Q");
    assert_eq!(s.key_utf32(24), 0x11);
    assert_eq!(s.key_one_sym(24), sym("q"), "the keysym is untouched");
    assert_eq!(s.key_utf8(65), "\0", "Ctrl+Space");
    assert_eq!(s.key_char(65), Some('\0'));
    assert_eq!(s.key_utf8(36), "\r");
    // On the second layout the key has no ASCII symbol; the first's is used.
    s.update_mask(CONTROL, 0, 0, 0, 0, 1);
    assert_eq!(s.key_syms(24), [sym("Cyrillic_shorti")]);
    assert_eq!(s.key_utf8(24), "\u{11}");
    s.update_mask(0, 0, 0, 0, 0, 1);
    assert_eq!(s.key_utf8(24), "й");
}

#[test]
fn layouts_wrap_clamp_and_redirect() {
    let mut s = state();
    s.update_mask(0, 0, 0, 0, 0, 1);
    assert_eq!(s.layout(), 1);
    assert_eq!(s.key_layout(24), Some(1));
    assert_eq!(s.key_layout(9), Some(0), "a key with one layout");
    assert_eq!(s.key_layout(201), None);
    assert_eq!(s.key_utf8(9), "\u{1b}");
    s.update_mask(0, 0, 0, 0, 0, 5);
    assert_eq!(s.layout(), 1, "wrapped into the keymap's two");
    s.update_mask(0, 0, 0, 1, 1, 1);
    assert_eq!(s.layout(), 1);
    s.update_mask(0, 0, 0, u32::MAX, 0, 0);
    assert_eq!(s.layout(), 1, "minus one wraps to the last");

    let k: Keymap = r#"xkb_keymap {
        xkb_keycodes { <A> = 10; <B> = 11; <C> = 12; <D> = 13; };
        xkb_types { type "ONE_LEVEL" { modifiers = none; }; };
        xkb_compatibility { };
        xkb_symbols {
            key <A> { [ a ], [ b ], [ c ], [ d ] };
            key <B> { groupsClamp, [ a ], [ b ] };
            key <C> { groupsRedirect = Group2, [ a ], [ b ], [ c ] };
            key <D> { [ a ], [ b ], [ c ] };
        };
    };"#
    .parse()
    .unwrap();
    assert_eq!(k.num_layouts(), 4);
    let mut s = State::new(k);
    s.update_mask(0, 0, 0, 0, 0, 3);
    let names: Vec<String> = (10..14).map(|kc| s.key_one_sym(kc).name()).collect();
    assert_eq!(
        names,
        ["d", "b", "b", "a"],
        "in range, clamped, redirected, wrapped"
    );
}

#[test]
fn several_keysyms_on_one_level() {
    let s = state();
    assert_eq!(s.key_syms(200), [sym("H"), sym("i")]);
    assert_eq!(s.key_one_sym(200), Keysym::NO_SYMBOL);
    assert_eq!(s.key_utf8(200), "Hi");
    assert_eq!(s.key_utf32(200), 0);
    assert_eq!(s.key_char(200), None);
}

#[test]
fn repeat_comes_from_the_key_or_its_interpret() {
    let k: Keymap = KEYMAP.parse().unwrap();
    assert!(k.key_repeats(24), "an ordinary key");
    assert!(k.key_repeats(9));
    assert!(!k.key_repeats(50), "a modifier: its interpret says no");
    assert!(!k.key_repeats(108));
    assert!(!k.key_repeats(200), "the key says no");
    assert!(!k.key_repeats(201), "nothing on it");
    assert!(!k.key_repeats(5000));
}

#[test]
fn leds() {
    let mut s = state();
    let k = s.keymap().clone();
    assert_eq!(k.num_leds(), 4);
    assert_eq!(k.led_name(0), Some("Caps Lock"));
    assert_eq!(k.led_name(2), None, "an unused slot");
    assert_eq!(k.led_index("Group 2"), Some(3));
    assert!(!s.led_is_active(0));
    s.update_mask(LOCK, 0, 0, 0, 0, 0);
    assert!(!s.led_is_active(0), "held, not locked");
    s.update_mask(0, 0, LOCK | MOD2, 0, 0, 0);
    assert!(s.led_is_active(0) && s.led_name_is_active("Num Lock"));
    assert!(!s.led_is_active(3) && !s.led_is_active(2) && !s.led_is_active(99));
    s.update_mask(0, 0, 0, 0, 0, 1);
    assert!(s.led_name_is_active("Group 2") && !s.led_name_is_active("Caps Lock"));
    assert!(!s.led_name_is_active("Nope"));
}

#[test]
fn which_modifiers_are_active() {
    let mut s = state();
    s.update_mask(SHIFT, CONTROL, LOCK, 0, 0, 0);
    assert_eq!(s.mods(Component::Depressed), SHIFT);
    assert_eq!(s.mods(Component::Latched), CONTROL);
    assert_eq!(s.mods(Component::Locked), LOCK);
    assert_eq!(s.mods(Component::Effective), SHIFT | CONTROL | LOCK);
    assert!(s.mod_name_is_active("Shift", Component::Depressed));
    assert!(!s.mod_name_is_active("Shift", Component::Locked));
    assert!(s.mod_name_is_active("Lock", Component::Effective));
    assert!(!s.mod_name_is_active("Mod1", Component::Effective));
    assert!(!s.mod_name_is_active("Nope", Component::Effective));
    assert!(s.mod_is_active(2, Component::Latched) && !s.mod_is_active(40, Component::Effective));
}

fn section(types: &str, compat: &str, symbols: &str) -> String {
    format!(
        "xkb_keymap {{\nxkb_keycodes {{ <A> = 10; }};\nxkb_types {{ {types} }};\n\
         xkb_compatibility {{ {compat} }};\nxkb_symbols {{ {symbols} }};\n}};"
    )
}

#[test]
fn what_is_wrong_and_where() {
    let fails = |text: &str| text.parse::<Keymap>().unwrap_err();
    for missing in [
        "xkb_keycodes",
        "xkb_types",
        "xkb_compatibility",
        "xkb_symbols",
    ] {
        let text = section("", "", "").replace(missing, "xkb_geometry");
        assert!(fails(&text).message.contains(missing), "{missing}");
    }
    let e = fails(&section("virtual_modifiers V = Shift+Nope;", "", ""));
    assert_eq!((e.line, e.column), (3, 13), "{e}");
    assert!(e.message.contains("Nope"), "{e}");
    let e = fails(&section("", "indicator \"x\" { modifiers = Nope; };", ""));
    assert!(e.message.contains("Nope"), "{e}");
    let e = fails(&section("", "indicator \"x\" { groups = Group0; };", ""));
    assert_eq!(e.message, "expected a group");
    let e = fails(&section("", "", "name[Group99] = \"x\";"));
    assert_eq!(
        (e.line, e.column, e.message.as_str()),
        (5, 15, "expected a group")
    );
    let e = fails(&section("virtual_modifiers Shift;", "", ""));
    assert!(e.message.contains("real modifier"), "{e}");
    let many: Vec<String> = (0..25).map(|i| format!("V{i}")).collect();
    let e = fails(&section(
        &format!("virtual_modifiers {};", many.join(",")),
        "",
        "",
    ));
    assert_eq!(e.message, "too many virtual modifiers");
    let e = fails(&section("", "interpret Any+Sometimes(Shift) { };", ""));
    assert_eq!(e.message, "unknown interpret match");
    let e = fails(&section("", "interpret.virtualModifier = Shift;", ""));
    assert!(e.message.contains("not a virtual modifier"), "{e}");
    let e = fails(&section("", "", "modifier_map Nope { <A> };"));
    assert!(e.message.contains("not a real modifier"), "{e}");
    let e = fails("xkb_keymap { xkb_keycodes { <A> = 0xffffffff; }; xkb_types { }; xkb_compatibility { }; xkb_symbols { }; };");
    assert_eq!(e.message, "keycode out of range");
    let e = fails("xkb_keymap { xkb_keycodes { indicator 33 = \"x\"; }; xkb_types { }; xkb_compatibility { }; xkb_symbols { }; };");
    assert_eq!(e.message, "indicator number out of range");
    assert_eq!(e.to_string(), "1:29: indicator number out of range");
    assert!(Keymap::from_bytes(b"\xff\xfe not a keymap").is_err());
}

#[test]
fn what_is_merely_odd_is_let_through() {
    // A keysym nobody knows is no symbol; a type nobody defined is the
    // first one; an interpret for an unknown keysym matches nothing; a key
    // defined twice is the two definitions merged, the later one winning
    // where both say something; a field of a type, a key or an interpret
    // that makes no sense is passed over.
    let k: Keymap = section(
        "type \"FIRST\" { modifiers = Shift; modifiers = Shift+Nope; map[Shift] = Level2; \
         map[Shift] = Level0; map[Lock] = Level9999; };",
        "interpret NoSuchKeysym { repeat = True; }; \
         interpret z { virtualModifier = Shift; repeat = Perhaps; };",
        "key <A> { [ z ] }; key <A> { type = \"NoSuchType\", [ NoSuchKeysym, b, c ] }; key <B> { [ x ] }; \
         key <A> { symbols[Group99] = [ a ], type = 5 };",
    )
    .parse()
    .unwrap();
    assert_eq!(
        k.num_levels_for_key(10, 0),
        2,
        "cut to the type's two levels"
    );
    assert_eq!(k.num_layouts_for_key(10), 1);
    assert_eq!(k.key_syms_by_level(10, 0, 0), [sym("z")]);
    assert_eq!(k.key_syms_by_level(10, 0, 1), [sym("b")]);
    // A keymap whose sections are empty is a keymap without keys.
    let empty: Keymap =
        "xkb_keymap { xkb_keycodes { }; xkb_types { }; xkb_compatibility { }; xkb_symbols { }; };"
            .parse()
            .unwrap();
    assert_eq!(
        (
            empty.min_keycode(),
            empty.max_keycode(),
            empty.num_layouts()
        ),
        (8, 255, 0),
        "what libxkbcommon answers for a keymap without keys"
    );
    let mut s = State::new(empty);
    s.update_mask(u32::MAX, u32::MAX, u32::MAX, 7, 7, 7);
    assert_eq!(s.layout(), 0);
    assert_eq!(s.key_utf8(10), "");
}

#[test]
fn a_keymap_cannot_ask_for_memory_out_of_proportion() {
    // Every key claims 32 layouts of 2048 levels.
    let mut codes = String::new();
    let mut keys = String::new();
    for i in 0..400 {
        codes.push_str(&format!("<K{i}> = {};", i + 8));
        keys.push_str(&format!("key <K{i}> {{ type[Group32] = \"BIG\", [ a ] }};"));
    }
    let text = format!(
        "xkb_keymap {{ xkb_keycodes {{ {codes} }}; xkb_types {{ type \"BIG\" {{ modifiers = Shift; map[Shift] = Level2048; }}; }}; \
         xkb_compatibility {{ }}; xkb_symbols {{ {keys} }}; }};"
    );
    assert_eq!(
        text.parse::<Keymap>().unwrap_err().message,
        "the keymap is too large"
    );
}

/// A small deterministic generator (xorshift), so the damage is the same
/// on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Read damaged text and, where it still reads, ask the keymap everything.
fn survive(bytes: &[u8]) -> bool {
    let Ok(keymap) = Keymap::from_bytes(bytes) else {
        return false;
    };
    let codes: Vec<u32> = keymap.keycodes().take(300).collect();
    let mut state = State::new(keymap);
    for (mods, group) in [(0, 0), (1, 0), (0x83, 1), (u32::MAX, u32::MAX), (4, 31)] {
        state.update_mask(mods, mods >> 1, mods >> 2, group, 0, group);
        for &kc in &codes {
            let _ = (
                state.key_syms(kc),
                state.key_one_sym(kc),
                state.key_utf8(kc),
                state.key_utf32(kc),
            );
            let _ = (
                state.key_consumed_mods(kc),
                state.key_repeats(kc),
                state.key_layout(kc),
            );
            for g in 0..state.keymap().num_layouts_for_key(kc) + 1 {
                let _ = (
                    state.key_level(kc, g),
                    state.keymap().num_levels_for_key(kc, g),
                );
            }
        }
        for i in 0..state.keymap().num_leds() + 1 {
            let _ = state.led_is_active(i);
        }
    }
    true
}

#[test]
fn damaged_keymaps_never_panic() {
    const PIECES: [&str; 28] = [
        "{",
        "}",
        "[",
        "]",
        "(",
        ")",
        ";",
        ",",
        "=",
        "+",
        "-",
        "!",
        "<",
        ">",
        "\"",
        "\\",
        ".",
        "0x",
        "99999999999",
        "Group32",
        "Level2048",
        "type",
        "key",
        "interpret",
        "virtual_modifiers",
        "modifier_map",
        "indicator",
        "\0",
    ];
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut read = 0;
    for source in [KEYMAP, include_str!("fixtures/us_ru.xkb")] {
        let bytes = source.as_bytes();
        // Cut short anywhere.
        for _ in 0..150 {
            let cut = rng.below(bytes.len());
            read += usize::from(survive(&bytes[..cut]));
        }
        // A few changes at random places: a piece of syntax put in, a
        // stretch taken out, a stretch doubled, a byte replaced.
        for _ in 0..600 {
            let mut damaged = bytes.to_vec();
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(damaged.len());
                match rng.below(4) {
                    0 => {
                        let piece = PIECES[rng.below(PIECES.len())].as_bytes();
                        damaged.splice(at..at, piece.iter().copied());
                    }
                    1 => {
                        let end = (at + rng.below(40)).min(damaged.len());
                        damaged.drain(at..end);
                    }
                    2 => {
                        let end = (at + rng.below(200)).min(damaged.len());
                        let copy = damaged[at..end].to_vec();
                        damaged.splice(at..at, copy);
                    }
                    _ => damaged[at] = rng.below(256) as u8,
                }
            }
            read += usize::from(survive(&damaged));
        }
    }
    // Some damage lands in a name or a comment and the keymap still reads.
    assert!(read > 0, "none of the damaged keymaps read");
}
