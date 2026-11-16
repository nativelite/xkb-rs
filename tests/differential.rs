//! Differential tests against the reference implementation: the system's
//! `libxkbcommon.so.0`, loaded at run time. Linux only, and skipped (with a
//! line saying so) where the library is not installed.
//!
//! Run with `--nocapture` to see how many comparisons were made.
#![cfg(target_os = "linux")]

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use xkb::{Component, Keymap, Keysym, State};

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

type P = *mut c_void;

#[repr(C)]
pub struct RuleNames {
    rules: *const c_char,
    model: *const c_char,
    layout: *const c_char,
    variant: *const c_char,
    options: *const c_char,
}

macro_rules! reference {
    ($( fn $name:ident($($arg:ty),*) $(-> $ret:ty)?; )*) => {
        /// The reference library's functions.
        pub struct Ref { $( pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?, )* }
        impl Ref {
            pub fn load() -> Option<Ref> {
                // SAFETY: dlopen and dlsym take NUL-terminated names; each
                // symbol is cast to the signature the library documents.
                unsafe {
                    let lib = dlopen(b"libxkbcommon.so.0\0".as_ptr().cast(), 2);
                    if lib.is_null() {
                        return None;
                    }
                    Some(Ref { $( $name: {
                        let p = dlsym(lib, concat!(stringify!($name), "\0").as_ptr().cast());
                        assert!(!p.is_null(), stringify!($name));
                        std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(p)
                    }, )* })
                }
            }
        }
    };
}

reference! {
    fn xkb_context_new(c_int) -> P;
    fn xkb_keymap_new_from_names(P, *const RuleNames, c_int) -> P;
    fn xkb_keymap_new_from_string(P, *const c_char, c_int, c_int) -> P;
    fn xkb_keymap_get_as_string(P, c_int) -> *mut c_char;
    fn xkb_keymap_unref(P);
    fn xkb_keymap_min_keycode(P) -> u32;
    fn xkb_keymap_max_keycode(P) -> u32;
    fn xkb_keymap_num_mods(P) -> u32;
    fn xkb_keymap_mod_get_name(P, u32) -> *const c_char;
    fn xkb_keymap_num_layouts(P) -> u32;
    fn xkb_keymap_layout_get_name(P, u32) -> *const c_char;
    fn xkb_keymap_num_leds(P) -> u32;
    fn xkb_keymap_led_get_name(P, u32) -> *const c_char;
    fn xkb_keymap_key_get_name(P, u32) -> *const c_char;
    fn xkb_keymap_num_layouts_for_key(P, u32) -> u32;
    fn xkb_keymap_num_levels_for_key(P, u32, u32) -> u32;
    fn xkb_keymap_key_get_syms_by_level(P, u32, u32, u32, *mut *const u32) -> c_int;
    fn xkb_keymap_key_repeats(P, u32) -> c_int;
    fn xkb_state_new(P) -> P;
    fn xkb_state_unref(P);
    fn xkb_state_update_mask(P, u32, u32, u32, u32, u32, u32) -> c_int;
    fn xkb_state_serialize_mods(P, c_int) -> u32;
    fn xkb_state_serialize_layout(P, c_int) -> u32;
    fn xkb_state_key_get_layout(P, u32) -> u32;
    fn xkb_state_key_get_level(P, u32, u32) -> u32;
    fn xkb_state_key_get_syms(P, u32, *mut *const u32) -> c_int;
    fn xkb_state_key_get_one_sym(P, u32) -> u32;
    fn xkb_state_key_get_utf8(P, u32, *mut c_char, usize) -> c_int;
    fn xkb_state_key_get_utf32(P, u32) -> u32;
    fn xkb_state_key_get_consumed_mods(P, u32) -> u32;
    fn xkb_state_led_index_is_active(P, u32) -> c_int;
    fn xkb_keysym_get_name(u32, *mut c_char, usize) -> c_int;
    fn xkb_keysym_from_name(*const c_char, c_int) -> u32;
    fn xkb_keysym_to_utf32(u32) -> u32;
    fn xkb_utf32_to_keysym(u32) -> u32;
    fn xkb_keysym_to_upper(u32) -> u32;
    fn xkb_keysym_to_lower(u32) -> u32;
}

