//! Compose files: every shape of line, how sequences displace each other,
//! the state walking them, and where the files are found.

use std::path::{Path, PathBuf};
use xkb::compose::{Paths, State, Status, Table};
use xkb::Keysym;

fn key(name: &str) -> Keysym {
    Keysym::from_name(name).unwrap()
}

fn seq(names: &[&str]) -> Vec<Keysym> {
    names.iter().map(|n| key(n)).collect()
}

/// A table's entries as (text, keysym), in the table's order.
fn results(t: &Table) -> Vec<(Option<&str>, Keysym)> {
    t.entries().map(|e| (e.text, e.keysym)).collect()
}

#[test]
fn lines_of_every_shape() {
    let t = Table::parse(concat!(
        "# a comment\n",
        "\n",
        "<dead_acute> <e> : \"é\" eacute # trailing\n",
        "<Multi_key> <o> <c> : \"©\"\n",
        "<Multi_key> <minus> <minus> <minus> : emdash\n",
        "<Multi_key> <quotedbl> <backslash> : \"\\\"\\\\\"\n",
        "<Multi_key> <1> : \"\\x41\\102\\x4\"\n",
        "Ctrl ~Shift <a> ! Alt <b> None <c> : \"mods\"\n",
        "\t<U20AC>\t<x>:\"euro\"\tEuroSign\r\n",
    ));
    let got: Vec<(Vec<Keysym>, Option<&str>, Keysym)> = t
        .entries()
        .map(|e| (e.sequence.to_vec(), e.text, e.keysym))
        .collect();
    let mut want = vec![
        (seq(&["dead_acute", "e"]), Some("é"), key("eacute")),
        (seq(&["Multi_key", "o", "c"]), Some("©"), Keysym::NO_SYMBOL),
        (
            seq(&["Multi_key", "minus", "minus", "minus"]),
            None,
            key("emdash"),
        ),
        (
            seq(&["Multi_key", "quotedbl", "backslash"]),
            Some("\"\\"),
            Keysym::NO_SYMBOL,
        ),
        (seq(&["Multi_key", "1"]), Some("AB\u{4}"), Keysym::NO_SYMBOL),
        (seq(&["a", "b", "c"]), Some("mods"), Keysym::NO_SYMBOL),
        (seq(&["U20AC", "x"]), Some("euro"), key("EuroSign")),
    ];
    want.sort();
    assert_eq!(got, want);
    assert_eq!(t.len(), 7);
    assert!(!t.is_empty());
}

#[test]
fn lines_that_cannot_be_read_are_left_out() {
    for line in [
        ": \"x\"",
        "<a> \"x\"",
        "<a> :",
        "<a> : \"x",
        "<a : \"x\"",
        "<NoSuchKeysym> : \"x\"",
        "<a> : NoSuchKeysym",
        "<a> : \"x\" \"y\"",
        "<a> : \"x\" junk",
        "<a> : !",
        "Hyper <a> : \"x\"",
        "~ <a> : \"x\"",
        "<a> $ : \"x\"",
        "<a> : \"\\xff\"",
        "<NoSymbol> : \"x\"",
        "<a> <a> <a> <a> <a> <a> <a> <a> <a> <a> <a> : \"eleven\"",
        "include \"/nowhere\"",
        "include",
    ] {
        assert!(Table::parse(line).is_empty(), "{line}");
    }
    assert_eq!(
        Table::parse("<a> <a> <a> <a> <a> <a> <a> <a> <a> <a> : \"ten\"").len(),
        1
    );
    assert_eq!(
        Table::parse_bytes(b"<a> : \"\xe9\"\n<b> : \"ok\" # \xe9\n").len(),
        1
    );
    assert_eq!(Table::parse("\u{feff}<a> : \"bom\"").len(), 1);
}

