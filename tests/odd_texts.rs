//! Keymap text that libxkbcommon's own writer never produces, and text
//! made to cost more than its size: what `xkbcomp` writes, what a person
//! types, what a hostile peer might send. The expectations here are what
//! libxkbcommon 1.6 answers for the same text; `tests/differential.rs`
//! checks that against the library itself where it is installed.

use std::time::{Duration, Instant};
use xkb::{Keymap, Keysym, State};

#[path = "handwritten/cases.rs"]
mod handwritten;

const LOCK: u32 = 2;
const CONTROL: u32 = 4;
const MOD1: u32 = 8;
const MOD3: u32 = 0x20;
const MOD4: u32 = 0x40;

fn sym(name: &str) -> Keysym {
    Keysym::from_name(name).unwrap_or_else(|| panic!("{name}"))
}

/// A keymap with keys `<A>` to `<F>` (keycodes 10 to 15), the usual small
/// types, and the given additions to each section.
fn keymap(types: &str, compat: &str, symbols: &str) -> Keymap {
    let text = format!(
        "xkb_keymap {{\n\
         xkb_keycodes {{ <A> = 10; <B> = 11; <C> = 12; <D> = 13; <E> = 14; <F> = 15; \
           alias <ALSO> = <A>; indicator 1 = \"Caps Lock\"; }};\n\
         xkb_types {{ virtual_modifiers Alt, NumLock; \
           type \"ONE_LEVEL\" {{ modifiers = none; }}; \
           type \"TWO_LEVEL\" {{ modifiers = Shift; map[Shift] = Level2; }}; \
           type \"ALPHABETIC\" {{ modifiers = Shift+Lock; map[Shift] = Level2; map[Lock] = Level2; }}; \
           {types} }};\n\
         xkb_compatibility {{ virtual_modifiers Alt, NumLock; \
           interpret Alt_L+AnyOf(all) {{ virtualModifier = Alt; }}; \
           {compat} }};\n\
         xkb_symbols {{ {symbols} }};\n}};"
    );
    text.parse().unwrap_or_else(|e| panic!("{e}\n{text}"))
}

fn syms(k: &Keymap, keycode: u32, layout: u32, level: u32) -> Vec<Keysym> {
    k.key_syms_by_level(keycode, layout, level).to_vec()
}

#[test]
fn defaults_for_keys_apply_to_the_keys_after_them() {
    let k = keymap(
        "",
        "",
        "key <A> { [ 1, exclam ], [ 2 ], [ 3 ] }; \
         key.type = \"ALPHABETIC\"; key.repeat = no; key.groupsClamp = true; \
         key <B> { [ 1, exclam ] }; key <C> { [ q ], [ w ] };",
    );
    assert!(k.key_repeats(10) && !k.key_repeats(11));
    let mut s = State::new(k);
    s.update_mask(0, 0, LOCK, 0, 0, 0);
    assert_eq!(s.key_one_sym(10), sym("1"), "TWO_LEVEL: Lock picks nothing");
    assert_eq!(s.key_one_sym(11), sym("exclam"), "ALPHABETIC by default");
    // The keymap's third layout, on a key with two: clamped to its last,
    // not wrapped to its first.
    s.update_mask(0, 0, 0, 0, 0, 2);
    assert_eq!(s.key_one_sym(10), sym("3"));
    assert_eq!(s.key_one_sym(12), sym("w"));
}

#[test]
fn a_key_with_more_than_four_levels_and_no_type_gets_the_first_type() {
    // The four-level types are there, and four levels would get one; five
    // get none of them.
    let four = "modifiers = Shift+Control; map[Shift] = Level2; map[Control] = Level3; \
                map[Shift+Control] = Level4;";
    let k = keymap(
        &format!(
            "type \"FOUR_LEVEL\" {{ {four} }}; type \"FOUR_LEVEL_ALPHABETIC\" {{ {four} }}; \
             type \"FOUR_LEVEL_SEMIALPHABETIC\" {{ {four} }};"
        ),
        "",
        "key <A> { [ a, A, b, B, c ] }; key <B> { [ a, A, b, B ] };",
    );
    assert_eq!(k.num_levels_for_key(10, 0), 1, "ONE_LEVEL is first");
    assert_eq!(syms(&k, 10, 0, 0), [sym("a")]);
    assert_eq!(k.num_levels_for_key(11, 0), 4);
}

