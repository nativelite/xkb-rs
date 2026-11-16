//! The keymap text format: tokens, and the statements of its sections.
//!
//! This reads the syntax only; `keymap.rs` gives the statements meaning.

use crate::Error;

/// Where a token starts: line and column, both from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct Pos {
    pub line: u32,
    pub col: u32,
}

pub(crate) fn err<T>(pos: Pos, message: impl Into<String>) -> Result<T, Error> {
    Err(Error {
        line: pos.line,
        column: pos.col,
        message: message.into(),
    })
}

#[derive(Clone, Debug, PartialEq)]
enum Tok<'a> {
    Ident(&'a str),
    Str(String),
    Key(&'a str),
    Int(u64),
    /// One of `; { } = [ ] ( ) . , + - * / ! ~`.
    Punct(u8),
    Eof,
}

struct Lexer<'a> {
    src: &'a str,
    at: usize,
    line: u32,
    line_start: usize,
}

impl<'a> Lexer<'a> {
    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            col: (self.at - self.line_start) as u32 + 1,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.as_bytes().get(self.at).copied()
    }

    fn next(&mut self) -> Result<(Tok<'a>, Pos), Error> {
        let b = self.src.as_bytes();
        // White space and comments (`//` and `#` to the end of the line).
        loop {
            match self.peek() {
                Some(b'\n') => {
                    self.at += 1;
                    self.line += 1;
                    self.line_start = self.at;
                }
                Some(c) if c.is_ascii_whitespace() => self.at += 1,
                Some(b'#') => self.skip_line(),
                Some(b'/') if b.get(self.at + 1) == Some(&b'/') => self.skip_line(),
                _ => break,
            }
        }
        let pos = self.pos();
        // A NUL ends the text: a keymap read from a file descriptor has one.
        let Some(c) = self.peek().filter(|&c| c != 0) else {
            return Ok((Tok::Eof, pos));
        };
        let start = self.at;
        match c {
            b'"' => {
                self.at += 1;
                let mut out: Vec<u8> = Vec::new();
                loop {
                    match self.peek() {
                        None | Some(0) | Some(b'\n') => return err(pos, "unterminated string"),
                        Some(b'"') => {
                            self.at += 1;
                            break;
                        }
                        Some(b'\\') => {
                            self.at += 1;
                            let Some(e) = self.peek().filter(|&e| e != 0 && e != b'\n') else {
                                return err(pos, "unterminated string");
                            };
                            self.at += 1;
                            match e {
                                b'n' => out.push(b'\n'),
                                b't' => out.push(b'\t'),
                                b'r' => out.push(b'\r'),
                                b'b' => out.push(8),
                                b'f' => out.push(12),
                                b'v' => out.push(11),
                                b'e' => out.push(27),
                                b'0'..=b'7' => {
                                    let mut v = u32::from(e - b'0');
                                    for _ in 0..2 {
                                        match self.peek() {
                                            Some(d @ b'0'..=b'7') => {
                                                v = v * 8 + u32::from(d - b'0');
                                                self.at += 1;
                                            }
                                            _ => break,
                                        }
                                    }
                                    // A NUL or a byte past 255 has no place
                                    // in a name; it is dropped.
                                    if (1..=0x7f).contains(&v) {
                                        out.push(v as u8);
                                    } else if let Some(c) =
                                        char::from_u32(v).filter(|_| (0x80..=0xff).contains(&v))
                                    {
                                        let mut buf = [0; 4];
                                        out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                                    }
                                }
                                other => out.push(other),
                            }
                        }
                        Some(other) => {
                            out.push(other);
                            self.at += 1;
                        }
                    }
                }
                Ok((Tok::Str(String::from_utf8_lossy(&out).into_owned()), pos))
            }
            b'<' => {
                self.at += 1;
                let from = self.at;
                while self
                    .peek()
                    .is_some_and(|c| c.is_ascii_graphic() && c != b'>')
                {
                    self.at += 1;
                }
                if self.peek() != Some(b'>') {
                    return err(pos, "unterminated key name");
                }
                let name = &self.src[from..self.at];
                self.at += 1;
                Ok((Tok::Key(name), pos))
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => {
                while self
                    .peek()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
                {
                    self.at += 1;
                }
                Ok((Tok::Ident(&self.src[start..self.at]), pos))
            }
            b'0'..=b'9' => {
                let hex = c == b'0' && matches!(b.get(self.at + 1), Some(b'x' | b'X'));
                let (radix, from) = if hex {
                    (16, self.at + 2)
                } else {
                    (10, self.at)
                };
                self.at = from;
                while self.peek().is_some_and(|c| (c as char).is_digit(radix)) {
                    self.at += 1;
                }
                let Ok(v) = u64::from_str_radix(&self.src[from..self.at], radix) else {
                    return err(pos, "malformed number");
                };
                // A fraction (geometry has them) is read and dropped.
                if !hex && self.peek() == Some(b'.') {
                    self.at += 1;
                    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.at += 1;
                    }
                }
                Ok((Tok::Int(v), pos))
            }
            b';' | b'{' | b'}' | b'=' | b'[' | b']' | b'(' | b')' | b'.' | b',' | b'+' | b'-'
            | b'*' | b'/' | b'!' | b'~' => {
                self.at += 1;
                Ok((Tok::Punct(c), pos))
            }
            _ => err(pos, "unexpected character"),
        }
    }

    fn skip_line(&mut self) {
        while self.peek().is_some_and(|c| c != b'\n' && c != 0) {
            self.at += 1;
        }
    }
}