fn load() -> Option<Ref> {
    let r = Ref::load();
    if r.is_none() {
        println!("SKIPPED: libxkbcommon.so.0 is not installed");
    }
    r
}

fn text(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // SAFETY: the library returns NUL-terminated strings.
    Some(unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned())
}

/// Collects mismatches, so one run shows them all (up to a limit).
#[derive(Default)]
struct Tally {
    compared: u64,
    wrong: Vec<String>,
    wrong_count: u64,
    kinds: std::collections::BTreeMap<String, u64>,
    /// Comparisons left out because this library cases a keysym by Unicode
    /// and the reference (1.6) by the older X11 tables.
    case_policy: u64,
    /// States left out because the reference computes a layout out of range
    /// for them (a negative layout that is a multiple of the count).
    reference_bug: u64,
}

impl Tally {
    fn eq<T: PartialEq + std::fmt::Debug>(
        &mut self,
        ours: T,
        theirs: T,
        what: impl FnOnce() -> String,
    ) {
        self.compared += 1;
        if ours != theirs {
            self.wrong_count += 1;
            // A few of each kind (the last word of the description).
            let what = what();
            let kind = what.rsplit(' ').next().unwrap_or("").to_string();
            let seen = self.kinds.entry(kind).or_insert(0);
            *seen += 1;
            if *seen <= 6 && self.wrong.len() < 60 {
                self.wrong
                    .push(format!("{what}: ours {ours:?}, reference {theirs:?}"));
            }
        }
    }

    fn finish(self, name: &str) {
        println!(
            "{name}: {} comparisons, {} mismatches {:?}; left out: {} for case policy, {} states for the reference's layout bug",
            self.compared, self.wrong_count, self.kinds, self.case_policy, self.reference_bug
        );
        assert!(
            self.wrong_count == 0,
            "{name}: {} mismatches:\n{}",
            self.wrong_count,
            self.wrong.join("\n")
        );
    }
}

/// Whether the reference calls a keysym lower case, upper case.
fn reference_case(r: &Ref, ks: Keysym) -> (bool, bool) {
    // SAFETY: no preconditions.
    let (low, up) = unsafe { ((r.xkb_keysym_to_lower)(ks.0), (r.xkb_keysym_to_upper)(ks.0)) };
    (low != up && ks.0 == low, low != up && ks.0 == up)
}

/// The reference's keymap for a layout, as text.
fn reference_keymap(r: &Ref, ctx: P, layout: &str, variant: &str, options: &str) -> Option<String> {
    let (rules, model) = (
        CString::new("evdev").unwrap(),
        CString::new("pc105").unwrap(),
    );
    let (layout, variant, options) = (
        CString::new(layout).unwrap(),
        CString::new(variant).unwrap(),
        CString::new(options).unwrap(),
    );
    let names = RuleNames {
        rules: rules.as_ptr(),
        model: model.as_ptr(),
        layout: layout.as_ptr(),
        variant: variant.as_ptr(),
        options: options.as_ptr(),
    };
    // SAFETY: the context is live and the names outlive the call.
    unsafe {
        let km = (r.xkb_keymap_new_from_names)(ctx, &names, 0);
        if km.is_null() {
            return None;
        }
        let s = text((r.xkb_keymap_get_as_string)(km, 1));
        (r.xkb_keymap_unref)(km);
        s
    }
}

fn syms_at(r: &Ref, km: P, kc: u32, layout: u32, level: u32) -> Vec<Keysym> {
    let mut p: *const u32 = std::ptr::null();
    // SAFETY: the keymap is live; the library sets `p` to `n` keysyms.
    unsafe {
        let n = (r.xkb_keymap_key_get_syms_by_level)(km, kc, layout, level, &mut p);
        (0..n.max(0) as usize).map(|i| Keysym(*p.add(i))).collect()
    }
}