#[test]
fn modifier_map_entries_for_one_key_add_up() {
    let k = keymap(
        "",
        "",
        "key <A> { [ Alt_L, Meta_L ] }; key <B> { [ Hyper_L ] }; key <C> { [ x ] }; \
         modifier_map Mod1 { Alt_L }; modifier_map Mod4 { Meta_L }; \
         modifier_map Mod3 { <B> }; modifier_map Mod4 { Hyper_L }; \
         modifier_map Mod3 { <C> }; modifier_map None { <C> }; \
         modifier_map Mod1 { <D>, <D>, 0x78 }; modifier_map Mod3 { <D> };",
    );
    let alt = k.mod_index("Alt").unwrap();
    // <A> carries Mod1 and Mod4, so Alt (its virtual modifier) is both.
    assert_eq!(k.mod_mask(alt), MOD1 | MOD4);
    // The same entry again changes its modifier (<D>: Mod3, not Mod1), and
    // `None` takes an entry's modifier away (<C>). A keysym may be a number
    // (0x78 is x, on <C>).
    let k = keymap(
        "type \"M1\" { modifiers = Mod1; map[Mod1] = Level2; }; \
         type \"M3\" { modifiers = Mod3; map[Mod3] = Level2; };",
        "interpret Any+Exactly(Mod1) { virtualModifier = NumLock; };",
        "key <C> { [ x ] }; key <D> { [ y ] }; \
         modifier_map Mod3 { <C> }; modifier_map None { <C> }; \
         modifier_map Mod1 { <D> }; modifier_map Mod3 { <D> }; modifier_map Mod1 { 0x78 };",
    );
    let num = k.mod_index("NumLock").unwrap();
    assert_eq!(k.mod_mask(num), MOD1, "only <C>, by its keysym, has Mod1");
}

#[test]
fn a_key_given_twice_is_merged_as_its_merge_mode_says() {
    let k = keymap(
        "",
        "",
        "key <A> { [ a, A ], [ b, B ] }; key <A> { symbols[Group2] = [ c ] }; \
         key <B> { type = \"TWO_LEVEL\", [ a, A ] }; key <B> { repeat = no }; \
         key <C> { [ a ] }; augment key <C> { [ b, c ] }; \
         key <D> { [ a, A ], [ d ] }; replace key <D> { [ e ] }; \
         key <E> { [ a ] }; key <ALSO> { [ z ] }; \
         key <F> { [ q, Q ] }; override key <F> { [ w ] };",
    );
    assert_eq!(k.num_layouts_for_key(10), 2);
    assert_eq!(syms(&k, 10, 0, 1), [sym("A")], "the first layout is kept");
    assert_eq!(
        syms(&k, 10, 1, 0),
        [sym("c")],
        "the later statement wins a level"
    );
    assert_eq!(syms(&k, 10, 1, 1), [sym("B")], "and leaves what it lacks");
    // Through its alias, <A> was given once more.
    assert_eq!(syms(&k, 10, 0, 0), [sym("z")]);
    assert_eq!(syms(&k, 11, 0, 1), [sym("A")]);
    assert!(!k.key_repeats(11), "the second statement only said repeat");
    // `augment`: what was there stays; what was not is added.
    assert_eq!(syms(&k, 12, 0, 0), [sym("a")]);
    assert_eq!(syms(&k, 12, 0, 1), [sym("c")]);
    // `replace`: nothing of the first statement is left.
    assert_eq!(k.num_layouts_for_key(13), 1);
    assert_eq!(syms(&k, 13, 0, 0), [sym("e")]);
    assert_eq!(syms(&k, 15, 0, 0), [sym("w")]);
    assert_eq!(syms(&k, 15, 0, 1), [sym("Q")]);

    // Types and keycodes too: the later one wins unless it says `augment`.
    let k = keymap(
        "type \"T\" { modifiers = Lock; map[Lock] = Level2; }; \
         augment type \"T\" { modifiers = Control; map[Control] = Level3; }; \
         type \"U\" { modifiers = Lock; map[Lock] = Level2; }; \
         type \"U\" { modifiers = Control; map[Control] = Level3; };",
        "",
        "key <A> { type = \"T\", [ a, b, c ] }; key <B> { type = \"U\", [ a, b, c ] };",
    );
    assert_eq!(
        (k.num_levels_for_key(10, 0), k.num_levels_for_key(11, 0)),
        (2, 3)
    );
}

