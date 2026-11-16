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
