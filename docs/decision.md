# Decision record: our own XKB for Wayland clients

*2026-11-15. Status: built, 0.1 unreleased.*

## The need

foldwave's Linux window (fwrender on Wayland) gets its keyboard as a keymap
in XKB's text format plus keycodes and modifier masks. Something has to turn
those into keysyms and text. Everywhere else that something is libxkbcommon,
a C library; nativelite ships no third-party code.

## Prior art

| Project | Licence | What it is |
| --- | --- | --- |
| [libxkbcommon](https://github.com/xkbcommon/libxkbcommon) | MIT (X11-style) | The reference. C. Compiles keymaps from layout names or from text, keeps state for both compositor and client, Compose. |
| [xkbcommon-rs / `xkbcommon`](https://github.com/rust-x-bindings/xkbcommon-rs) | MIT | Rust bindings to the C library. |
| [`xkeysym`](https://github.com/rust-windowing/xkeysym) | MIT / Apache-2.0 / Zlib | Keysym names and numbers in Rust, no keymaps. |
| [kbvm](https://github.com/mahkoh/kbvm) | MIT / Apache-2.0 | A full Rust implementation of the XKB specification. Thirteen third-party dependencies in its manifest (`hashbrown`, `bstr`, `smallvec`, ...). *Its source was not read for this work.* |

## Is their way the best way for us?

libxkbcommon does three jobs: compile a keymap from layout names (for
compositors), run the key-press state machine (for compositors), and read a
keymap and answer questions about keys (for clients). A Wayland client only
ever does the third: the compositor compiles the keymap, sends its text, and
reports the modifier state as masks. The third job is a parser and table
lookups.

So: **reimplement, client side only.** Inspiration, not a port: no code was
copied or translated. The format and the rules (how a key without a type gets
one, how interprets give keys their virtual modifiers, how a level is
chosen) are behaviour every reader of the format must share, and were written
from knowledge of the format and then corrected against the reference's
observable answers. Facts taken from other projects (keysym names, numbers,
code points; three test keymaps) are listed in `THIRD-PARTY-NOTICES.md`.

## What is in 0.1 and what is not

In: keysyms; the keymap text format as compositors send it; client state
(`update_mask`); keysyms, text, consumed modifiers, repeat, LEDs.

Out, with reasons:

- **Keymap from layout names (RMLVO).** Needs the xkeyboard-config rules and
  include machinery; only a compositor (or a client inventing its own
  keymap) needs it.
- **Server-side state (`update_key`, actions).** The compositor owns it.
  Actions are parsed and dropped.
- **Controls, GTK consumed mode, case-insensitive keysym lookup.** No use in
  fwrender; small to add when one appears.

## One deliberate difference: case

A key without a type is typed by whether its first keysyms are a lower/upper
pair, and Caps Lock upper-cases a keysym the key's type does not handle.
libxkbcommon up to 1.7 cases keysyms with tables inherited from X11
(Latin 1-9, Greek, Cyrillic, Armenian and a few more). Since 1.8.0 it uses
Unicode's simple (one-to-one) case mappings, with one exception, `ssharp` to
U+1E9E, and types keys by them
([NEWS, 1.8.0](https://github.com/xkbcommon/libxkbcommon/blob/master/NEWS.md);
1.8 was not run here). This library does the same: Unicode's simple case
mapping through `char`, plus the capital sharp s for `ssharp`. Against 1.6.0:

- wherever 1.6.0 turns a character into another character, this library
  gives the same keysym (1,900 conversions compared, 0 differ);
- this library also cases 1,479 keysyms 1.6.0 leaves alone (letters outside
  the X11 tables);
- 1.6.0 gives 136 results that are not characters at all (it "converts"
  numbers that are no keysym, and returns a bare code point as a keysym for
  `mu`, `ssharp` and `ydiaeresis`); these are not followed.

The visible effect: on a keymap from a compositor running libxkbcommon 1.7 or
older, a handful of keys holding such letters (the Turkish dotless i is the
common one) can react to Caps Lock differently here than in that compositor's
own clients.

## Proof

`tests/differential.rs` loads the system's libxkbcommon (1.6.0 on Ubuntu
24.04) and compares, with both libraries reading the same keymap text:

| What | Comparisons | Mismatches |
| --- | --- | --- |
| 14 layouts (us, de, fr, ru, gb, us+ru, de neo, us intl, four layouts at once, jp, option sets, gr+il+ara, ca multix, dvorak), every key, 753 modifier/layout states: keymap contents, layout, level, keysyms, one keysym, UTF-8, UTF-32, consumed modifiers, repeat, modifier masks, LEDs | 48,295,514 | 0 |
| All 589 layouts and variants of xkeyboard-config 2.41, each with `us` as a second layout, 38 states | 107,091,059 | 0 |
| Every keysym in the named ranges and the Unicode range: name, name back to number, code point; every character to keysym; 57 odd names | 9,370,424 | 0 |
| Case, where the reference turns a character into a character | 1,900 | 0 |

Left out of the first two rows, and counted by the test: 212,900 comparisons
on keys where the case policy above can change the answer, and 132 states
where libxkbcommon 1.6.0 itself computes a layout one past the last (a
negative layout that is a multiple of the layout count).

The same answers for three keymaps are recorded in `tests/fixtures/` and
checked on every platform, so the Windows gate proves more than "it
compiles".
