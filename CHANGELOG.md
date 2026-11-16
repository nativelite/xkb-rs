# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `Keysym`: names to numbers and back (the keysym list of libxkbcommon
  1.6.0, the `U<code point>` and `0x<number>` forms), keysym to character
  and back, upper and lower case by Unicode's simple case mapping.
- `Keymap`: reads the `xkb_keymap` text format (keycodes, types,
  compatibility, symbols) as compositors send it; key names and aliases,
  layouts, levels, keysyms per level, modifiers (virtual ones resolved to
  real ones through interprets and the modifier map, or as given), key
  repeat, LEDs. Errors carry line and column.
- `State`: modifiers and layout set from the compositor's masks
  (`update_mask`); per key the layout, level, keysyms, one keysym with the
  Caps Lock transformation, UTF-8 and UTF-32 with the Control
  transformation, consumed modifiers; active modifiers and LEDs.
- `compose::Table`: X11 Compose files, from text, a file, or a locale
  (`compose.dir`, `locale.alias`, `$XCOMPOSEFILE`, `$XDG_CONFIG_HOME/XCompose`,
  `~/.XCompose`; `include` with `%L`, `%H`, `%S`). `compose::State`: feed
  keysyms, get nothing / composing / composed / cancelled and the result's
  text and keysym.
- `compose::MAX_FILE_SIZE` (16 MiB): the largest file read as a Compose
  file. Only a regular file is read, whether named directly or by an
  `include`.

### Changed
After an independent review, with libxkbcommon 1.6 as the arbiter:
- A keymap that says the same thing twice is merged as libxkbcommon merges
  it, where the later statement used to replace the earlier one whole: a
  key given again (group by group and level by level), a type, an interpret
  and an LED map (field by field), a keycode, an alias and a modifier map
  entry. `augment`, `override` and `replace` before a statement are
  honoured; they used to be dropped.
- `key.field = value` and `indicator.field = value` set defaults for the
  statements after them; they used to be ignored.
- The modifier map adds each entry's modifier to its key, where the last
  entry for a key used to win; an entry given again changes its modifier,
  and `modifier_map None` takes one away.
- A key with more than four levels and no type gets the keymap's first type,
  not a four-level one. A group given only actions, only a type, or an empty
  list of symbols counts as a group; a level given only an action counts as
  a level.
- In a type: the first `modifiers` counts; a number's bits for modifiers
  the keymap lacks are kept (such an entry never matches, where it used to
  become the entry for no modifier); `level_name` gives the type no level; a
  level or a layout may be an expression (`1+1`); `!` is not a mask
  operator, `~` is.
- LEDs: `whichModState = None` with modifiers named means the effective
  ones; a name stays with the first number it was given; `any` as a state
  component is the effective one.
- `{ NoSymbol }` is an empty level; `groupsWrap = no` clamps; a bare
  `repeat` in an interpret is yes; an octal escape of 128 and up is a byte;
  a number is not a yes or no.
- A field of a key, a type or an interpret that makes no sense is passed
  over instead of failing the keymap, as libxkbcommon passes over it
  wherever another field follows.
- Read where they were refused: a section given twice (the first counts),
  the `xkb_semantics` and `xkb_layout` wrappers, an interpret naming an
  undeclared virtual modifier before another field.
- `Keymap::min_keycode` and `max_keycode` count a keycode whose name a
  later statement took away, and are 8 and 255 for a keymap without keys
  (they were 0 and 0). `Keymap::layout_name` returns an empty name as
  `Some("")`.
- `State::key_level` returns `None` for a layout the key does not have; it
  used to bring the layout into the key's range.
- Compose: the ISO_Level5 keysyms (`0xfe11` to `0xfe13`) and `0xfe10` are
  not passed over as modifier keys; they end a sequence, as in libxkbcommon.

### Fixed
- A chain of some tens of thousands of `+` in one expression overflowed the
  stack and aborted the process (libxkbcommon 1.6 does the same). A chain is
  now one flat node however long.
- Reading a keymap could take time in proportion to the square of its size:
  a modifier map naming a keysym (each entry searched every level of every
  key: 178 s for a 63 KB text, now under 0.1 s), keycodes, aliases, types,
  a type's entries, interprets given again, and the interpret for each
  level.
- An `include` in a Compose file naming a device or a pipe was read without
  end, or waited on.
