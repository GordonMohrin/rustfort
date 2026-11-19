//! [prefs15] "Preferences" block of the unit sheet's personality tab: what a unit likes and
//! hates (exported from DF, or estimated by Rustfort when the export had none), grouped like
//! DF's sheet. Own wording (no DF preference strings). Data: `Sim::unit_preferences`.

use super::col;
use super::psyche::{pronoun, Row};
use crate::game::Game;
use macroquad::prelude::*;
use rf_core::Unit;

const GROUPS: [(&str, &str); 7] = [
    ("material", "Materials"),
    ("item", "Items"),
    ("creature", "Creatures"),
    ("plant", "Plants"),
    ("food", "Food and drink"),
    ("other", "Colors, shapes and arts"),
    ("dislike", "Dislikes"),
];

/// Rows for the personality tab (appended after the beliefs).
pub fn preference_rows(g: &Game, u: &Unit) -> Vec<Row> {
    let lines = g.sim.unit_preferences(u.id);
    let mut rows = vec![Row::Gap, Row::Head("Preferences".into())];
    if lines.is_empty() {
        rows.push(Row::Text("No preferences recorded.".into(), col::TEXT_DIM));
        return rows;
    }
    let derived = lines.iter().any(|l| l.derived);
    for (key, title) in GROUPS {
        let mut first = true;
        for l in lines.iter().filter(|l| l.group == key) {
            if first {
                rows.push(Row::Text(format!("{title}:"), col::TEXT_DIM));
                first = false;
            }
            let c = if key == "dislike" { Color::from_rgba(220, 160, 140, 255) } else { col::TEXT };
            rows.push(Row::Text(format!("  {} {}.", pronoun(u), l.text), c));
        }
    }
    if derived {
        rows.push(Row::Text("(Estimated by Rustfort: the export had no preferences.)".into(), col::TEXT_DIM));
    }
    rows
}