/// Compare everything about one keymap text, under `states`, with the
/// reference reading the same text.
fn compare(r: &Ref, ctx: P, label: &str, source: &str, states: &[[u32; 6]], t: &mut Tally) {
    let c = CString::new(source).unwrap();
    // SAFETY: the context is live and the text NUL-terminated.
    let km = unsafe { (r.xkb_keymap_new_from_string)(ctx, c.as_ptr(), 1, 0) };
    assert!(
        !km.is_null(),
        "{label}: the reference rejects its own keymap"
    );
    let ours: Keymap = match source.parse() {
        Ok(k) => k,
        Err(e) => panic!("{label}: {e}"),
    };
    // SAFETY (for the whole function): `km` and the state made from it are
    // live until the end, and every call takes the arguments documented.
    unsafe {
        let (min, max) = (
            (r.xkb_keymap_min_keycode)(km),
            (r.xkb_keymap_max_keycode)(km),
        );
        t.eq(ours.min_keycode(), min, || format!("{label} min keycode"));
        t.eq(ours.max_keycode(), max, || format!("{label} max keycode"));
        let num_mods = (r.xkb_keymap_num_mods)(km);
        t.eq(ours.num_mods(), num_mods, || format!("{label} mods"));
        for i in 0..num_mods {
            t.eq(
                ours.mod_name(i).map(str::to_string),
                text((r.xkb_keymap_mod_get_name)(km, i)),
                || format!("{label} mod {i}"),
            );
        }
        let layouts = (r.xkb_keymap_num_layouts)(km);
        t.eq(ours.num_layouts(), layouts, || format!("{label} layouts"));
        for i in 0..layouts {
            t.eq(
                ours.layout_name(i).map(str::to_string),
                text((r.xkb_keymap_layout_get_name)(km, i)),
                || format!("{label} layout {i}"),
            );
        }
        let leds = (r.xkb_keymap_num_leds)(km);
        t.eq(ours.num_leds(), leds, || format!("{label} leds"));
        for i in 0..leds {
            t.eq(
                ours.led_name(i).map(str::to_string),
                text((r.xkb_keymap_led_get_name)(km, i)),
                || format!("{label} led {i}"),
            );
        }
        for kc in min.saturating_sub(1)..=max + 1 {
            t.eq(
                ours.key_name(kc).map(str::to_string),
                text((r.xkb_keymap_key_get_name)(km, kc)),
                || format!("{label} key {kc} name"),
            );
            t.eq(
                ours.key_repeats(kc),
                (r.xkb_keymap_key_repeats)(km, kc) != 0,
                || format!("{label} key {kc} {:?} repeats", ours.key_name(kc)),
            );
            let groups = (r.xkb_keymap_num_layouts_for_key)(km, kc);
            t.eq(ours.num_layouts_for_key(kc), groups, || {
                format!("{label} key {kc} layouts")
            });
            for g in 0..groups + 1 {
                let levels = (r.xkb_keymap_num_levels_for_key)(km, kc, g);
                t.eq(ours.num_levels_for_key(kc, g), levels, || {
                    format!("{label} key {kc} layout {g} levels")
                });
                for l in 0..levels + 1 {
                    t.eq(
                        ours.key_syms_by_level(kc, g, l).to_vec(),
                        syms_at(r, km, kc, g, l),
                        || format!("{label} key {kc} layout {g} level {l}"),
                    );
                }
            }
        }

        // A key without a type of its own gets one from the case of its first
        // keysyms. Where the two libraries case one of them differently
        // (Unicode here, the X11 tables in the reference), the key may get
        // another type; such keys are counted and not compared further.
        let type_may_differ: Vec<bool> = (min..=max)
            .map(|kc| {
                (0..ours.num_layouts_for_key(kc)).any(|g| {
                    (0..ours.num_levels_for_key(kc, g).min(4)).any(|l| {
                        ours.key_syms_by_level(kc, g, l)
                            .first()
                            .is_some_and(|s| (s.is_lower(), s.is_upper()) != reference_case(r, *s))
                    })
                })
            })
            .collect();

        let st = (r.xkb_state_new)(km);
        let mut state = State::new(ours);
        let mut buf = [0 as c_char; 64];
        for s in states {
            // The reference wraps a negative layout that is a multiple of the
            // layout count to the count itself, one past the last layout.
            let locked = i64::from(s[5] as i32).rem_euclid(i64::from(layouts.max(1)));
            let sum = i64::from(s[3] as i32) + i64::from(s[4] as i32) + locked;
            if sum < 0 && sum % i64::from(layouts.max(1)) == 0 {
                t.reference_bug += 1;
                continue;
            }
            (r.xkb_state_update_mask)(st, s[0], s[1], s[2], s[3], s[4], s[5]);
            state.update_mask(s[0], s[1], s[2], s[3], s[4], s[5]);
            let at = |what: &str| format!("{label} state {s:x?} {what}");
            for (ours, theirs) in [
                (Component::Depressed, 1),
                (Component::Latched, 2),
                (Component::Locked, 4),
                (Component::Effective, 8),
            ] {
                t.eq(
                    state.mods(ours),
                    (r.xkb_state_serialize_mods)(st, theirs),
                    || at("mods"),
                );
            }
            t.eq(
                state.layout(),
                (r.xkb_state_serialize_layout)(st, 128),
                || at("layout"),
            );
            for i in 0..leds {
                t.eq(
                    state.led_is_active(i),
                    (r.xkb_state_led_index_is_active)(st, i) > 0,
                    || at(&format!("led {i}")),
                );
            }
            for kc in min..=max {
                if type_may_differ[(kc - min) as usize] {
                    t.case_policy += 7;
                    continue;
                }
                let at = |what: &str| {
                    format!(
                        "{label} state {s:x?} key {kc} {:?} {what}",
                        state.keymap().key_name(kc)
                    )
                };
                let layout = (r.xkb_state_key_get_layout)(st, kc);
                t.eq(state.key_layout(kc).unwrap_or(u32::MAX), layout, || {
                    at("layout")
                });
                if layout != u32::MAX {
                    t.eq(
                        state.key_level(kc, layout).unwrap_or(u32::MAX),
                        (r.xkb_state_key_get_level)(st, kc, layout),
                        || at("level"),
                    );
                }
                let mut p: *const u32 = std::ptr::null();
                let n = (r.xkb_state_key_get_syms)(st, kc, &mut p);
                let theirs: Vec<Keysym> =
                    (0..n.max(0) as usize).map(|i| Keysym(*p.add(i))).collect();
                t.eq(state.key_syms(kc).to_vec(), theirs, || at("syms"));
                t.eq(
                    state.key_consumed_mods(kc),
                    (r.xkb_state_key_get_consumed_mods)(st, kc),
                    || at("consumed"),
                );
                // Caps Lock upper-cases the keysym: comparable only where
                // both libraries have the same upper case for it.
                if state
                    .key_syms(kc)
                    .iter()
                    .any(|s| s.to_upper().0 != (r.xkb_keysym_to_upper)(s.0))
                {
                    t.case_policy += 3;
                    continue;
                }
                t.eq(
                    state.key_one_sym(kc),
                    Keysym((r.xkb_state_key_get_one_sym)(st, kc)),
                    || at("one sym"),
                );
                let n = (r.xkb_state_key_get_utf8)(st, kc, buf.as_mut_ptr(), buf.len());
                let theirs: Vec<u8> = buf[..n.clamp(0, 63) as usize]
                    .iter()
                    .map(|&b| b as u8)
                    .collect();
                t.eq(state.key_utf8(kc).into_bytes(), theirs, || at("utf8"));
                t.eq(
                    state.key_utf32(kc),
                    (r.xkb_state_key_get_utf32)(st, kc),
                    || at("utf32"),
                );
            }
        }
        (r.xkb_state_unref)(st);
        (r.xkb_keymap_unref)(km);
    }
}