#[test]
fn groups_and_levels_given_only_actions_or_a_type_count() {
    let k = keymap(
        "type \"FOUR\" { modifiers = Shift+Control; map[Shift] = Level2; map[Control] = Level3; \
           map[Shift+Control] = Level4; };",
        "",
        "key <A> { [ c ], actions[Group2] = [ NoAction() ] }; \
         key <B> { [ Num_Lock, a ], actions[Group1] = [ NoAction(), NoAction(), NoAction(), NoAction() ] }; \
         key <C> { type[Group3] = \"TWO_LEVEL\" }; \
         key <D> { [ ] }; key <E> { [ a ], [ ] }; key <F> { };",
    );
    assert_eq!(k.num_layouts(), 3);
    assert_eq!(k.num_layouts_for_key(10), 2);
    // Four actions make four levels, so no two-level type is worked out.
    assert_eq!(k.num_levels_for_key(11, 0), 1, "FOUR_LEVEL is not defined");
    assert_eq!(k.num_layouts_for_key(12), 3);
    assert_eq!(k.num_layouts_for_key(13), 1, "an empty list is a layout");
    assert_eq!(k.num_layouts_for_key(14), 2);
    assert_eq!(k.num_layouts_for_key(15), 0);
    let mut s = State::new(k);
    s.update_mask(0, 0, 0, 0, 0, 1);
    assert_eq!(s.layout(), 1);
    assert!(s.key_syms(14).is_empty(), "the empty second layout");
}

#[test]
fn types_take_their_fields_as_libxkbcommon_does() {
    let k = keymap(
        "type \"J\" { modifiers = 0xffffffff; map[0x80000000] = Level2; }; \
         type \"H\" { modifiers = Shift; map[Shift] = Level2; level_name[Level5] = \"five\"; }; \
         type \"E\" { modifiers = Shift+Lock; map[Shift] = 1+1; map[Lock] = 2*2-1; }; \
         type \"B\" { modifiers = !Shift; map[Lock] = Level2; map[Control+Lock] = 3; }; \
         type \"I\" { modifiers = ~Shift; map[Lock] = Level2; map[Control+Lock] = 3; }; \
         type \"M\" { modifiers = Shift; modifiers = Lock; map[Shift] = Level2; map[Lock] = 3; \
           map[Shift] = Level0; map[Nope] = Level2; };",
        "",
        "key <A> { type = \"J\", [ a, b ] }; key <B> { type = \"H\", [ a, b, c, d, e ] }; \
         key <C> { type = \"E\", [ a, b, c ] }; key <D> { type = \"B\", [ a, b, c ] }; \
         key <E> { type = \"I\", [ a, b, c ] }; key <F> { type = \"M\", [ a, b, c ] };",
    );
    let mut s = State::new(k);
    // A bit for a modifier the keymap does not have never matches; it is
    // not the same as no modifier at all.
    assert_eq!(s.key_level(10, 0), Some(0));
    // A level's name gives the type no level.
    assert_eq!(s.keymap().num_levels_for_key(11, 0), 2);
    // A level may be worked out: 1+1 is the second, 2*2-1 the third.
    assert_eq!(s.keymap().num_levels_for_key(12, 0), 3);
    // `!` is no mask: the type is left without modifiers, and both of its
    // entries are then the one for no modifier, the later level winning.
    assert_eq!(s.key_level(13, 0), Some(2));
    assert_eq!(s.key_consumed_mods(13), 0);
    // `~` is: every modifier but Shift.
    assert_eq!(s.key_level(14, 0), Some(0));
    s.update_mask(LOCK, 0, 0, 0, 0, 0);
    assert_eq!(s.key_level(14, 0), Some(1));
    s.update_mask(LOCK | CONTROL, 0, 0, 0, 0, 0);
    assert_eq!(s.key_level(14, 0), Some(2));
    // The first `modifiers` counts; a level or a modifier that is none is
    // passed over, and so is the entry for a modifier the type lacks.
    assert_eq!(s.keymap().num_levels_for_key(15, 0), 3);
    s.update_mask(1, 0, 0, 0, 0, 0);
    assert_eq!(s.key_level(15, 0), Some(1));
}

