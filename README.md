# xkb-rs

XKB keymaps for a Wayland client, built on the Rust standard library alone.
One concern: turn the keycodes and modifier masks a compositor sends into
keysyms and text. It does what a client uses libxkbcommon for.

Crate `nativelite-xkb` on crates.io, library name `xkb`:

```toml
[dependencies]
xkb = { package = "nativelite-xkb", version = "0.1" }
```

- `Keymap` reads the keymap text a compositor sends over `wl_keyboard.keymap`
  (the `xkb_keymap { ... }` format libxkbcommon and `xkbcomp` write): key
  names and aliases, key types with their level maps and preserved
  modifiers, the compatibility section's interprets (for virtual modifiers
  and key repeat) and LED maps, symbols with any number of layouts and
  levels, several keysyms on a level, and the modifier map. Types a key does
  not name are inferred as libxkbcommon infers them.
- `State` holds the modifiers and layout from `wl_keyboard.modifiers` and
  answers, per key: the layout and level in effect, the keysyms, the one
  keysym with Caps Lock applied, the text (UTF-8 or a code point) with the
  Control transformation, the modifiers the key consumed, whether it
  repeats, and which LEDs are lit.
- `Keysym` names keysyms (`"Return"`, `"eacute"`, `"U20AC"`, `"0x1008ff11"`),
  converts them to and from characters, and changes their case.
- `compose::Table` reads X11 Compose files (dead keys and Compose-key
  sequences): the file of a locale through `compose.dir` and `locale.alias`,
  or the user's `$XCOMPOSEFILE`, `$XDG_CONFIG_HOME/XCompose` or
  `~/.XCompose`, with `include` and its `%L`, `%H`, `%S`. `compose::State`
  is fed each key press's keysym and says whether a sequence is under way,
  completed (with its text and keysym) or cancelled.

Text from another process is never trusted: a malformed keymap is an `Error`
with a line and a column, never a panic. A keymap cannot ask for memory or
time out of proportion to its size, and no expression, however long or deep,
can overflow the stack (nesting is cut off at 64 levels; a chain of
operators is not nesting). A Compose file is read only if it is a regular
file of at most 16 MiB (`compose::MAX_FILE_SIZE`), and so is each file it
includes.

Where a keymap says the same thing twice, or says something unusual, it is
read as libxkbcommon 1.6 reads it: a key, type, interpret or LED map given
again is merged with the earlier one (`augment`, `override` and `replace`
before a statement are honoured), `key.field = value` and
`indicator.field = value` set defaults for the statements after them, and
so on down to the small print. `tests/handwritten/cases.txt` holds some 300
such texts, each compared with the reference.

**Not implemented:**

- Compiling a keymap from layout names (rules, model, layout, variant,
  options) out of the xkeyboard-config files. A Wayland client does not need
  it: the compositor sends the compiled keymap.
- The compositor's side of the state: turning key presses into modifier and
  layout changes (`xkb_state_update_key`, key actions, latches, locks).
  `State` is set from the masks the compositor reports.
- `include` statements, the geometry section (skipped), and keyboard
  controls: an LED tied to a control (`controls = ...`) never lights.
- The GTK flavour of consumed modifiers; only the XKB one is here.
- Looking keysyms up without regard to case.
- Compose: modifiers before a keysym (`Ctrl <a>`) are read and ignored, as
  libxkbcommon ignores them; sequences longer than ten keysyms are left
  out, as there. A line that cannot be read is left out without a report.
  In a Compose file that is not UTF-8 the lines with non-UTF-8 strings are
  left out (libxkbcommon rejects such a file whole).

**More lenient than libxkbcommon.** Some text libxkbcommon refuses is read
here. No case was found where both read a text and disagree about it; these
are texts only this library reads:

- Keymaps:
  - more than four layouts on a key (up to 32), and levels named past
    `Level8` (up to 2048);
  - a field of a key, a type or an interpret that makes no sense (an unknown
    name, a value of the wrong kind, symbols for a layout that has them
    already) is passed over even where it is the last of its body, which is
    the one place libxkbcommon refuses it; an unknown statement in a section
    is passed over too;
  - the names of the real modifiers in any case (`shift`, `MOD3`);
  - an empty `{ }` level, `AnyOf()` with nothing in it, a `{ ... }` of
    actions on one level;
  - `\"` inside a string, `0X` for hexadecimal, a fraction after a number;
  - a comma after the last item of a list, `;` and `,` used for each other
    between the items of a body, no `;` after the keymap.