/// Modifier and layout states: every real-modifier mask in the first two
/// layouts, a spread of masks split over depressed, latched and locked,
/// each virtual modifier, and layouts in and out of range.
fn full_states() -> Vec<[u32; 6]> {
    let mut v = Vec::new();
    for m in 0..256 {
        v.push([m, 0, 0, 0, 0, 0]);
        v.push([0, 0, m, 0, 0, 1]);
    }
    for m in [
        0x01, 0x02, 0x03, 0x04, 0x08, 0x10, 0x80, 0x81, 0x82, 0x84, 0x90, 0xff,
    ] {
        v.push([0, m, 0, 0, 0, 0]);
        v.push([m & 0x0f, m & 0x30, m & 0xc0, 1, 0, 0]);
        for g in [2, 3, 4, 5, 7, u32::MAX] {
            v.push([m, 0, 0, 0, 0, g]);
            v.push([m, 0, 0, g, 1, 0]);
        }
        v.push([m, 0, 0, 1, 1, 1]);
        v.push([m, 0, 0, u32::MAX, 0, 1]);
    }
    for bit in 8..32 {
        v.push([1 << bit, 0, 0, 0, 0, 0]);
        v.push([0, 0, 1 << bit | 1, 0, 0, 0]);
    }
    v.push([u32::MAX, 0, 0, 0, 0, 0]);
    v
}

