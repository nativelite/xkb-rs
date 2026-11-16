//! XKB keymaps for a Wayland client, on the Rust standard library alone.
//!
//! A compositor hands a client its keymap as text (`wl_keyboard.keymap`),
//! then reports key presses as keycodes and the modifiers as masks. This
//! crate turns those into keysyms and text:
//!
//! - [`Keymap`] reads the keymap text;
//! - [`State`] holds the modifiers and layout the compositor reports and
//!   says what a key means under them;
//! - [`Keysym`] names keysyms and converts them to and from characters.
//! - [`compose`] reads X11 Compose files and follows dead-key and Compose
//!   sequences.
//!
//! ```
//! use xkb::{Keymap, Keysym, State};
//!
//! let keymap: Keymap = r#"
//! xkb_keymap {
//!     xkb_keycodes { <AD01> = 24; <LFSH> = 50; };
//!     xkb_types {
//!         type "ALPHABETIC" {
//!             modifiers = Shift+Lock;
//!             map[Shift] = Level2;
//!             map[Lock] = Level2;
//!         };
//!     };
//!     xkb_compatibility { };
//!     xkb_symbols {
//!         key <AD01> { [ q, Q ] };
//!         key <LFSH> { [ Shift_L ] };
//!         modifier_map Shift { <LFSH> };
//!     };
//! };"#
//! .parse()
//! .unwrap();
//!
//! let mut state = State::new(keymap);
//! assert_eq!(state.key_one_sym(24), Keysym::from_name("q").unwrap());
//! assert_eq!(state.key_utf8(24), "q");
//!
//! // wl_keyboard.modifiers: Shift (bit 0) held.
//! state.update_mask(1, 0, 0, 0, 0, 0);
//! assert_eq!(state.key_utf8(24), "Q");
//! ```
//!
//! Not here (see the README): compiling a keymap from layout names, and the
//! compositor's side of the state (turning key presses into modifier
//! changes).

pub mod compose;
mod keymap;
mod keysym;
mod keysym_table;
mod parse;
mod state;

pub use keymap::{Component, Keycode, Keymap};
pub use keysym::Keysym;
pub use state::State;

/// Why a keymap could not be read, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The line of the text, from 1.
    pub line: u32,
    /// The column of that line in bytes, from 1.
    pub column: u32,
    /// What is wrong there.
    pub message: String,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for Error {}
