//! Compose: sequences of keysyms that make one character, such as a dead
//! accent and a letter, or the Compose key and two more keys.
//!
//! The sequences come from X11's Compose files. [`Table`] reads them (the
//! user's `~/.XCompose`, or the file of the locale), and a [`State`] is fed
//! each key press's keysym and says whether it is part of a sequence.
//!
//! ```
//! use xkb::compose::{State, Status, Table};
//! use xkb::Keysym;
//!
//! let table = Table::parse("<dead_acute> <e> : \"é\" eacute\n");
//! let mut state = State::new(table);
//! let key = |name| Keysym::from_name(name).unwrap();
//! assert_eq!(state.feed(key("dead_acute")), Status::Composing);
//! assert_eq!(state.feed(key("e")), Status::Composed);
//! assert_eq!(state.utf8(), "é");
//! assert_eq!(state.keysym(), key("eacute"));
//! ```

use crate::keysym::Keysym;
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// How deep `include` may nest.
const MAX_INCLUDE_DEPTH: u32 = 5;
/// How many files one table may include in all: a file that includes
/// itself several times would otherwise be read a great many times.
const MAX_INCLUDES: u32 = 64;
/// The longest sequence a Compose file may define; longer ones are left out.
const MAX_SEQUENCE: usize = 10;

/// The largest file read as a Compose file, or as one of the locale data's
/// lists: 16 MiB. The largest Compose file X11 ships is under 1 MiB; a path
/// that leads to something endless must not be read to the end.
pub const MAX_FILE_SIZE: u64 = 16 * 1024 * 1024;

/// Read a file of at most [`MAX_FILE_SIZE`] bytes. Only a regular file is
/// opened: a pipe, a device or a directory under the name of a Compose file
/// is an error, not something to wait on or read without end.
fn read_file(path: &Path) -> io::Result<Vec<u8>> {
    use std::io::Read;
    if !std::fs::metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a regular file", path.display()),
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is larger than a Compose file may be", path.display()),
        ));
    }
    Ok(bytes)
}

/// Where Compose files are looked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// X11's locale data: `compose.dir`, `locale.alias` and each locale's
    /// `Compose` file. `/usr/share/X11/locale` on most systems.
    pub locale_dir: PathBuf,
    /// The user's home directory, for `~/.XCompose` and `%H`.
    pub home: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME`, for `XCompose` in it; `~/.config` if unset.
    pub xdg_config_home: Option<PathBuf>,
    /// `$XCOMPOSEFILE`: a file to use before any other.
    pub compose_file: Option<PathBuf>,
}

impl Paths {
    /// The paths the environment gives: `XLOCALEDIR` (default
    /// `/usr/share/X11/locale`), `HOME`, `XDG_CONFIG_HOME`, `XCOMPOSEFILE`.
    pub fn from_env() -> Paths {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        Paths {
            locale_dir: var("XLOCALEDIR").unwrap_or_else(|| "/usr/share/X11/locale".into()),
            home: var("HOME"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
            compose_file: var("XCOMPOSEFILE"),
        }
    }

    /// Only the system's data under `locale_dir`: no user files.
    pub fn system(locale_dir: impl Into<PathBuf>) -> Paths {
        Paths {
            locale_dir: locale_dir.into(),
            home: None,
            xdg_config_home: None,
            compose_file: None,
        }
    }

    /// Look `name` up in a two-column file of the locale data
    /// (`locale.alias`, `compose.dir`): lines of `left: right` or `left
    /// right`. Returns the other column of the first line whose `left`
    /// (or, with `by_right`, `right`) is `name`.
    fn lookup(&self, file: &str, name: &str, by_right: bool) -> Option<String> {
        let bytes = read_file(&self.locale_dir.join(file)).ok()?;
        for line in bytes.split(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(line);
            let line = line.split('#').next().unwrap_or("");
            let mut words = line.split_whitespace();
            let (Some(left), Some(right)) = (words.next(), words.next()) else {
                continue;
            };
            let left = left.strip_suffix(':').unwrap_or(left);
            let (key, value) = if by_right {
                (right, left)
            } else {
                (left, right)
            };
            if key == name {
                return Some(value.to_string());
            }
        }
        None
    }

    /// The locale's full name: what `locale.alias` says, or the name itself.
    fn resolve_locale(&self, locale: &str) -> String {
        self.lookup("locale.alias", locale, false)
            .unwrap_or_else(|| locale.to_string())
    }

    /// The Compose file of a (resolved) locale, from `compose.dir`.
    fn locale_file(&self, locale: &str) -> Option<PathBuf> {
        // The C locale's own file is Latin-1; a C locale nearly always
        // stands for an unset UTF-8 one, so it gets the UTF-8 file.
        let locale = if locale == "C" { "en_US.UTF-8" } else { locale };
        let file = self.lookup("compose.dir", locale, true)?;
        Some(self.locale_dir.join(file))
    }
}

