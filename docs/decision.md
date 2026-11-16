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
(`update_mask`); keysyms, text, consumed modifiers, repeat, LEDs; Compose
(without it dead keys type nothing, and they are how most European layouts
type accents).

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
| 14 layouts (us, de, fr, ru, gb, us+ru, de neo, us intl, four layouts at once, jp, option sets, gr+il+ara, ca multix, dvorak), every key, 753 modifier/layout states: keymap contents, layout, level (also of layouts the key does not have), keysyms, one keysym, UTF-8, UTF-32, consumed modifiers, repeat, modifier masks, LEDs | 76,477,394 | 0 |
| All 589 layouts and variants of xkeyboard-config 2.41, each with `us` as a second layout, 38 states | 169,805,140 | 0 |
| 326 keymap texts written by hand (`tests/handwritten/cases.txt`), in forms the reference's own writer never produces, 436 states each: 253 read by both libraries and compared in full, 25 refused by both, 47 refused by the reference and read here (the README lists how this library is the more lenient), 1 with an `include` the reference resolves from the system's files; none read by the reference and refused here | 130,057,328 | 0 |
| Every keysym in the named ranges and the Unicode range: name, name back to number, code point; every character to keysym; 57 odd names | 9,370,424 | 0 |
| Case, where the reference turns a character into a character | 1,900 | 0 |
| Compose: the tables of 16 locales (the 19 asked for less the 3 neither library finds), entry by entry, then two states fed every sequence and a 20,000-round random walk per table, comparing status, text and keysym after each keysym | 3,309,866 | 0 |
| Compose: a file with every construct of the syntax (escapes, modifiers, duplicate, longer and shorter sequences, ten and eleven keysyms, malformed lines) | 117,182 | 0 |

Left out of the first three rows, and counted by the test: 100,597
comparisons where the case policy above can change the answer (a key whose
type is worked out from the case of its first keysyms, where the two
libraries case one of them differently; and, with Lock on, a keysym the two
upper-case differently), and 2,172 states where libxkbcommon 1.6.0 itself
computes a layout one past the last (a negative layout that is a multiple
of the layout count).

The hand-written texts were added after an independent review found a dozen
places where valid text outside what the reference's writer emits was read
differently (defaults set by `key.field`, a key given twice, a modifier map
naming one key twice, groups that exist only through actions, and others).
Each is now read as the reference reads it, and each has a case in that
file.

Compose follows the reference's rules where the format leaves room: a
sequence given again replaces only what the new line gives (a string alone
keeps the earlier keysym), a longer sequence displaces the shorter one it
extends, whatever follows the result keysym on a line is not looked at, and
an explicitly empty string is the text. It differs in one place: a Compose
file that is not UTF-8 (the `en_US` and `german` locales resolve to a
Latin-1 one) is rejected whole by libxkbcommon; here its readable lines are
kept.

The same answers for three keymaps are recorded in `tests/fixtures/` and
checked on every platform, so the Windows gate proves more than "it
compiles".
