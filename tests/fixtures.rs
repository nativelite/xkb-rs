//! The reference library's keymaps for three layouts, and its answers for a
//! set of modifier states, recorded by `write_fixtures` in
//! `tests/differential.rs`. This checks the same answers on any platform.

use std::fmt::Write;
use xkb::{Keymap, State};

fn check(keymap: &str, expected: &str) -> usize {
    let keymap: Keymap = keymap.parse().unwrap();
    let mut state = State::new(keymap);
    let mut checked = 0;
    for line in expected.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let hex = |i: usize| u32::from_str_radix(f[i], 16).unwrap();
        let dec = |i: usize| f[i].parse::<u32>().unwrap();
        state.update_mask(hex(0), hex(1), hex(2), 0, 0, dec(3));
        let kc = dec(4);
        let layout = state.key_layout(kc).unwrap();
        let syms: Vec<String> = state
            .key_syms(kc)
            .iter()
            .map(|s| format!("{:x}", s.0))
            .collect();
        let utf8 = state.key_utf8(kc).bytes().fold(String::new(), |mut s, b| {
            write!(s, "{b:02x}").unwrap();
            s
        });
        let ours = format!(
            "{} {} {} {} {kc} {layout} {} {:x} {:x} {:x} {} {}",
            f[0],
            f[1],
            f[2],
            f[3],
            state.key_level(kc, layout).unwrap(),
            state.key_consumed_mods(kc),
            state.key_one_sym(kc).0,
            state.key_utf32(kc),
            if syms.is_empty() {
                "-".to_string()
            } else {
                syms.join(",")
            },
            if utf8.is_empty() {
                "-".to_string()
            } else {
                utf8
            },
        );
        assert_eq!(ours, line);
        checked += 1;
    }
    checked
}

#[test]
fn us() {
    let n = check(
        include_str!("fixtures/us.xkb"),
        include_str!("fixtures/us.expect"),
    );
    assert_eq!(n, 1464);
}

#[test]
fn de() {
    let n = check(
        include_str!("fixtures/de.xkb"),
        include_str!("fixtures/de.expect"),
    );
    assert_eq!(n, 1446);
}

#[test]
fn us_and_ru() {
    let n = check(
        include_str!("fixtures/us_ru.xkb"),
        include_str!("fixtures/us_ru.expect"),
    );
    assert_eq!(n, 2928);
}

#[test]
fn what_the_us_keymap_says_about_itself() {
    let keymap: Keymap = include_str!("fixtures/us.xkb").parse().unwrap();
    assert_eq!((keymap.min_keycode(), keymap.max_keycode()), (9, 708));
    assert_eq!(keymap.num_layouts(), 1);
    assert_eq!(keymap.layout_name(0), Some("English (US)"));
    assert_eq!(keymap.layout_index("English (US)"), Some(0));
    assert_eq!(keymap.key_by_name("AD01"), Some(24));
    assert_eq!(keymap.key_name(24), Some("AD01"));
    assert_eq!(keymap.key_by_name("LatQ"), Some(24), "an alias");
    assert_eq!(keymap.mod_index("Mod5"), Some(7));
    let num_lock = keymap.mod_index("NumLock").unwrap();
    assert_eq!(keymap.mod_mask(num_lock), 0x10, "NumLock is Mod2");
    let level3 = keymap.mod_index("LevelThree").unwrap();
    assert_eq!(keymap.mod_mask(level3), 0x80, "LevelThree is Mod5");
    assert_eq!(keymap.led_index("Caps Lock"), Some(0));
    assert!(
        keymap.key_repeats(24) && !keymap.key_repeats(50),
        "q repeats, Shift does not"
    );

    let mut state = State::new(keymap);
    assert_eq!(state.key_utf8(24), "q");
    assert!(state.update_mask(0, 0, 2, 0, 0, 0));
    assert!(!state.update_mask(0, 0, 2, 0, 0, 0), "nothing changed");
    assert_eq!(state.key_utf8(24), "Q");
    assert_eq!(state.key_utf8(10), "1", "Caps Lock leaves digits alone");
    assert!(state.led_name_is_active("Caps Lock") && !state.led_name_is_active("Num Lock"));
    state.update_mask(4, 0, 0, 0, 0, 0);
    assert_eq!(state.key_utf8(54), "\u{3}", "Ctrl+C");
    assert_eq!(state.key_char(54), Some('\u{3}'));
    assert_eq!(state.key_utf8(113), "", "an arrow types nothing");
    assert_eq!(state.key_char(113), None);
}

#[test]
fn control_reaches_through_a_cyrillic_layout() {
    let keymap: Keymap = include_str!("fixtures/us_ru.xkb").parse().unwrap();
    assert_eq!(keymap.num_layouts(), 2);
    assert_eq!(keymap.layout_name(1), Some("Russian"));
    let mut state = State::new(keymap);
    state.update_mask(0, 0, 0, 0, 0, 1);
    assert_eq!(state.layout(), 1);
    assert_eq!(state.key_utf8(54), "с");
    state.update_mask(4, 0, 0, 0, 0, 1);
    assert_eq!(state.key_one_sym(54).name(), "Cyrillic_es");
    assert_eq!(state.key_utf8(54), "\u{3}", "Ctrl+C all the same");
}

#[test]
fn the_bytes_of_a_keymap_file_read_too() {
    let mut bytes = include_bytes!("fixtures/de.xkb").to_vec();
    bytes.push(0);
    let keymap = Keymap::from_bytes(&bytes).unwrap();
    let mut state = State::new(keymap);
    assert_eq!(state.key_utf8(20), "ß");
    assert_eq!(state.key_one_sym(21).name(), "dead_acute");
    // AltGr (Mod5) + q is the at sign on a German keyboard.
    state.update_mask(0x80, 0, 0, 0, 0, 0);
    assert_eq!(state.key_utf8(24), "@");
}