/// A smaller set, for the sweep over every layout.
fn quick_states() -> Vec<[u32; 6]> {
    let mut v = Vec::new();
    for m in [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x08, 0x10, 0x11, 0x12, 0x80, 0x81, 0x82, 0x83, 0x84,
        0x90, 0x92, 0xa0, 0xff,
    ] {
        v.push([m, 0, 0, 0, 0, 0]);
        v.push([0, 0, m, 0, 0, 1]);
    }
    v
}

const LAYOUTS: [(&str, &str, &str); 14] = [
    ("us", "", ""),
    ("de", "", ""),
    ("fr", "", ""),
    ("ru", "", ""),
    ("gb", "", ""),
    ("us,ru", "", "grp:alt_shift_toggle"),
    ("de", "neo", ""),
    ("us", "intl", ""),
    (
        "us,de,fr,ru",
        ",neo,,",
        "grp:win_space_toggle,lv3:ralt_switch",
    ),
    ("jp", "", ""),
    ("us", "", "caps:escape,compose:menu,ctrl:nocaps"),
    ("gr,il,ara", "", "grp:caps_toggle"),
    ("ca", "multix", ""),
    ("us", "dvorak", "caps:ctrl_modifier,lv5:rwin_switch_lock"),
];

#[test]
fn keymaps_and_state_match_the_reference() {
    let Some(r) = load() else { return };
    // SAFETY: no preconditions.
    let ctx = unsafe { (r.xkb_context_new)(0) };
    assert!(!ctx.is_null());
    let mut t = Tally::default();
    let states = full_states();
    for (layout, variant, options) in LAYOUTS {
        let label = format!("{layout}({variant}) {options}");
        let source = reference_keymap(&r, ctx, layout, variant, options)
            .unwrap_or_else(|| panic!("{label}: the reference cannot compile it"));
        compare(&r, ctx, &label, &source, &states, &mut t);
    }
    t.finish("keymaps and state, 14 layouts in full");
}

/// Every layout and variant the system's rules list.
fn all_layouts() -> Vec<(String, String)> {
    let Ok(list) = std::fs::read_to_string("/usr/share/X11/xkb/rules/evdev.lst") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut section = "";
    for line in list.lines() {
        if let Some(s) = line.strip_prefix("! ") {
            section = s.trim();
            continue;
        }
        let mut words = line.split_whitespace();
        match (section, words.next(), words.next()) {
            ("layout", Some(l), _) => out.push((l.to_string(), String::new())),
            ("variant", Some(v), Some(l)) => {
                out.push((l.trim_end_matches(':').to_string(), v.to_string()))
            }
            _ => {}
        }
    }
    out
}

#[test]
fn every_layout_of_the_system_matches_the_reference() {
    let Some(r) = load() else { return };
    // SAFETY: no preconditions.
    let ctx = unsafe { (r.xkb_context_new)(0) };
    let mut t = Tally::default();
    let states = quick_states();
    let mut done = 0;
    for (layout, variant) in all_layouts() {
        // With "us" as a second layout, so each is also read as group 1 of 2.
        let Some(source) =
            reference_keymap(&r, ctx, &format!("{layout},us"), &format!("{variant},"), "")
        else {
            continue;
        };
        compare(
            &r,
            ctx,
            &format!("{layout}({variant}),us"),
            &source,
            &states,
            &mut t,
        );
        done += 1;
    }
    t.finish(&format!(
        "keymaps and state, all {done} layouts and variants of the system"
    ));
}