#[test]
fn the_fine_print_of_results() {
    // What follows the keysym is not looked at; an octal number past a byte
    // is dropped; an empty string is a string.
    let t = Table::parse("<a> : x y\n<b> : \"\\777\\101\"\n<c> : \"\"\n<d> : \"\" x\n");
    assert_eq!(
        results(&t),
        [
            (None, key("x")),
            (Some("A"), Keysym::NO_SYMBOL),
            (Some(""), Keysym::NO_SYMBOL),
            (Some(""), key("x"))
        ]
    );
    // An empty string is the text; only a missing one falls back to the
    // keysym's character.
    let mut s = State::new(t);
    assert_eq!(
        (s.feed(key("a")), s.utf8().as_str()),
        (Status::Composed, "x")
    );
    assert_eq!(
        (s.feed(key("d")), s.utf8().as_str()),
        (Status::Composed, "")
    );
    assert_eq!(s.keysym(), key("x"));
}

#[test]
fn no_sequence_is_the_start_of_another() {
    // The same sequence again: the later result, for what it gives.
    let t = Table::parse("<a> <b> : \"1\" x\n<a> <b> : \"2\"\n");
    assert_eq!(results(&t), [(Some("2"), key("x"))]);
    // A longer one replaces the shorter one it extends.
    let t = Table::parse("<a> <b> : \"short\"\n<a> <b> <c> : \"long\"\n");
    assert_eq!(results(&t), [(Some("long"), Keysym::NO_SYMBOL)]);
    // A shorter one that would cut longer ones off is left out.
    let t = Table::parse("<a> <b> <c> : \"long\"\n<a> <b> <d> : \"other\"\n<a> <b> : \"short\"\n");
    let texts: Vec<_> = t.entries().map(|e| e.text).collect();
    assert_eq!(texts, [Some("long"), Some("other")]);
}

#[test]
fn a_state_walks_the_sequences() {
    let table = Table::parse(concat!(
        "<dead_acute> <e> : \"é\" eacute\n",
        "<Multi_key> <o> <c> : \"©\"\n",
        "<Multi_key> <minus> <minus> : endash\n",
    ));
    let mut s = State::new(table);
    assert_eq!(s.status(), Status::Nothing);
    assert_eq!(s.feed(key("a")), Status::Nothing, "an ordinary key");
    assert_eq!(s.feed(key("dead_acute")), Status::Composing);
    assert_eq!((s.utf8(), s.keysym()), (String::new(), Keysym::NO_SYMBOL));
    assert_eq!(
        s.feed(key("Shift_L")),
        Status::Composing,
        "a modifier changes nothing"
    );
    assert_eq!(s.feed(key("e")), Status::Composed);
    assert_eq!((s.utf8().as_str(), s.keysym()), ("é", key("eacute")));
    // The next key starts over.
    assert_eq!(s.feed(key("Multi_key")), Status::Composing);
    assert_eq!(s.utf8(), "");
    assert_eq!(s.feed(key("o")), Status::Composing);
    assert_eq!(s.feed(key("x")), Status::Cancelled);
    assert_eq!(s.feed(key("x")), Status::Nothing);
    for name in ["Multi_key", "o", "c"] {
        s.feed(key(name));
    }
    assert_eq!(
        (s.status(), s.utf8().as_str(), s.keysym()),
        (Status::Composed, "©", Keysym::NO_SYMBOL)
    );
    // Only a keysym in the file: its character is the text.
    for name in ["Multi_key", "minus", "minus"] {
        s.feed(key(name));
    }
    assert_eq!(
        (s.status(), s.utf8().as_str(), s.keysym()),
        (Status::Composed, "–", key("endash"))
    );
    // A sequence can follow a completed one directly.
    assert_eq!(s.feed(key("dead_acute")), Status::Composing);
    s.reset();
    assert_eq!(s.status(), Status::Nothing);
    assert_eq!(s.feed(key("e")), Status::Nothing);
    assert_eq!(s.table().len(), 3);
    // An empty table composes nothing.
    let mut s = State::new(Table::default());
    assert_eq!(s.feed(key("dead_acute")), Status::Nothing);
}