/// One sequence of a [`Table`] and what it makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry<'a> {
    /// The keysyms to press, in order.
    pub sequence: &'a [Keysym],
    /// The text it makes; `None` if the file gives only a keysym.
    pub text: Option<&'a str>,
    /// The keysym it makes; [`Keysym::NO_SYMBOL`] if the file gives only
    /// text.
    pub keysym: Keysym,
}

/// The Compose sequences of a locale or a file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    /// Sorted by sequence; no sequence is the start of another.
    entries: Vec<(Vec<Keysym>, Option<String>, Keysym)>,
}

impl Table {
    /// The table for a locale (`"en_US.UTF-8"`, usually from `LC_ALL`,
    /// `LC_CTYPE` or `LANG`): the first of `$XCOMPOSEFILE`,
    /// `$XDG_CONFIG_HOME/XCompose`, `~/.XCompose` and the locale's own file
    /// that can be read. An error of kind `NotFound` if there is none.
    pub fn from_locale(locale: &str, paths: &Paths) -> io::Result<Table> {
        let locale = paths.resolve_locale(locale);
        let config = paths
            .xdg_config_home
            .clone()
            .or_else(|| paths.home.as_ref().map(|h| h.join(".config")));
        let candidates = [
            paths.compose_file.clone(),
            config.map(|c| c.join("XCompose")),
            paths.home.as_ref().map(|h| h.join(".XCompose")),
            paths.locale_file(&locale),
        ];
        for path in candidates.into_iter().flatten() {
            if let Ok(bytes) = read_file(&path) {
                let mut loader = Loader::new(Some(paths), &locale);
                loader.text(&bytes);
                return Ok(loader.finish());
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no Compose file for locale {locale}"),
        ))
    }

    /// The table of one Compose file. `locale` is what `%L` in an `include`
    /// stands for.
    pub fn from_file(path: &Path, locale: &str, paths: &Paths) -> io::Result<Table> {
        let bytes = read_file(path)?;
        let locale = paths.resolve_locale(locale);
        let mut loader = Loader::new(Some(paths), &locale);
        loader.text(&bytes);
        Ok(loader.finish())
    }

    /// The table of a Compose file's text. Lines that cannot be read are
    /// left out, as are `include` lines: there is nowhere to include from.
    pub fn parse(text: &str) -> Table {
        Table::parse_bytes(text.as_bytes())
    }

    /// As [`Table::parse`], for text that may not be UTF-8 throughout (a
    /// line whose string is not UTF-8 is left out).
    pub fn parse_bytes(bytes: &[u8]) -> Table {
        let mut loader = Loader::new(None, "");
        loader.text(bytes);
        loader.finish()
    }

    /// How many sequences the table has.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table has no sequences.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every sequence, ordered by its keysyms.
    pub fn entries(&self) -> impl Iterator<Item = Entry<'_>> {
        self.entries.iter().map(|(sequence, text, keysym)| Entry {
            sequence,
            text: text.as_deref(),
            keysym: *keysym,
        })
    }

    /// What the table has for the keysyms pressed so far.
    fn find(&self, pressed: &[Keysym]) -> Found {
        let at = self
            .entries
            .partition_point(|(sequence, ..)| sequence.as_slice() < pressed);
        match self.entries.get(at) {
            Some((sequence, ..)) if sequence == pressed => Found::Whole(at),
            Some((sequence, ..)) if sequence.starts_with(pressed) => Found::Start,
            _ => Found::Nothing,
        }
    }
}

