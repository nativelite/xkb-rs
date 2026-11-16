//! Keymap texts written by hand, in the forms libxkbcommon's own writer
//! never produces: what `xkbcomp` writes, what a person types, and what is
//! merely allowed. `cases.txt` holds them in a compact form; this puts each
//! back together.
//!
//! The file is a list of `=== label` records, each a list of `--- part`
//! blocks. The first record, `BASE`, holds the four sections most cases
//! share. A case gives `sym` (its keys), and may replace a base section
//! (`kc`, `types`, `compat`, `mods`) or add to its end (`kc+`, `types+`,
//! `compat+`), name another wrapper (`head`) or order of sections
//! (`order`, of the letters `k t c s`). A `raw` block is the whole text.

use std::collections::HashMap;

const CASES: &str = include_str!("cases.txt");

/// Every case: its label and its keymap text.
pub fn all() -> Vec<(String, String)> {
    let mut records: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in CASES.split('\n') {
        if let Some(label) = line.strip_prefix("=== ") {
            records.push((label.to_string(), Vec::new()));
        } else if let Some(part) = line.strip_prefix("--- ") {
            let record = records.last_mut().expect("a part before any case");
            record.1.push((part.to_string(), String::new()));
        } else if let Some(part) = records.last_mut().and_then(|r| r.1.last_mut()) {
            if !part.1.is_empty() {
                part.1.push('\n');
            }
            part.1.push_str(line);
        }
    }
    // Each block was written with a line break after it; the last line of a
    // block is therefore empty and not part of it.
    for (_, parts) in &mut records {
        for (_, text) in parts {
            if text.ends_with('\n') {
                text.pop();
            }
        }
    }
    let (label, base) = records.remove(0);
    assert_eq!(label, "BASE");
    let base: HashMap<String, String> = base.into_iter().collect();
    records
        .into_iter()
        .map(|(label, parts)| {
            let parts: HashMap<String, String> = parts.into_iter().collect();
            if let Some(raw) = parts.get("raw") {
                return (label, raw.clone());
            }
            let section = |key: &str| match (parts.get(key), parts.get(&format!("{key}+"))) {
                (Some(whole), _) => whole.clone(),
                (None, Some(more)) => format!("{}{more}", base[key]),
                (None, None) => base[key].clone(),
            };
            let sym = parts.get("sym").map_or("", String::as_str);
            let sections: Vec<String> = parts
                .get("order")
                .map_or("ktcs", String::as_str)
                .chars()
                .map(|c| match c {
                    'k' => format!("xkb_keycodes \"kc\" {{{}}};", section("kc")),
                    't' => format!("xkb_types \"ty\" {{{}}};", section("types")),
                    'c' => format!("xkb_compatibility \"co\" {{{}}};", section("compat")),
                    's' => format!("xkb_symbols \"sy\" {{{}{sym}}};", section("mods")),
                    other => panic!("{label}: section {other:?}"),
                })
                .collect();
            let head = parts.get("head").map_or("xkb_keymap", String::as_str);
            let text = format!("{head} {{\n{}\n}};\n", sections.join("\n"));
            (label, text)
        })
        .collect()
}