/// An expression: a value on either side of an `=`, or an item of a list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Expr<'a> {
    Ident(&'a str),
    Str(String),
    Int(u64),
    Key(&'a str),
    /// `a.b`
    Field(&'a str, &'a str),
    /// `a[i]`, `a.b[i]`
    Index(Box<Expr<'a>>, Box<Expr<'a>>),
    /// `Name(arg, field=value, ...)`: an action, or an interpret's match.
    Call(&'a str, Vec<Expr<'a>>),
    /// `!a` or `~a`
    Not(Box<Expr<'a>>),
    /// `-a`
    Neg(Box<Expr<'a>>),
    /// `a + b`, `a - b`, `a * b`, `a / b`
    Bin(u8, Box<Expr<'a>>, Box<Expr<'a>>),
    /// `[ a, b, ... ]`
    List(Vec<Expr<'a>>),
    /// `{ a, b, ... }` inside a list: several keysyms on one level.
    Set(Vec<Expr<'a>>),
}

/// One line of a block: `lhs = value`, or a bare `value`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item<'a> {
    pub pos: Pos,
    pub lhs: Option<Expr<'a>>,
    pub value: Expr<'a>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Stmt<'a> {
    /// `lhs = value;` or a bare `value;` at the top of a section.
    Item(Item<'a>),
    /// `alias <A> = <B>;`
    Alias(&'a str, &'a str),
    /// `indicator 3 = "Scroll Lock";`
    LedName(u64, String),
    /// `virtual_modifiers A, B = Mod2;`
    VMods(Vec<(&'a str, Option<Expr<'a>>)>),
    /// `type "NAME" { ... };`
    Type(String, Vec<Item<'a>>),
    /// `interpret sym+Match(mods) { ... };`: the keysym, the match, the body.
    Interpret(Expr<'a>, Option<Expr<'a>>, Vec<Item<'a>>),
    /// `indicator "Caps Lock" { ... };`
    LedMap(String, Vec<Item<'a>>),
    /// `key <NAME> { ... };`
    Key(&'a str, Vec<Item<'a>>),
    /// `modifier_map Shift { <LFSH>, <RTSH> };`
    ModMap(&'a str, Vec<Expr<'a>>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    Keycodes,
    Types,
    Compat,
    Symbols,
}

/// A keymap's sections, each a list of positioned statements.
#[derive(Debug, Default)]
pub(crate) struct File<'a> {
    pub sections: Vec<(Section, Vec<(Pos, Stmt<'a>)>)>,
}

/// How deep expressions and blocks may nest: a keymap needs three or four
/// levels, and text from another process must not be able to exhaust the
/// stack.
const MAX_DEPTH: u32 = 64;

struct Parser<'a> {
    lex: Lexer<'a>,
    tok: Tok<'a>,
    pos: Pos,
    depth: u32,
}

fn is(word: &str, keyword: &str) -> bool {
    word.eq_ignore_ascii_case(keyword)
}

impl<'a> Parser<'a> {
    fn bump(&mut self) -> Result<Tok<'a>, Error> {
        let (tok, pos) = self.lex.next()?;
        self.pos = pos;
        Ok(std::mem::replace(&mut self.tok, tok))
    }

    fn at(&self, p: u8) -> bool {
        self.tok == Tok::Punct(p)
    }

    fn eat(&mut self, p: u8) -> Result<bool, Error> {
        if self.at(p) {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn expect(&mut self, p: u8) -> Result<(), Error> {
        if self.eat(p)? {
            Ok(())
        } else {
            err(self.pos, format!("expected `{}`", p as char))
        }
    }

    fn ident(&mut self) -> Result<&'a str, Error> {
        match self.tok {
            Tok::Ident(s) => {
                self.bump()?;
                Ok(s)
            }
            _ => err(self.pos, "expected a name"),
        }
    }

    fn string(&mut self) -> Result<String, Error> {
        match self.bump()? {
            Tok::Str(s) => Ok(s),
            _ => err(self.pos, "expected a string"),
        }
    }

    fn key(&mut self) -> Result<&'a str, Error> {
        match self.tok {
            Tok::Key(s) => {
                self.bump()?;
                Ok(s)
            }
            _ => err(self.pos, "expected a key name"),
        }
    }

    fn deeper(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return err(self.pos, "nested too deeply");
        }
        Ok(())
    }

    fn expr(&mut self) -> Result<Expr<'a>, Error> {
        self.deeper()?;
        let mut lhs = self.unary()?;
        while let Tok::Punct(op @ (b'+' | b'-' | b'*' | b'/')) = self.tok {
            self.bump()?;
            let rhs = self.unary()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        self.depth -= 1;
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr<'a>, Error> {
        self.deeper()?;
        let e = match self.tok {
            Tok::Punct(b'!' | b'~') => {
                self.bump()?;
                Expr::Not(Box::new(self.unary()?))
            }
            Tok::Punct(b'-') => {
                self.bump()?;
                Expr::Neg(Box::new(self.unary()?))
            }
            Tok::Punct(b'+') => {
                self.bump()?;
                self.unary()?
            }
            _ => self.primary()?,
        };
        self.depth -= 1;
        Ok(e)
    }

    /// Comma-separated expressions up to the closing `close`, which is
    /// taken. In a list, `{ ... }` is a set and `a = b` is an argument.
    fn list(&mut self, close: u8) -> Result<Vec<Expr<'a>>, Error> {
        let mut out = Vec::new();
        while !self.eat(close)? {
            if self.eat(b'{')? {
                self.deeper()?;
                let set = self.list(b'}')?;
                self.depth -= 1;
                out.push(Expr::Set(set));
            } else {
                let e = self.expr()?;
                // `field = value` in an action's arguments: the value is
                // read and dropped, the field kept.
                if self.eat(b'=')? {
                    self.expr()?;
                }
                out.push(e);
            }
            if !self.at(close) {
                self.expect(b',')?;
            }
        }
        Ok(out)
    }

    fn primary(&mut self) -> Result<Expr<'a>, Error> {
        let pos = self.pos;
        match self.bump()? {
            Tok::Int(v) => Ok(Expr::Int(v)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Key(k) => Ok(Expr::Key(k)),
            Tok::Punct(b'(') => {
                let e = self.expr()?;
                self.expect(b')')?;
                Ok(e)
            }
            Tok::Punct(b'[') => Ok(Expr::List(self.list(b']')?)),
            Tok::Ident(name) => {
                if self.eat(b'(')? {
                    return Ok(Expr::Call(name, self.list(b')')?));
                }
                let mut e = Expr::Ident(name);
                if self.eat(b'.')? {
                    e = Expr::Field(name, self.ident()?);
                }
                if self.eat(b'[')? {
                    let index = self.expr()?;
                    self.expect(b']')?;
                    e = Expr::Index(Box::new(e), Box::new(index));
                }
                Ok(e)
            }
            _ => err(pos, "expected a value"),
        }
    }

    /// `lhs = value` or a bare value.
    fn item(&mut self) -> Result<Item<'a>, Error> {
        let pos = self.pos;
        let first = self.expr()?;
        if self.eat(b'=')? {
            let value = self.expr()?;
            Ok(Item {
                pos,
                lhs: Some(first),
                value,
            })
        } else {
            Ok(Item {
                pos,
                lhs: None,
                value: first,
            })
        }
    }

    /// The items of a `{ ... }` block, separated by `;` or `,`; the opening
    /// brace is already taken, the closing one is taken here.
    fn block(&mut self) -> Result<Vec<Item<'a>>, Error> {
        let mut out = Vec::new();
        while !self.eat(b'}')? {
            out.push(self.item()?);
            if !self.at(b'}') && !self.eat(b';')? {
                self.expect(b',')?;
            }
        }
        Ok(out)
    }

    fn statement(&mut self) -> Result<Stmt<'a>, Error> {
        let pos = self.pos;
        // A merge mode before a statement changes nothing in a complete
        // keymap, where nothing is defined twice.
        while let Tok::Ident(w) = self.tok {
            if is(w, "include") {
                return err(
                    pos,
                    "`include` needs the keymap's source files; a complete keymap has none",
                );
            }
            if ["augment", "override", "replace", "alternate"]
                .iter()
                .any(|k| is(w, k))
            {
                self.bump()?;
            } else {
                break;
            }
        }
        let stmt = match self.tok {
            Tok::Ident(w) if is(w, "virtual_modifiers") => {
                self.bump()?;
                let mut names = Vec::new();
                loop {
                    let name = self.ident()?;
                    let value = if self.eat(b'=')? {
                        Some(self.expr()?)
                    } else {
                        None
                    };
                    names.push((name, value));
                    if !self.eat(b',')? {
                        break;
                    }
                }
                Stmt::VMods(names)
            }
            Tok::Ident(w) if is(w, "alias") => {
                self.bump()?;
                let alias = self.key()?;
                self.expect(b'=')?;
                Stmt::Alias(alias, self.key()?)
            }
            Tok::Ident(w) if is(w, "virtual") || is(w, "indicator") => {
                if is(w, "virtual") {
                    self.bump()?;
                    if !matches!(self.tok, Tok::Ident(i) if is(i, "indicator")) {
                        return err(self.pos, "expected `indicator`");
                    }
                }
                self.led()?
            }
            Tok::Ident(w) if is(w, "type") => {
                self.bump()?;
                if let Tok::Str(_) = self.tok {
                    let name = self.string()?;
                    self.expect(b'{')?;
                    Stmt::Type(name, self.block()?)
                } else {
                    self.rest_of_item(pos, w)?
                }
            }
            Tok::Ident(w) if is(w, "interpret") => {
                self.bump()?;
                if self.at(b'.') {
                    self.rest_of_item(pos, w)?
                } else {
                    let sym = match self.bump()? {
                        Tok::Ident(s) => Expr::Ident(s),
                        Tok::Int(v) => Expr::Int(v),
                        _ => return err(self.pos, "expected a keysym"),
                    };
                    let matching = if self.eat(b'+')? {
                        Some(self.expr()?)
                    } else {
                        None
                    };
                    self.expect(b'{')?;
                    Stmt::Interpret(sym, matching, self.block()?)
                }
            }
            Tok::Ident(w) if is(w, "key") => {
                self.bump()?;
                if let Tok::Key(_) = self.tok {
                    let name = self.key()?;
                    self.expect(b'{')?;
                    Stmt::Key(name, self.block()?)
                } else {
                    self.rest_of_item(pos, w)?
                }
            }
            Tok::Ident(w) if is(w, "modifier_map") || is(w, "modmap") || is(w, "mod_map") => {
                self.bump()?;
                let name = self.ident()?;
                self.expect(b'{')?;
                Stmt::ModMap(name, self.list(b'}')?)
            }
            Tok::Ident(w) if is(w, "group") => {
                self.bump()?;
                if let Tok::Int(_) = self.tok {
                    // `group 2 = AltGr;` (compatibility): nothing a client uses.
                    self.bump()?;
                    self.expect(b'=')?;
                    self.expr()?;
                    Stmt::Item(Item {
                        pos,
                        lhs: None,
                        value: Expr::Ident(w),
                    })
                } else {
                    self.rest_of_item(pos, w)?
                }
            }
            _ => Stmt::Item(self.item()?),
        };
        self.expect(b';')?;
        Ok(stmt)
    }

    /// A statement that began with a keyword but is a plain `keyword.field =
    /// value` or `keyword[i] = value`; the keyword is already taken.
    fn rest_of_item(&mut self, pos: Pos, word: &'a str) -> Result<Stmt<'a>, Error> {
        let mut lhs = Expr::Ident(word);
        if self.eat(b'.')? {
            lhs = Expr::Field(word, self.ident()?);
        }
        if self.eat(b'[')? {
            let index = self.expr()?;
            self.expect(b']')?;
            lhs = Expr::Index(Box::new(lhs), Box::new(index));
        }
        if self.eat(b'=')? {
            let value = self.expr()?;
            Ok(Stmt::Item(Item {
                pos,
                lhs: Some(lhs),
                value,
            }))
        } else {
            Ok(Stmt::Item(Item {
                pos,
                lhs: None,
                value: lhs,
            }))
        }
    }

    /// After `indicator`: a name for a number (keycodes), a map for a name
    /// (compatibility), or a default (`indicator.field = value`).
    fn led(&mut self) -> Result<Stmt<'a>, Error> {
        let pos = self.pos;
        let word = self.ident()?;
        match self.tok {
            Tok::Int(index) => {
                self.bump()?;
                self.expect(b'=')?;
                Ok(Stmt::LedName(index, self.string()?))
            }
            Tok::Str(_) => {
                let name = self.string()?;
                self.expect(b'{')?;
                Ok(Stmt::LedMap(name, self.block()?))
            }
            _ => self.rest_of_item(pos, word),
        }
    }

    /// Skip a `{ ... }` block whose opening brace is already taken.
    fn skip_block(&mut self) -> Result<(), Error> {
        let mut open = 1u32;
        while open > 0 {
            match self.bump()? {
                Tok::Punct(b'{') => open += 1,
                Tok::Punct(b'}') => open -= 1,
                Tok::Eof => return err(self.pos, "unexpected end of the keymap"),
                _ => {}
            }
        }
        Ok(())
    }

    fn file(&mut self) -> Result<File<'a>, Error> {
        let mut file = File::default();
        self.flags()?;
        let word = self.ident()?;
        if !is(word, "xkb_keymap") {
            return err(self.pos, "expected `xkb_keymap`");
        }
        if let Tok::Str(_) = self.tok {
            self.bump()?;
        }
        self.expect(b'{')?;
        while !self.eat(b'}')? {
            self.flags()?;
            let pos = self.pos;
            let word = self.ident()?;
            let section = if is(word, "xkb_keycodes") {
                Some(Section::Keycodes)
            } else if is(word, "xkb_types") {
                Some(Section::Types)
            } else if [
                "xkb_compatibility",
                "xkb_compatibility_map",
                "xkb_compat",
                "xkb_compat_map",
            ]
            .iter()
            .any(|k| is(word, k))
            {
                Some(Section::Compat)
            } else if is(word, "xkb_symbols") {
                Some(Section::Symbols)
            } else if is(word, "xkb_geometry") {
                None
            } else {
                return err(pos, "expected a keymap section");
            };
            if let Tok::Str(_) = self.tok {
                self.bump()?;
            }
            self.expect(b'{')?;
            match section {
                None => self.skip_block()?,
                Some(section) => {
                    if file.sections.iter().any(|(s, _)| *s == section) {
                        return err(pos, "a section given twice");
                    }
                    let mut statements = Vec::new();
                    while !self.eat(b'}')? {
                        let pos = self.pos;
                        statements.push((pos, self.statement()?));
                    }
                    file.sections.push((section, statements));
                }
            }
            self.expect(b';')?;
        }
        self.eat(b';')?;
        if self.tok != Tok::Eof {
            return err(self.pos, "text after the keymap");
        }
        Ok(file)
    }

    /// The flags a section may carry before its keyword.
    fn flags(&mut self) -> Result<(), Error> {
        const FLAGS: [&str; 9] = [
            "default",
            "partial",
            "hidden",
            "alphanumeric_keys",
            "modifier_keys",
            "keypad_keys",
            "function_keys",
            "alternate_group",
            "virtual",
        ];
        while matches!(self.tok, Tok::Ident(w) if FLAGS.iter().any(|f| is(w, f))) {
            self.bump()?;
        }
        Ok(())
    }
}

/// Read a keymap's text into its sections.
pub(crate) fn parse(src: &str) -> Result<File<'_>, Error> {
    let mut p = Parser {
        lex: Lexer {
            src,
            at: 0,
            line: 1,
            line_start: 0,
        },
        tok: Tok::Eof,
        pos: Pos::default(),
        depth: 0,
    };
    p.bump()?;
    p.file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stmts(body: &str) -> Vec<Stmt<'_>> {
        // Leaked so the statements can borrow from it; a test's worth.
        let src: &'static str =
            Box::leak(format!("xkb_keymap {{ xkb_symbols {{ {body} }}; }};").into_boxed_str());
        let mut file = parse(src).unwrap();
        file.sections
            .remove(0)
            .1
            .into_iter()
            .map(|(_, s)| s)
            .collect()
    }

    #[test]
    fn tokens() {
        let s = stmts(
            "// a comment\n# another\nname[Group1] = \"a\\tb\\101\\\"\"; <AE01> = 0x0A; x = 12.5;",
        );
        assert_eq!(s.len(), 3);
        let Stmt::Item(i) = &s[0] else { panic!() };
        assert_eq!(i.value, Expr::Str("a\tbA\"".into()));
        let Stmt::Item(i) = &s[1] else { panic!() };
        assert_eq!(i.lhs, Some(Expr::Key("AE01")));
        assert_eq!(i.value, Expr::Int(10));
        let Stmt::Item(i) = &s[2] else { panic!() };
        assert_eq!(i.value, Expr::Int(12));
    }

    #[test]
    fn a_key_with_every_kind_of_item() {
        let s = stmts(
            "key <AD01> { type[Group1]= \"ALPHABETIC\", symbols[Group1]= [ q, { Q, 1 } ], \
             actions[Group1]= [ NoAction(), SetMods(modifiers=Shift,clearLocks) ], \
             repeat= No, groupsClamp, !locking, [ a ] };",
        );
        let Stmt::Key(name, items) = &s[0] else {
            panic!()
        };
        assert_eq!(*name, "AD01");
        assert_eq!(items.len(), 7);
        assert_eq!(
            items[1].value,
            Expr::List(vec![
                Expr::Ident("q"),
                Expr::Set(vec![Expr::Ident("Q"), Expr::Int(1)])
            ])
        );
        assert_eq!(
            items[2].value,
            Expr::List(vec![
                Expr::Call("NoAction", vec![]),
                Expr::Call(
                    "SetMods",
                    vec![Expr::Ident("modifiers"), Expr::Ident("clearLocks")]
                )
            ])
        );
        assert_eq!(
            items[4],
            Item {
                pos: items[4].pos,
                lhs: None,
                value: Expr::Ident("groupsClamp")
            }
        );
        assert_eq!(items[5].value, Expr::Not(Box::new(Expr::Ident("locking"))));
        assert_eq!(items[6].value, Expr::List(vec![Expr::Ident("a")]));
    }

    #[test]
    fn the_other_statements() {
        let s = stmts(
            "virtual_modifiers NumLock, Alt=Mod1; alias <A> = <B>; indicator 1 = \"Caps Lock\"; \
             virtual indicator 2 = \"x\"; indicator \"Caps Lock\" { modifiers= Lock; !allowExplicit; }; \
             type \"T\" { modifiers= Shift+Lock; map[Shift]= Level2; }; \
             interpret Num_Lock+AnyOf(all) { virtualModifier= NumLock; }; interpret.repeat= False; \
             interpret Any + Any { repeat= True; }; modifier_map Shift { <LFSH>, Shift_R }; \
             override key.type = \"X\"; group 2 = AltGr; indicator.allowExplicit = True;",
        );
        assert_eq!(s.len(), 13);
        assert!(
            matches!(&s[0], Stmt::VMods(v) if v.len() == 2 && v[1].1 == Some(Expr::Ident("Mod1")))
        );
        assert_eq!(s[1], Stmt::Alias("A", "B"));
        assert_eq!(s[2], Stmt::LedName(1, "Caps Lock".into()));
        assert_eq!(s[3], Stmt::LedName(2, "x".into()));
        assert!(matches!(&s[4], Stmt::LedMap(n, b) if n == "Caps Lock" && b.len() == 2));
        assert!(matches!(&s[5], Stmt::Type(n, b) if n == "T" && b.len() == 2));
        assert!(
            matches!(&s[6], Stmt::Interpret(Expr::Ident("Num_Lock"), Some(Expr::Call("AnyOf", _)), b) if b.len() == 1)
        );
        assert!(
            matches!(&s[7], Stmt::Item(i) if i.lhs == Some(Expr::Field("interpret", "repeat")))
        );
        assert!(matches!(
            &s[8],
            Stmt::Interpret(Expr::Ident("Any"), Some(Expr::Ident("Any")), _)
        ));
        assert!(matches!(&s[9], Stmt::ModMap("Shift", l) if l.len() == 2));
        assert!(matches!(&s[10], Stmt::Item(i) if i.lhs == Some(Expr::Field("key", "type"))));
    }

    fn fails(src: &str) -> Error {
        parse(src).unwrap_err()
    }

    #[test]
    fn errors_say_where() {
        let e = fails("xkb_keymap {\n  xkb_symbols {\n    key <A> { [ a }\n  };\n};");
        assert_eq!((e.line, e.column), (3, 19), "{e}");
        assert!(e.to_string().contains("3:19"), "{e}");
        assert_eq!(fails("").message, "expected a name");
        assert_eq!(
            fails("xkb_keymap { xkb_symbols { name = \"a").message,
            "unterminated string"
        );
        assert_eq!(
            fails("xkb_keymap { xkb_symbols { <A = 1; }; };").message,
            "unterminated key name"
        );
        assert_eq!(
            fails("xkb_keymap { xkb_symbols { a = $; }; };").message,
            "unexpected character"
        );
        assert_eq!(
            fails("xkb_keymap { xkb_symbols { a = 99999999999999999999; }; };").message,
            "malformed number"
        );
        assert_eq!(
            fails("xkb_keymap { xkb_bogus { }; };").message,
            "expected a keymap section"
        );
        assert_eq!(
            fails("xkb_keymap { }; more").message,
            "text after the keymap"
        );
        assert_eq!(
            fails("xkb_keymap { xkb_types { }; xkb_types { }; };").message,
            "a section given twice"
        );
        assert!(fails("xkb_keymap { xkb_symbols { include \"us\" }; };")
            .message
            .contains("include"));
        let deep = format!(
            "xkb_keymap {{ xkb_symbols {{ a = {}1; }}; }};",
            "(".repeat(100_000)
        );
        assert_eq!(fails(&deep).message, "nested too deeply");
        let deep = format!(
            "xkb_keymap {{ xkb_symbols {{ a = [{}; }}; }};",
            "{".repeat(100_000)
        );
        assert_eq!(fails(&deep).message, "nested too deeply");
        let deep = format!(
            "xkb_keymap {{ xkb_symbols {{ a = {}1; }}; }};",
            "!".repeat(100_000)
        );
        assert_eq!(fails(&deep).message, "nested too deeply");
    }

    #[test]
    fn geometry_is_skipped_and_a_nul_ends_the_text() {
        let f = parse("xkb_keymap \"n\" { xkb_geometry \"g\" { shape \"x\" { { [1.5,2] } }; }; xkb_types { }; };\0junk").unwrap();
        assert_eq!(f.sections.len(), 1);
    }
}