#[test]
fn leds_take_their_fields_as_libxkbcommon_does() {
    let text = "xkb_keymap { \
        xkb_keycodes { <A> = 10; indicator 1 = \"Caps Lock\"; indicator 2 = \"Foo\"; \
          indicator 5 = \"Caps Lock\"; indicator 3 = \"Three\"; indicator 3 = \"Third\"; }; \
        xkb_types { }; \
        xkb_compatibility { \
          indicator \"Foo\" { whichModState = None; modifiers = Lock; }; \
          indicator \"Caps Lock\" { modifiers = Lock; }; \
          indicator \"Caps Lock\" { whichGroupState = Any; groups = Group2; }; \
          indicator.modifiers = Shift; indicator.whichModState = Locked; \
          indicator \"Third\" { groups = 0; }; }; \
        xkb_symbols { key <A> { [ a ], [ b ] }; }; };";
    let k: Keymap = text.parse().unwrap();
    // A name stays with the first number it was given; a number takes the
    // last name it was given.
    assert_eq!(k.num_leds(), 3);
    assert_eq!(k.led_name(2), Some("Third"));
    let mut s = State::new(k);
    // `None` for the state with modifiers named means the effective ones.
    s.update_mask(LOCK, 0, 0, 0, 0, 0);
    assert!(s.led_name_is_active("Foo"));
    // The defaults: "Third" watches Shift, locked.
    assert!(!s.led_name_is_active("Third"));
    s.update_mask(0, 0, 1, 0, 0, 0);
    assert!(s.led_name_is_active("Third"));
    // A map given twice is merged half by half: the modifiers of the
    // first, the groups of the second.
    assert!(!s.led_name_is_active("Caps Lock"));
    s.update_mask(0, 0, LOCK, 0, 0, 0);
    assert!(s.led_name_is_active("Caps Lock"));
    s.update_mask(0, 0, 0, 0, 0, 1);
    assert!(s.led_name_is_active("Caps Lock"), "the second layout");
}