enum Found {
    /// The keysyms are a whole sequence: the entry at this index.
    Whole(usize),
    /// They begin at least one sequence.
    Start,
    Nothing,
}

/// Reads Compose files into a table.
struct Loader<'p> {
    map: BTreeMap<Vec<Keysym>, (Option<String>, Option<Keysym>)>,
    paths: Option<&'p Paths>,
    locale: String,
    depth: u32,
    includes: u32,
}

enum Tok<'a> {
    End,
    Colon,
    Bang,
    Tilde,
    Ident(&'a [u8]),
    Keysym(&'a [u8]),
    Str(String),
    Bad,
}

struct Lexer<'a> {
    line: &'a [u8],
    at: usize,
}

impl<'a> Lexer<'a> {
    fn peek(&self) -> Option<u8> {
        self.line.get(self.at).copied()
    }

    fn spaces(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r')) {
            self.at += 1;
        }
    }

    fn next(&mut self) -> Tok<'a> {
        self.spaces();
        let Some(c) = self.peek() else {
            return Tok::End;
        };
        let start = self.at;
        self.at += 1;
        match c {
            b'#' | 0 => {
                self.at = self.line.len();
                Tok::End
            }
            b':' => Tok::Colon,
            b'!' => Tok::Bang,
            b'~' => Tok::Tilde,
            b'<' => {
                while self.peek().is_some_and(|c| c != b'>') {
                    self.at += 1;
                }
                if self.peek().is_none() {
                    return Tok::Bad;
                }
                self.at += 1;
                Tok::Keysym(&self.line[start + 1..self.at - 1])
            }
            b'"' => self.string(),
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                while self
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
                {
                    self.at += 1;
                }
                Tok::Ident(&self.line[start..self.at])
            }
            _ => Tok::Bad,
        }
    }

    /// A string, after its opening quote: `\\`, `\"`, `\x` and one or two
    /// hex digits, `\` and up to three octal digits; any other escape is
    /// dropped and the character after it kept.
    fn string(&mut self) -> Tok<'a> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => return Tok::Bad,
                Some(b'"') => {
                    self.at += 1;
                    break;
                }
                Some(b'\\') => {
                    self.at += 1;
                    match self.peek() {
                        Some(c @ (b'\\' | b'"')) => {
                            out.push(c);
                            self.at += 1;
                        }
                        Some(b'x' | b'X') => {
                            self.at += 1;
                            if let Some(v) = self.digits(16, 2).filter(|&v| v != 0) {
                                out.push(v as u8);
                            }
                        }
                        _ => {
                            // A NUL, or an octal number past one byte, is dropped.
                            if let Some(v) = self.digits(8, 3).filter(|v| (1..=255).contains(v)) {
                                out.push(v as u8);
                            }
                        }
                    }
                }
                Some(c) => {
                    out.push(c);
                    self.at += 1;
                }
            }
        }
        match String::from_utf8(out) {
            Ok(s) => Tok::Str(s),
            Err(_) => Tok::Bad,
        }
    }

    /// Up to `max` digits in `radix`, as a number; `None` if there is no
    /// digit.
    fn digits(&mut self, radix: u32, max: usize) -> Option<u32> {
        let mut value: Option<u32> = None;
        for _ in 0..max {
            let Some(d) = self.peek().and_then(|c| (c as char).to_digit(radix)) else {
                break;
            };
            value = Some(value.unwrap_or(0) * radix + d);
            self.at += 1;
        }
        value
    }

    /// The path of an `include "..."`, with `%H`, `%L`, `%S` and `%%`
    /// replaced. `None` if it is malformed or names something not known.
    fn include_path(&mut self, loader: &Loader<'_>) -> Option<PathBuf> {
        self.spaces();
        if self.peek() != Some(b'"') {
            return None;
        }
        self.at += 1;
        let paths = loader.paths?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self.peek()?;
            self.at += 1;
            match c {
                b'"' => break,
                b'%' => {
                    let what = self.peek()?;
                    self.at += 1;
                    let part = match what {
                        b'%' => PathBuf::from("%"),
                        b'H' => paths.home.clone()?,
                        b'L' => paths.locale_file(&loader.locale)?,
                        b'S' => paths.locale_dir.clone(),
                        _ => return None,
                    };
                    out.extend_from_slice(part.to_string_lossy().as_bytes());
                }
                other => out.push(other),
            }
        }
        if !matches!(self.next(), Tok::End) {
            return None;
        }
        Some(PathBuf::from(String::from_utf8(out).ok()?))
    }
}