- Compose files:
  - a file with an `include` that cannot be read, or with more than ten
    lines that cannot be read, still gives its good lines (libxkbcommon
    refuses the file);
  - result strings of 255 bytes and more are kept;
  - malformed modifier lists before a keysym (`! None <e>`, `None Ctrl <a>`,
    a modifier after the keysym) are read and ignored like well-formed ones.

**Zero runtime dependencies** — `[dependencies]` is empty, and a guard fails
the check if that ever changes.

## Example

```rust
use xkb::{Keymap, Keysym, State};

// In a client this is the text of the wl_keyboard.keymap file:
// `Keymap::from_bytes(&bytes)`.
let keymap: Keymap = r#"
xkb_keymap {
    xkb_keycodes { <AD01> = 24; <LFSH> = 50; };
    xkb_types {
        type "ALPHABETIC" {
            modifiers = Shift+Lock;
            map[Shift] = Level2;
            map[Lock] = Level2;
        };
    };
    xkb_compatibility { };
    xkb_symbols {
        key <AD01> { [ q, Q ] };
        key <LFSH> { [ Shift_L ] };
        modifier_map Shift { <LFSH> };
    };
};"#
.parse()
.unwrap();

let mut state = State::new(keymap);
assert_eq!(state.key_one_sym(24), Keysym::from_name("q").unwrap());
assert_eq!(state.key_utf8(24), "q");

// wl_keyboard.modifiers(depressed = Shift, latched = 0, locked = 0, group = 0)
state.update_mask(1, 0, 0, 0, 0, 0);
assert_eq!(state.key_utf8(24), "Q");
```

A Wayland keycode is the evdev scancode plus 8; `wl_keyboard.key` sends the
scancode, so add 8 before asking.

Dead keys and the Compose key: feed each key press's keysym to a
`compose::State` first, and type what it says.

```rust
use xkb::compose::{State, Status, Table};
use xkb::Keysym;

// In a client: Table::from_locale("en_US.UTF-8", &Paths::from_env()).
let table = Table::parse("<dead_acute> <e> : \"é\" eacute\n");
let mut compose = State::new(table);
let key = |name| Keysym::from_name(name).unwrap();
assert_eq!(compose.feed(key("dead_acute")), Status::Composing); // type nothing
assert_eq!(compose.feed(key("e")), Status::Composed);
assert_eq!(compose.utf8(), "é"); // type this instead of the key
```

## How it is checked

- Unit tests for each rule of the two formats, the errors, where Compose
  files are found, and damaged text (truncated and randomly mutated keymaps
  and Compose files, fixed seed).
- Three complete keymaps (US, German, US with Russian) with the reference
  library's answers recorded beside them, checked on every platform.
- Keymap text written by hand in the forms libxkbcommon's own writer never
  produces (`tests/handwritten/cases.txt`), with the expected answers for
  the main ones in `tests/odd_texts.rs`; text made to be costly (a chain of
  200,000 operators read on a 2 MB stack; texts that used to take time in
  proportion to the square of their size).
- On Linux, a differential test against the system's `libxkbcommon.so.0`
  (loaded at run time, skipped where it is not installed): every layout and
  variant the system lists, every key, hundreds of modifier and layout
  states, every keysym's name and character, the Compose tables of sixteen
  locales walked sequence by sequence, and each of the hand-written texts
  (both libraries must agree whether it is a keymap and, where it is, on
  everything about it). See `docs/decision.md` for the numbers and for the
  one place this library deliberately differs (the case of letters follows
  Unicode).

## Develop

```sh
python dev.py check   # dependency guard + cargo fmt --check + cargo test
python dev.py test    # cargo test
```

`tools/gen_keysyms.py` regenerates the keysym table (`src/keysym_table.rs`)
from the keysym header and the reference library; it is not part of the
build.

## Why

Keyboard input on Wayland is a text format and a table lookup; it needs no
C library. This crate builds on `str` parsing, `Vec`, and `char`'s Unicode
case mapping from the standard library.

## License

MIT. Keysym names and numbers come from the X11 keysym definitions, and the
test fixtures from xkeyboard-config: see `THIRD-PARTY-NOTICES.md`.