/// A directory of this test's own, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let path = std::env::temp_dir().join(format!("xkb-rs-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Dir(path)
    }

    fn write(&self, file: &str, text: &str) -> PathBuf {
        let path = self.0.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A small copy of X11's locale data.
fn locale_data(dir: &Dir) -> PathBuf {
    dir.write(
        "locale/compose.dir",
        "# comment\nen_US.UTF-8/Compose: en_US.UTF-8\nen_US.UTF-8/Compose\t\tde_DE.UTF-8\n\
         iso8859-1/Compose: C\nel_GR.UTF-8/Compose: el_GR.UTF-8\n",
    );
    dir.write(
        "locale/locale.alias",
        "# comment\ngerman: de_DE.UTF-8\ngreek\tel_GR.UTF-8\n",
    );
    dir.write(
        "locale/en_US.UTF-8/Compose",
        "<dead_acute> <e> : \"é\" eacute\n<Multi_key> <o> <c> : \"©\"\n",
    );
    dir.write(
        "locale/el_GR.UTF-8/Compose",
        "include \"%S/en_US.UTF-8/Compose\"\n<dead_acute> <Greek_alpha> : \"ά\"\n",
    );
    dir.write("locale/iso8859-1/Compose", "<a> : \"latin\"\n");
    dir.0.join("locale")
}

fn texts(t: &Table) -> Vec<&str> {
    let mut v: Vec<&str> = t.entries().filter_map(|e| e.text).collect();
    v.sort();
    v
}

#[test]
fn the_file_of_a_locale() {
    let dir = Dir::new("locale");
    let paths = Paths::system(locale_data(&dir));
    let t = Table::from_locale("en_US.UTF-8", &paths).unwrap();
    assert_eq!(texts(&t), ["©", "é"]);
    // Another locale sharing the file; an alias; the C locale, which gets
    // the UTF-8 file and not its own Latin-1 one.
    assert_eq!(Table::from_locale("de_DE.UTF-8", &paths).unwrap(), t);
    assert_eq!(Table::from_locale("german", &paths).unwrap(), t);
    assert_eq!(Table::from_locale("C", &paths).unwrap(), t);
    // A file that includes the system's (%S).
    let greek = Table::from_locale("greek", &paths).unwrap();
    assert_eq!(texts(&greek), ["©", "é", "ά"]);
    // A locale the data does not list.
    let e = Table::from_locale("xx_XX.UTF-8", &paths).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
    assert!(e.to_string().contains("xx_XX.UTF-8"), "{e}");
    // No data at all.
    let nowhere = Paths::system(dir.0.join("nowhere"));
    assert!(Table::from_locale("en_US.UTF-8", &nowhere).is_err());
}

#[test]
fn the_users_files_come_first() {
    let dir = Dir::new("user");
    let system = locale_data(&dir);
    let home = dir.0.join("home");
    // ~/.XCompose, adding to the locale's file (%L) and to a file of its
    // own beside it (%H); a literal percent sign in a name.
    dir.write(
        "home/.XCompose",
        "include \"%L\"\ninclude \"%H/extra%%.compose\"\n<Multi_key> <h> <i> : \"hello\"\n<dead_acute> <e> : \"E\"\n",
    );
    dir.write("home/extra%.compose", "<Multi_key> <x> : \"extra\"\n");
    let mut paths = Paths {
        locale_dir: system,
        home: Some(home.clone()),
        xdg_config_home: None,
        compose_file: None,
    };
    let t = Table::from_locale("el_GR.UTF-8", &paths).unwrap();
    assert_eq!(
        texts(&t),
        ["E", "extra", "hello", "©", "ά"],
        "the later line for dead_acute e wins"
    );
    let acute_e = t.entries().find(|e| e.text == Some("E")).unwrap();
    assert_eq!(
        acute_e.keysym,
        key("eacute"),
        "and keeps the keysym the earlier line gave"
    );

    // $XDG_CONFIG_HOME/XCompose (or ~/.config/XCompose) before ~/.XCompose.
    dir.write("home/.config/XCompose", "<a> : \"config\"\n");
    assert_eq!(
        texts(&Table::from_locale("en_US.UTF-8", &paths).unwrap()),
        ["config"]
    );
    let xdg = dir.write("xdg/XCompose", "<a> : \"xdg\"\n");
    paths.xdg_config_home = Some(xdg.parent().unwrap().to_path_buf());
    assert_eq!(
        texts(&Table::from_locale("en_US.UTF-8", &paths).unwrap()),
        ["xdg"]
    );
    // $XCOMPOSEFILE before everything; one that cannot be read is passed over.
    paths.compose_file = Some(dir.write("mine", "<a> : \"mine\"\n"));
    assert_eq!(
        texts(&Table::from_locale("en_US.UTF-8", &paths).unwrap()),
        ["mine"]
    );
    paths.compose_file = Some(dir.0.join("missing"));
    assert_eq!(
        texts(&Table::from_locale("en_US.UTF-8", &paths).unwrap()),
        ["xdg"]
    );
}

#[test]
fn one_file_and_its_includes() {
    let dir = Dir::new("file");
    let paths = Paths::system(locale_data(&dir));
    let file = dir.write("my.compose", "include \"%L\"\n<a> : \"mine\"\n");
    let t = Table::from_file(&file, "en_US.UTF-8", &paths).unwrap();
    assert_eq!(texts(&t), ["mine", "©", "é"]);
    assert!(Table::from_file(Path::new("/no/such/file"), "en_US.UTF-8", &paths).is_err());
    // Includes that go nowhere, that need a home there is none of, or that
    // use an unknown escape, add nothing; the rest of the file is read.
    let file = dir.write(
        "odd.compose",
        "include \"%S/missing\"\ninclude \"%H/x\"\ninclude \"%Q\"\ninclude \"unterminated\ninclude \"%L\" junk\n<a> : \"still\"\n",
    );
    assert_eq!(
        texts(&Table::from_file(&file, "en_US.UTF-8", &paths).unwrap()),
        ["still"]
    );
    // A file that includes itself stops after a few levels.
    let own = dir.0.join("loop.compose");
    let text = format!(
        "<a> : \"loop\"\ninclude \"{}\"\n",
        own.to_string_lossy().replace('%', "%%")
    );
    std::fs::write(&own, text).unwrap();
    assert_eq!(
        texts(&Table::from_file(&own, "en_US.UTF-8", &paths).unwrap()),
        ["loop"]
    );
    // Nor does one that includes itself many times over take forever.
    let include = format!("include \"{}\"\n", own.to_string_lossy().replace('%', "%%"));
    std::fs::write(&own, format!("{}<a> : \"many\"\n", include.repeat(200))).unwrap();
    assert_eq!(
        texts(&Table::from_file(&own, "en_US.UTF-8", &paths).unwrap()),
        ["many"]
    );
}

#[test]
fn paths_from_the_environment_default_to_the_system_directory() {
    // Only what does not depend on this machine's environment.
    let paths = Paths::from_env();
    if std::env::var_os("XLOCALEDIR").is_none() {
        assert_eq!(paths.locale_dir, PathBuf::from("/usr/share/X11/locale"));
    }
    assert_eq!(Paths::system("/x").home, None);
}

#[test]
fn damaged_compose_files_never_panic() {
    let source = concat!(
        "<dead_acute> <e> : \"é\" eacute # comment\n",
        "! Ctrl ~Shift <Multi_key> <o> <c> : \"©\" copyright\n",
        "<Multi_key> <1> : \"\\x41\\102\\\"\"\n",
        "include \"%L\"\n",
    )
    .as_bytes();
    const PIECES: [&str; 14] = [
        "<", ">", ":", "\"", "\\", "\\x", "\\7", "!", "~", "#", "%", "include ", "\n", "\0",
    ];
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut below = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n.max(1) as u64) as usize
    };
    for _ in 0..4000 {
        let mut damaged = source.to_vec();
        for _ in 0..1 + below(5) {
            let at = below(damaged.len());
            match below(4) {
                0 => {
                    let piece = PIECES[below(PIECES.len())].as_bytes();
                    damaged.splice(at..at, piece.iter().copied());
                }
                1 => {
                    let end = (at + below(12)).min(damaged.len());
                    damaged.drain(at..end);
                }
                2 => damaged.truncate(at),
                _ if damaged.is_empty() => {}
                _ => damaged[at] = below(256) as u8,
            }
        }
        let table = Table::parse_bytes(&damaged);
        let first: Vec<Keysym> = table.entries().flat_map(|e| e.sequence.to_vec()).collect();
        let mut state = State::new(table);
        for sym in first {
            state.feed(sym);
            let _ = (state.status(), state.utf8(), state.keysym());
        }
    }
}