impl<'p> Loader<'p> {
    fn new(paths: Option<&'p Paths>, locale: &str) -> Loader<'p> {
        Loader {
            map: BTreeMap::new(),
            paths,
            locale: locale.to_string(),
            depth: 0,
            includes: 0,
        }
    }

    fn text(&mut self, bytes: &[u8]) {
        let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
        for line in bytes.split(|&b| b == b'\n') {
            self.line(line);
        }
    }

    /// One line: nothing, an include, or `sequence : result`. A line that
    /// cannot be read adds nothing.
    fn line(&mut self, line: &[u8]) {
        let mut lex = Lexer { line, at: 0 };
        let mut tok = lex.next();
        if let Tok::Ident(b"include") = tok {
            if self.depth < MAX_INCLUDE_DEPTH && self.includes < MAX_INCLUDES {
                self.includes += 1;
                if let Some(bytes) = lex.include_path(self).and_then(|p| read_file(&p).ok()) {
                    self.depth += 1;
                    self.text(&bytes);
                    self.depth -= 1;
                }
            }
            return;
        }
        // The sequence: `<keysym>`s, each after an optional list of
        // modifiers (`None`, or `!` and names with an optional `~`), which
        // is read and not used.
        let mut sequence: Vec<Keysym> = Vec::new();
        loop {
            match tok {
                Tok::Colon => break,
                Tok::Keysym(name) => {
                    let Some(sym) = std::str::from_utf8(name)
                        .ok()
                        .and_then(Keysym::from_name)
                        .filter(|s| *s != Keysym::NO_SYMBOL)
                    else {
                        return;
                    };
                    sequence.push(sym);
                }
                Tok::Bang => {}
                Tok::Tilde => match lex.next() {
                    Tok::Ident(name) if is_modifier_name(name) => {}
                    _ => return,
                },
                Tok::Ident(name) if name == b"None" || is_modifier_name(name) => {}
                _ => return,
            }
            tok = lex.next();
        }
        if sequence.is_empty() || sequence.len() > MAX_SEQUENCE {
            return;
        }
        // The result: a string, a keysym, or a string and then a keysym.
        // Whatever follows the keysym is not looked at.
        let named = |name: &[u8]| {
            std::str::from_utf8(name)
                .ok()
                .and_then(Keysym::from_name)
                .filter(|s| *s != Keysym::NO_SYMBOL)
        };
        let (text, keysym) = match lex.next() {
            Tok::Str(s) => match lex.next() {
                Tok::End => (Some(s), None),
                Tok::Ident(name) => match named(name) {
                    Some(sym) => (Some(s), Some(sym)),
                    None => return,
                },
                _ => return,
            },
            Tok::Ident(name) => match named(name) {
                Some(sym) => (None, Some(sym)),
                None => return,
            },
            _ => return,
        };
        // No sequence may be the start of another: a longer one replaces a
        // shorter one it extends, and a shorter one that would cut longer
        // ones off is left out.
        for n in 1..sequence.len() {
            self.map.remove(&sequence[..n]);
        }
        let extends = self
            .map
            .range(sequence.clone()..)
            .next()
            .is_some_and(|(longer, _)| {
                longer.len() > sequence.len() && longer.starts_with(&sequence)
            });
        if extends {
            return;
        }
        // The same sequence again replaces what the new line gives and
        // keeps the rest: a string alone leaves an earlier keysym in place.
        let entry = self.map.entry(sequence).or_default();
        if text.is_some() {
            entry.0 = text;
        }
        if keysym.is_some() {
            entry.1 = keysym;
        }
    }

    fn finish(self) -> Table {
        Table {
            entries: self
                .map
                .into_iter()
                .map(|(sequence, (text, keysym))| {
                    (sequence, text, keysym.unwrap_or(Keysym::NO_SYMBOL))
                })
                .collect(),
        }
    }
}

fn is_modifier_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"Ctrl" | b"Lock" | b"Caps" | b"Shift" | b"Alt" | b"Meta"
    )
}