#[test]
fn the_small_things_of_a_key_body() {
    let k = keymap(
        "",
        "interpret b { repeat; }; interpret c { !repeat; }; interpret.repeat = False; \
         interpret d { virtualModifier = Nope; repeat = True; }; \
         interpret e { repeat = True; }; interpret e { virtualModifier = Alt; }; \
         interpret e+AnyOfOrNone(all) { repeat = False; };",
        "key <A> { [ { NoSymbol }, { a, NoSymbol }, { b } ] }; \
         key <B> { groupsWrap = no, [ b ], [ x ] }; key <C> { groupsClamp = 1, [ c ], [ y ] }; \
         key <D> { symbols[1+1] = [ d ], symbols[Group99] = [ z ], type = 5, [ d ], symbols[1] = [ q ] }; \
         key <E> { [ e ] }; modifier_map Mod3 { <E> }; \
         name[Group1] = \"\"; name[Group2] = \"two\";",
    );
    // `{ NoSymbol }` is an empty level; in a longer set it is a keysym.
    assert_eq!(k.num_levels_for_key(10, 0), 1);
    assert!(syms(&k, 10, 0, 0).is_empty());
    // A bare `repeat` is yes, `!repeat` is no.
    assert!(k.key_repeats(11) && !k.key_repeats(12));
    // A field that is wrong is passed over, the others stand.
    assert!(k.key_repeats(13));
    assert_eq!(k.num_layouts_for_key(13), 2);
    assert_eq!(syms(&k, 13, 0, 0), [sym("d")], "given once already");
    assert_eq!(syms(&k, 13, 1, 0), [sym("d")]);
    // The same interpret three times: each later one wins what it sets.
    assert!(!k.key_repeats(14));
    assert_eq!(k.mod_mask(k.mod_index("Alt").unwrap()), MOD3);
    // An empty name is a name.
    assert_eq!(k.layout_name(0), Some(""));
    assert_eq!(k.layout_index("two"), Some(1));
    let mut s = State::new(k);
    s.update_mask(0, 0, 0, 0, 0, 2);
    assert_eq!(s.layout(), 0);
    // `groupsWrap = no` clamps; a number is no yes or no, so <C> wraps.
    s.update_mask(0, 0, 0, 2, 0, 1);
    assert_eq!(s.layout(), 1);
    let mut s = State::new(keymap(
        "",
        "",
        "key <A> { [ a ], [ b ], [ c ] }; key <B> { groupsWrap = no, [ b ], [ x ] }; \
         key <C> { groupsClamp = 1, [ c ], [ y ] };",
    ));
    s.update_mask(0, 0, 0, 0, 0, 2);
    assert_eq!(s.key_one_sym(11), sym("x"), "clamped to the last");
    assert_eq!(s.key_one_sym(12), sym("c"), "wrapped to the first");
    // A layout the key does not have has no level.
    assert_eq!(s.key_level(11, 1), Some(0));
    assert_eq!(s.key_level(11, 2), None);
    assert_eq!(s.key_level(99, 0), None);
}

#[test]
fn text_libxkbcommon_reads_is_read() {
    for text in [
        // The other two words for a file of all the sections.
        "xkb_semantics { xkb_keycodes { <A> = 10; }; xkb_types { }; xkb_compat { }; xkb_symbols { }; };",
        "xkb_layout { xkb_keycodes { <A> = 10; }; xkb_types { }; xkb_compat { }; xkb_symbols { }; };",
        // A section given twice: the first counts.
        "xkb_keymap { xkb_keycodes { <A> = 10; }; xkb_types { }; xkb_compat { }; \
         xkb_symbols { key <A> { [ a ] }; }; xkb_keycodes { <A> = 99; }; };",
        // An interpret naming a virtual modifier nobody declared, with
        // another field after it.
        "xkb_keymap { xkb_keycodes { <A> = 10; }; xkb_types { }; \
         xkb_compat { interpret a { virtualModifier = AltGr; repeat = True; }; }; \
         xkb_symbols { key <A> { [ a ] }; }; };",
    ] {
        let k: Keymap = text.parse().unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!((k.min_keycode(), k.max_keycode()), (10, 10), "{text}");
    }
    // A keymap without keys answers as libxkbcommon does.
    let k: Keymap = "xkb_keymap { xkb_keycodes { minimum = 20; maximum = 30; }; xkb_types { }; \
                     xkb_compat { }; xkb_symbols { }; };"
        .parse()
        .unwrap();
    assert_eq!((k.min_keycode(), k.max_keycode()), (8, 255));
    // The range counts a name a later statement took away again.
    let k: Keymap = "xkb_keymap { xkb_keycodes { <A> = 5; <B> = 9; <A> = 7; augment <A> = 3; }; \
                     xkb_types { }; xkb_compat { }; xkb_symbols { }; };"
        .parse()
        .unwrap();
    assert_eq!((k.min_keycode(), k.max_keycode()), (3, 9));
    assert_eq!(k.key_by_name("A"), Some(7));
    assert_eq!(k.key_name(5), None);
}

