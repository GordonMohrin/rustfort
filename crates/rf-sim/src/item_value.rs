//! [prefs15] DF v50 item value (wiki "Item value", "Item quality", "Decoration"; checked against
//! the reference export, see docs/mechanics/prefs15.md section 4).
//!
//! `value = floor(base x material value x quality multiplier) + quality bonus` per unit, times the
//! stack, plus each decoration at its own quality. Engravings and room tiles keep the old
//! multipliers (`trade::quality_mult`, wiki "Engraving", "Zone").

/// Quality multipliers as fractions: 1, 1.1, 1.2, 4/3, 1.5, 2, 20 (ordinary .. artifact).
pub(crate) const Q_NUM: [i64; 7] = [1, 11, 6, 4, 3, 2, 20];
pub(crate) const Q_DEN: [i64; 7] = [1, 10, 5, 3, 2, 1, 1];
/// Flat bonus added after the multiplier (wiki "Item quality/Table").
pub(crate) const Q_BONUS: [i64; 7] = [0, 3, 6, 10, 15, 30, 300];

/// Base value of an item kind (wiki "Item value", base values checked against the export).
pub fn base_value_v50(kind: &str, subtype: &str) -> i64 {
    match kind {
        "CORPSE" | "CORPSEPIECE" | "REMAINS" | "VERMIN" | "PET" => 0,
        "GLOB" | "SEEDS" | "DRINK" | "POWDER_MISC" | "LIQUID_MISC" | "AMMO" | "FOOD" | "BRANCH" => 1,
        "MEAT" | "FISH" | "FISH_RAW" | "EGG" | "PLANT" | "PLANT_GROWTH" => 2,
        "WOOD" | "BOULDER" => 3,
        "BAR" | "BLOCKS" | "SKIN_TANNED" | "SHEET" | "SHOES" | "GLOVES" => 5,
        "THREAD" | "ROUGH" => 6,
        "CLOTH" => 7,
        "SMALLGEM" | "BOOK" | "SIEGEAMMO" | "TRACTION_BENCH" => 20,
        // The export's single millstone fits 25 (ESTIMATE).
        "WINDOW" | "STATUE" | "MILLSTONE" => 25,
        "TRAPPARTS" | "CATAPULTPARTS" | "BALLISTAPARTS" => 30,
        "TOOL" => match subtype {
            "ITEM_TOOL_CAULDRON" | "ITEM_TOOL_MINECART" | "ITEM_TOOL_WHEELBARROW" | "ITEM_TOOL_STEPLADDER" => 50,
            _ => 10,
        },
        _ => 10,
    }
}

/// Value of one unit of an item at quality `q` (0..=6).
#[inline]
pub fn quality_value(base: i64, mat_value: i64, q: u8) -> i64 {
    let q = (q as usize).min(6);
    base * mat_value.max(1) * Q_NUM[q] / Q_DEN[q] + Q_BONUS[q]
}

/// Value of a decoration (wiki "Decoration"): base 20 for cut gems, 10 otherwise.
#[inline]
pub fn decoration_value(gem: bool, mat_value: i64, q: u8) -> i64 {
    quality_value(if gem { 20 } else { 10 }, mat_value, q)
}

/// Value of a whole (undecorated) item: per-unit value times the stack. Prepared meals add the
/// ingredients' base value 2 per unit (ESTIMATE: Rustfort tracks no ingredient list).
pub fn item_value_v50(kind: &str, subtype: &str, mat_value: i64, q: u8, stack: i32) -> i64 {
    let per = if kind == "FOOD" { quality_value(1, 1, q) + 2 } else { quality_value(base_value_v50(kind, subtype), mat_value, q) };
    per * stack.max(1) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_matches_df_export_samples() {
        // superior wood chair (wood value 1)
        assert_eq!(item_value_v50("CHAIR", "", 1, 3, 1), 23);
        // exceptional chalk (value 2) mug
        assert_eq!(quality_value(10, 2, 4), 45);
        // masterwork wood chair
        assert_eq!(item_value_v50("CHAIR", "", 1, 5, 1), 50);
        // artifact wood amulet, no decorations / one wood decoration
        assert_eq!(item_value_v50("AMULET", "", 1, 6, 1), 500);
        assert_eq!(item_value_v50("AMULET", "", 1, 6, 1) + decoration_value(false, 1, 6), 1_000);
        // ordinary gold (30) statue
        assert_eq!(item_value_v50("STATUE", "", 30, 0, 1), 750);
        // well-crafted / finely-crafted 10-value items: 14, 18 (export admire severities)
        assert_eq!(quality_value(10, 1, 1), 14);
        assert_eq!(quality_value(10, 1, 2), 18);
        assert_eq!(quality_value(10, 1, 4), 30);
        // boulders, logs, cut gems
        assert_eq!(item_value_v50("BOULDER", "", 1, 0, 1), 3);
        assert_eq!(item_value_v50("SMALLGEM", "", 1, 0, 1), 20);
        assert_eq!(item_value_v50("TRAPPARTS", "", 1, 0, 1), 30);
    }
}