/// Where a [`State`] is in a sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Not in a sequence: the last keysym is an ordinary key press.
    Nothing,
    /// In the middle of a sequence: the key presses so far type nothing yet.
    Composing,
    /// A sequence was completed: [`State::utf8`] and [`State::keysym`] have
    /// its result, which replaces the key press.
    Composed,
    /// The last keysym does not continue the sequence that was under way;
    /// the sequence and the keysym are dropped.
    Cancelled,
}

/// The progress through a Compose sequence as keys are pressed.
///
/// Feed it the keysym of every key press (not releases, not repeats of
/// modifiers), then act on the [`Status`]: type the result when `Composed`,
/// type nothing while `Composing` or when `Cancelled`, and handle the key as
/// usual when `Nothing`.
#[derive(Debug, Clone)]
pub struct State {
    table: Table,
    pressed: Vec<Keysym>,
    status: Status,
    /// The index of the completed entry, when `Composed`.
    result: Option<usize>,
}

/// Whether a keysym is a modifier key's: these never take part in a
/// sequence (Shift is pressed in the middle of many).
fn is_modifier(sym: Keysym) -> bool {
    // Shift_L to Hyper_R, ISO_Lock to ISO_Last_Group_Lock, Mode_switch,
    // Num_Lock: the keysyms libxkbcommon passes over. The ISO_Level5 keys
    // are not among them.
    matches!(sym.0, 0xffe1..=0xffee | 0xfe01..=0xfe0f | 0xff7e | 0xff7f)
}

impl State {
    /// A state at the start, for the sequences of `table`.
    pub fn new(table: Table) -> State {
        State {
            table,
            pressed: Vec::new(),
            status: Status::Nothing,
            result: None,
        }
    }

    /// The table this state follows.
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// Take the keysym of a key press and return the status after it. A
    /// modifier key's keysym changes nothing.
    pub fn feed(&mut self, keysym: Keysym) -> Status {
        if is_modifier(keysym) {
            return self.status;
        }
        if self.status != Status::Composing {
            self.pressed.clear();
        }
        self.result = None;
        self.pressed.push(keysym);
        self.status = match self.table.find(&self.pressed) {
            Found::Whole(at) => {
                self.result = Some(at);
                Status::Composed
            }
            Found::Start => Status::Composing,
            Found::Nothing if self.pressed.len() > 1 => Status::Cancelled,
            Found::Nothing => Status::Nothing,
        };
        self.status
    }

    /// Where the state is.
    pub fn status(&self) -> Status {
        self.status
    }

    /// Back to the start, forgetting any sequence under way.
    pub fn reset(&mut self) {
        self.pressed.clear();
        self.status = Status::Nothing;
        self.result = None;
    }

    /// The text of the sequence just completed: what the file gives, or the
    /// character of its keysym if it gives only a keysym. Empty unless the
    /// status is [`Status::Composed`].
    pub fn utf8(&self) -> String {
        match self.result.map(|at| &self.table.entries[at]) {
            Some((_, Some(text), _)) => text.clone(),
            Some((_, None, keysym)) => keysym.to_char().map(String::from).unwrap_or_default(),
            None => String::new(),
        }
    }

    /// The keysym of the sequence just completed: what the file gives, or
    /// [`Keysym::NO_SYMBOL`] if it gives only text or the status is not
    /// [`Status::Composed`].
    pub fn keysym(&self) -> Keysym {
        self.result
            .map_or(Keysym::NO_SYMBOL, |at| self.table.entries[at].2)
    }
}
