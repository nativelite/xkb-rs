# xkb-rs

XKB keymaps for a Wayland client, built on the Rust standard library alone.
One concern: turn the keycodes and modifier masks a compositor sends into
keysyms and text. It does what a client uses libxkbcommon for.

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

Text from another process is never trusted: a malformed keymap is an `Error`
with a line and a column, never a panic, and a keymap cannot ask for memory
out of proportion to its size.

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

## How it is checked

- Unit tests for each rule of the format, the errors, and damaged text
  (truncated and randomly mutated keymaps, fixed seed).
- Three complete keymaps (US, German, US with Russian) with the reference
  library's answers recorded beside them, checked on every platform.
- On Linux, a differential test against the system's `libxkbcommon.so.0`
  (loaded at run time, skipped where it is not installed): every layout and
  variant the system lists, every key, hundreds of modifier and layout
  states, and every keysym's name and character. See `docs/decision.md` for
  the numbers and for the one place this library deliberately differs (the
  case of letters follows Unicode).

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