#[test]
fn keysyms_match_the_reference() {
    let Some(r) = load() else { return };
    let mut t = Tally::default();
    let mut buf = [0 as c_char; 128];
    let ranges = [
        (0u32, 0x10_0000u32),
        (0x0100_0000, 0x0111_0000),
        (0x1000_0000, 0x1009_0000),
        (0x1fff_ff00, 0x2000_0000),
    ];
    for (lo, hi) in ranges {
        for ks in lo..hi {
            // SAFETY: the buffer is as long as said.
            let n = unsafe { (r.xkb_keysym_get_name)(ks, buf.as_mut_ptr(), buf.len()) };
            assert!(n > 0, "{ks:#x}");
            let name = text(buf.as_ptr()).unwrap();
            t.eq(Keysym(ks).name(), name.clone(), || {
                format!("name of {ks:#x}")
            });
            t.eq(Keysym::from_name(&name), Some(Keysym(ks)), || {
                format!("from_name({name})")
            });
            // SAFETY: no preconditions.
            let cp = unsafe { (r.xkb_keysym_to_utf32)(ks) };
            // The reference of this version lets surrogates through; a
            // surrogate is not a character.
            let cp = if (0xd800..=0xdfff).contains(&cp) {
                0
            } else {
                cp
            };
            t.eq(Keysym(ks).to_utf32(), cp, || {
                format!("utf32 of {ks:#x} {name}")
            });
        }
    }
    for cp in (0..0x11_0000).filter_map(char::from_u32) {
        // SAFETY: no preconditions.
        let theirs = unsafe { (r.xkb_utf32_to_keysym)(cp as u32) };
        t.eq(Keysym::from_char(cp), Keysym(theirs), || {
            format!("keysym of U+{:04X}", cp as u32)
        });
    }
    for name in [
        "",
        "a",
        "A",
        "U",
        "u",
        "U0",
        "U1f",
        "U20",
        "U7e",
        "U7f",
        "U9f",
        "Ua0",
        "UA0",
        "Uff",
        "U100",
        "U0100",
        "U10FFFF",
        "U110000",
        "U0010FFFF",
        "U00000041",
        "U000000041",
        "U+41",
        "U 41",
        "U41 ",
        "Ug",
        "U-1",
        "0x",
        "0x0",
        "0x41",
        "0X41",
        "0x1fffffff",
        "0x20000000",
        "0xffffffff",
        "0x100000000",
        "0x 1",
        "0x+1",
        "0xg",
        "XF86_AudioMute",
        "XF86_",
        "XF86_NoSuchKey",
        "xf86_audiomute",
        "return",
        "Return",
        "NoSymbol",
        "VoidSymbol",
        "dead_perispomeni",
        "KP_Space",
        "space",
        " space",
        "space ",
        "1",
        "0",
        "minus",
        "-",
        "\u{e9}",
        "eacute",
    ] {
        let c = CString::new(name).unwrap();
        // SAFETY: the name is NUL-terminated.
        let theirs = unsafe { (r.xkb_keysym_from_name)(c.as_ptr(), 0) };
        // The reference has one value for "no such name" and for NoSymbol.
        t.eq(
            Keysym::from_name(name).unwrap_or(Keysym::NO_SYMBOL),
            Keysym(theirs),
            || format!("from_name({name:?})"),
        );
    }
    t.finish("keysym names and characters");
}

/// Case conversion follows Unicode here; the reference of this version
/// (1.6) uses the older X11 tables, which know fewer letters, "convert"
/// numbers that are no keysym, and give a few letters (`mu`, `ssharp`,
/// `ydiaeresis`) a result that is no keysym. Wherever the reference turns a
/// character into another character, the two must agree.
#[test]
fn keysym_case_agrees_with_the_reference_where_it_has_one() {
    let Some(r) = load() else { return };
    let mut t = Tally::default();
    let (mut same, mut only_ours, mut quirks) = (0u64, 0u64, 0u64);
    let ranges = [
        (0u32, 0x10_0000u32),
        (0x0100_0000, 0x0111_0000),
        (0x1000_0000, 0x1009_0000),
    ];
    for (lo, hi) in ranges {
        for ks in lo..hi {
            // SAFETY: no preconditions.
            let (up, low) = unsafe { ((r.xkb_keysym_to_upper)(ks), (r.xkb_keysym_to_lower)(ks)) };
            for (ours, theirs, what) in [
                (Keysym(ks).to_upper(), up, "upper"),
                (Keysym(ks).to_lower(), low, "lower"),
            ] {
                let characters =
                    Keysym(ks).to_char().is_some() && Keysym(theirs).to_char().is_some();
                if theirs != ks && !characters {
                    quirks += 1;
                } else if theirs != ks {
                    t.eq(ours, Keysym(theirs), || {
                        format!("{} {what}", Keysym(ks).name())
                    });
                } else if ours.0 != ks {
                    only_ours += 1;
                } else {
                    same += 1;
                }
            }
        }
    }
    println!("case: {same} unchanged in both, {only_ours} cased only by Unicode, {quirks} reference results that are no character");
    t.finish("keysym case, where the reference changes the keysym");
}