/// Run `f` on a thread with a 2 MB stack: what a thread other than the
/// main one gets by default.
fn on_a_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .expect("the thread must not die")
}

#[test]
fn a_long_chain_of_operators_cannot_overflow_the_stack() {
    const TERMS: usize = 200_000;
    let wrap = |types: &str, compat: &str, symbols: &str| {
        format!(
            "xkb_keymap {{ xkb_keycodes {{ <A> = 10; indicator 1 = \"x\"; }}; \
             xkb_types {{ {types} }}; xkb_compat {{ {compat} }}; xkb_symbols {{ {symbols} }}; }};"
        )
    };
    let chain = |term: &str| vec![term; TERMS].join("+");
    let texts = [
        // Worked out as a modifier mask.
        wrap(
            &format!(
                "type \"T\" {{ modifiers = {}; map[{}] = Level2; }};",
                chain("Shift"),
                chain("Shift")
            ),
            "",
            "key <A> { type = \"T\", [ a, b ] };",
        ),
        // As a group mask and a state component.
        wrap(
            "",
            &format!(
                "indicator \"x\" {{ groups = {}; whichGroupState = {}; }};",
                chain("1"),
                chain("Locked")
            ),
            "",
        ),
        // As a number.
        wrap(
            &format!(
                "type \"T\" {{ modifiers = Shift; map[Shift] = {}-{}; }};",
                chain("1"),
                TERMS - 2
            ),
            "",
            "key <A> { type = \"T\", [ a, b ] };",
        ),
        // Never worked out at all: only read, and dropped.
        wrap("", "", &format!("a = {};", chain("1"))),
        wrap("", "", &format!("key <A> {{ [ a ], x = {} }};", chain("b"))),
    ];
    let levels = on_a_small_stack(move || {
        texts
            .iter()
            .map(|text| {
                let k: Keymap = text.parse().unwrap_or_else(|e| panic!("{e}"));
                (k.num_levels_for_key(10, 0), k.num_leds())
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(levels, [(2, 1), (0, 1), (2, 1), (0, 1), (1, 1)]);
}

/// How long reading `text` takes, and that it reads.
fn timed(what: &str, text: &str) -> Duration {
    let start = Instant::now();
    let k: Result<Keymap, _> = text.parse();
    let took = start.elapsed();
    assert!(k.is_ok(), "{what}: {}", k.unwrap_err());
    took
}

/// Each of these texts took the code before many seconds (tens, built
/// without optimisation), by doing work in proportion to the square of its
/// size. They now take a fraction of a second; the bound is far above that,
/// so a slow or busy machine does not fail it, and far below the old cost.
#[test]
fn reading_takes_time_in_proportion_to_the_text() {
    let numbered = |n: usize, f: &dyn Fn(usize) -> String| (0..n).map(f).collect::<String>();
    let wrap = |keycodes: &str, types: &str, compat: &str, symbols: &str| {
        format!(
            "xkb_keymap {{ xkb_keycodes {{ {keycodes} }}; xkb_types {{ {types} }}; \
             xkb_compat {{ {compat} }}; xkb_symbols {{ {symbols} }}; }};"
        )
    };
    let keys = numbered(500, &|i| format!("<K{i}> = {};", i + 8));
    let texts = [
        (
            "a modifier map naming one keysym 20,000 times, over a million levels",
            wrap(
                &keys,
                "type \"x\" { modifiers = Shift; map[Shift] = Level2048; };",
                "",
                &format!(
                    "{} modifier_map Shift {{ {} }};",
                    numbered(500, &|i| format!("key <K{i}> {{ type = \"x\", [ b ] }};")),
                    vec!["a"; 20_000].join(",")
                ),
            ),
        ),
        (
            "a modifier map naming 20,000 different keysyms",
            wrap(
                &keys,
                "type \"x\" { modifiers = Shift; map[Shift] = Level2048; };",
                "",
                &format!(
                    "{} modifier_map Shift {{ {} }};",
                    numbered(500, &|i| format!("key <K{i}> {{ type = \"x\", [ b ] }};")),
                    numbered(20_000, &|i| format!("U{:04X},", 0x4e00 + i))
                ),
            ),
        ),
        (
            "100,000 keycodes",
            wrap(
                &numbered(100_000, &|i| format!("<K{i}> = {i};")),
                "",
                "",
                "",
            ),
        ),
        (
            "50,000 aliases",
            wrap(
                &format!(
                    "<A> = 10; {}",
                    numbered(50_000, &|i| format!("alias <L{i}> = <A>;"))
                ),
                "",
                "",
                "",
            ),
        ),
        (
            "60,000 types",
            wrap(
                "<A> = 10;",
                &numbered(60_000, &|i| format!("type \"{i}\" {{ }};")),
                "",
                "key <A> { type = \"59999\", [ a ] };",
            ),
        ),
        (
            "a type with 60,000 entries",
            wrap(
                "<A> = 10;",
                &format!(
                    "type \"T\" {{ modifiers = 0xffffffff; {} }};",
                    numbered(60_000, &|i| format!("map[{}] = Level2;", i + 256))
                ),
                "",
                "",
            ),
        ),
        (
            "30,000 interprets and 30,000 statements for one key",
            wrap(
                "<A> = 10;",
                "",
                &numbered(30_000, &|i| {
                    format!("interpret a+Exactly({i}) {{ repeat = True; }};")
                }),
                &numbered(30_000, &|i| {
                    format!("key <A> {{ symbols[Group{}] = [ a ] }};", i % 4 + 1)
                }),
            ),
        ),
        (
            "30,000 keys each asking the same 30,000 interprets",
            wrap(
                &numbered(30_000, &|i| format!("<K{i}> = {};", i + 8)),
                "",
                &numbered(30_000, &|i| {
                    format!("interpret a+Exactly({}) {{ repeat = True; }};", i + 1)
                }),
                &numbered(30_000, &|i| format!("key <K{i}> {{ [ a ] }};")),
            ),
        ),
    ];
    for (what, text) in &texts {
        let took = timed(what, text);
        println!("{what}: {} bytes in {took:?}", text.len());
        assert!(took < Duration::from_secs(20), "{what} took {took:?}");
    }
}

#[test]
fn the_handwritten_texts_read_or_fail_and_never_panic() {
    let cases = handwritten::all();
    assert!(cases.len() > 300, "{} cases", cases.len());
    let mut read = 0;
    for (_label, text) in &cases {
        let Ok(keymap) = text.parse::<Keymap>() else {
            continue;
        };
        read += 1;
        let codes: Vec<u32> = keymap.keycodes().collect();
        let mut state = State::new(keymap);
        for (mods, group) in [(0, 0), (0xff, 1), (u32::MAX, u32::MAX)] {
            state.update_mask(mods, 0, mods, 0, 0, group);
            for &kc in &codes {
                let _ = (
                    state.key_syms(kc),
                    state.key_utf8(kc),
                    state.key_one_sym(kc),
                );
                let _ = (state.key_consumed_mods(kc), state.key_level(kc, group));
            }
            for led in 0..=state.keymap().num_leds() {
                let _ = state.led_is_active(led);
            }
        }
    }
    assert!(read > 250, "only {read} of the cases read");
}