/// Write the fixtures of `tests/fixtures.rs`: three of the reference's
/// keymaps and, beside each, what the reference answers for a set of states
/// (so the same answers can be checked where the reference is not
/// installed). Run by hand:
///
///     cargo test --test differential write_fixtures -- --ignored
#[test]
#[ignore = "rewrites tests/fixtures from the reference library"]
fn write_fixtures() {
    let r = load().expect("the reference library");
    // SAFETY: no preconditions.
    let ctx = unsafe { (r.xkb_context_new)(0) };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for (file, layout, options) in [
        ("us", "us", ""),
        ("de", "de", ""),
        ("us_ru", "us,ru", "grp:alt_shift_toggle"),
    ] {
        let source = reference_keymap(&r, ctx, layout, "", options).unwrap();
        std::fs::write(dir.join(format!("{file}.xkb")), &source).unwrap();
        let ours: Keymap = source.parse().unwrap();
        let c = CString::new(source).unwrap();
        let mut out = String::new();
        // SAFETY: as in `compare`.
        unsafe {
            let km = (r.xkb_keymap_new_from_string)(ctx, c.as_ptr(), 1, 0);
            let st = (r.xkb_state_new)(km);
            let mut buf = [0 as c_char; 64];
            for group in 0..(r.xkb_keymap_num_layouts)(km) {
                for (d, l, k) in FIXTURE_MODS {
                    (r.xkb_state_update_mask)(st, d, l, k, 0, 0, group);
                    for kc in 9..136 {
                        let layout = (r.xkb_state_key_get_layout)(st, kc);
                        if layout == u32::MAX {
                            continue;
                        }
                        let mut p: *const u32 = std::ptr::null();
                        let n = (r.xkb_state_key_get_syms)(st, kc, &mut p);
                        let syms: Vec<Keysym> =
                            (0..n.max(0) as usize).map(|i| Keysym(*p.add(i))).collect();
                        // Leave out what depends on the one policy that
                        // differs from the reference (see `compare`).
                        let type_may_differ = (0..ours.num_layouts_for_key(kc)).any(|g| {
                            (0..ours.num_levels_for_key(kc, g).min(4)).any(|l| {
                                ours.key_syms_by_level(kc, g, l).first().is_some_and(|s| {
                                    (s.is_lower(), s.is_upper()) != reference_case(&r, *s)
                                })
                            })
                        });
                        if type_may_differ
                            || syms
                                .iter()
                                .any(|s| s.to_upper().0 != (r.xkb_keysym_to_upper)(s.0))
                        {
                            continue;
                        }
                        let n = (r.xkb_state_key_get_utf8)(st, kc, buf.as_mut_ptr(), buf.len());
                        let utf8: String = buf[..n.clamp(0, 63) as usize]
                            .iter()
                            .map(|&b| format!("{:02x}", b as u8))
                            .collect();
                        let syms: Vec<String> = syms.iter().map(|s| format!("{:x}", s.0)).collect();
                        out.push_str(&format!(
                            "{d:x} {l:x} {k:x} {group} {kc} {layout} {} {:x} {:x} {:x} {} {}\n",
                            (r.xkb_state_key_get_level)(st, kc, layout),
                            (r.xkb_state_key_get_consumed_mods)(st, kc),
                            (r.xkb_state_key_get_one_sym)(st, kc),
                            (r.xkb_state_key_get_utf32)(st, kc),
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
                        ));
                    }
                }
            }
        }
        std::fs::write(dir.join(format!("{file}.expect")), out).unwrap();
    }
}

/// The modifier states of the fixtures: depressed, latched, locked.
const FIXTURE_MODS: [(u32, u32, u32); 12] = [
    (0, 0, 0),
    (1, 0, 0),
    (0, 0, 2),
    (1, 0, 2),
    (4, 0, 0),
    (5, 0, 0),
    (8, 0, 0),
    (0, 0, 0x10),
    (0x80, 0, 0),
    (0x81, 0, 0),
    (0x80, 0, 2),
    (0, 1, 0x10),
];
