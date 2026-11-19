//! [prefs15] Strange moods and preferences (wiki "Strange mood", "Artifacts created" and base
//! material demands; spec docs/mechanics/prefs15.md section 6).
//!
//! - Artifact type: the wiki list of the mood skill; a dwarf who likes an item type that is on
//!   the list always makes that type (several matches: one at random).
//! - Base material: metalworkers want a liked metal the fort has smelted; bone carvers who like
//!   a shell use shells.
//! - Decorations never repeat the base material type; miners, engravers, masons and stone
//!   crafters never ask for blocks.
//! - Value: `floor(base x mat x 20) + 300` plus every decoration at artifact quality.

use crate::rng::Rng;
use crate::Sim;
use rf_core::NO_MAT;

/// (item kind, subtype, noun); same shape as `moods::ArtKind`.
pub(crate) type Art = (&'static str, &'static str, &'static str);
/// An artifact choice with its weight in tenths († entries weigh 0.1, wiki).
type Entry = (Art, u32);

const W: u32 = 10;
const LOW: u32 = 1;

const ARMORSMITH: &[Entry] = &[
    (("ARMOR", "ITEM_ARMOR_MAIL_SHIRT", "mail shirt"), W),
    (("ARMOR", "ITEM_ARMOR_BREASTPLATE", "breastplate"), W),
    (("PANTS", "ITEM_PANTS_LEGGINGS", "leggings"), W),
    (("PANTS", "ITEM_PANTS_GREAVES", "greaves"), W),
    (("GLOVES", "ITEM_GLOVES_GAUNTLETS", "gauntlet"), W),
    (("SHOES", "ITEM_SHOES_BOOTS_LOW", "low boot"), W),
    (("SHOES", "ITEM_SHOES_BOOTS", "high boot"), W),
    (("HELM", "ITEM_HELM_CAP", "cap"), W),
    (("HELM", "ITEM_HELM_HELM", "helm"), W),
    (("SHIELD", "ITEM_SHIELD_SHIELD", "shield"), W),
    (("SHIELD", "ITEM_SHIELD_BUCKLER", "buckler"), W),
];
const BONE: &[Entry] = &[
    (("PANTS", "ITEM_PANTS_LEGGINGS", "leggings"), W),
    (("PANTS", "ITEM_PANTS_GREAVES", "greaves"), W),
    (("GLOVES", "ITEM_GLOVES_GAUNTLETS", "gauntlet"), W),
    (("HELM", "ITEM_HELM_HELM", "helm"), W),
    (("SHIELD", "ITEM_SHIELD_SHIELD", "shield"), W),
    (("SHIELD", "ITEM_SHIELD_BUCKLER", "buckler"), W),
    (("INSTRUMENT", "", "instrument"), W),
    (("TOY", "", "toy"), W),
    (("DOOR", "", "door"), W),
    (("BED", "", "bed"), W),
    (("CHAIR", "", "throne"), W),
    (("TABLE", "", "table"), W),
    (("STATUE", "", "statue"), W),
    (("BOX", "", "chest"), W),
    (("BIN", "", "bin"), W),
    (("ARMORSTAND", "", "armor stand"), W),
    (("WEAPONRACK", "", "weapon rack"), W),
    (("CABINET", "", "cabinet"), W),
    (("COFFIN", "", "coffin"), W),
    (("FLOODGATE", "", "floodgate"), W),
    (("HATCH_COVER", "", "hatch cover"), W),
    (("GRATE", "", "grate"), W),
    (("CHAIN", "", "chain"), W),
    (("CAGE", "", "cage"), W),
    (("ANIMALTRAP", "", "animal trap"), W),
    (("FIGURINE", "", "figurine"), W),
    (("AMULET", "", "amulet"), W),
    (("SCEPTER", "", "scepter"), W),
    (("CROWN", "", "crown"), W),
    (("RING", "", "ring"), W),
    (("EARRING", "", "earring"), W),
    (("BRACELET", "", "bracelet"), W),
    (("WEAPON", "*", "weapon"), W),
    (("TRAPCOMP", "*", "trap component"), LOW),
];
const SHELL: &[Entry] = &[
    (("PANTS", "ITEM_PANTS_LEGGINGS", "leggings"), W),
    (("GLOVES", "ITEM_GLOVES_GAUNTLETS", "gauntlet"), W),
    (("HELM", "ITEM_HELM_HELM", "helm"), W),
    (("FIGURINE", "", "figurine"), W),
    (("AMULET", "", "amulet"), W),
    (("CROWN", "", "crown"), W),
    (("RING", "", "ring"), W),
    (("EARRING", "", "earring"), W),
    (("BRACELET", "", "bracelet"), W),
    (("CHAIN", "", "chain"), W),
    (("CAGE", "", "cage"), W),
    (("ANIMALTRAP", "", "animal trap"), W),
    (("INSTRUMENT", "", "instrument"), W),
    (("TOY", "", "toy"), W),
];
const BOWYER: &[Entry] = &[(("WEAPON", "ITEM_WEAPON_CROSSBOW", "crossbow"), W)];
const CARPENTER: &[Entry] = &[
    (("DOOR", "", "door"), W),
    (("BED", "", "bed"), W),
    (("CHAIR", "", "chair"), W),
    (("TABLE", "", "table"), W),
    (("STATUE", "", "statue"), W),
    (("BOX", "", "chest"), W),
    (("BIN", "", "bin"), W),
    (("ARMORSTAND", "", "armor stand"), W),
    (("WEAPONRACK", "", "weapon rack"), W),
    (("CABINET", "", "cabinet"), W),
    (("COFFIN", "", "coffin"), W),
    (("FLOODGATE", "", "floodgate"), W),
    (("HATCH_COVER", "", "hatch cover"), W),
    (("GRATE", "", "grate"), W),
    (("CAGE", "", "cage"), W),
    (("BARREL", "", "barrel"), W),
    (("BUCKET", "", "bucket"), W),
    (("ANIMALTRAP", "", "animal trap"), W),
    (("SPLINT", "", "splint"), W),
    (("CRUTCH", "", "crutch"), W),
];
const CLOTHIER: &[Entry] = &[
    (("ARMOR", "ITEM_ARMOR_DRESS", "dress"), W),
    (("ARMOR", "ITEM_ARMOR_SHIRT", "shirt"), W),
    (("ARMOR", "ITEM_ARMOR_TUNIC", "tunic"), W),
    (("ARMOR", "ITEM_ARMOR_TOGA", "toga"), W),
    (("ARMOR", "ITEM_ARMOR_VEST", "vest"), W),
    (("ARMOR", "ITEM_ARMOR_ROBE", "robe"), W),
    (("ARMOR", "ITEM_ARMOR_COAT", "coat"), W),
    (("ARMOR", "ITEM_ARMOR_CLOAK", "cloak"), W),
    (("PANTS", "ITEM_PANTS_TROUSERS", "trousers"), W),
    (("PANTS", "ITEM_PANTS_LOINCLOTH", "loincloth"), W),
    (("PANTS", "ITEM_PANTS_BRAIES", "braies"), W),
    (("GLOVES", "ITEM_GLOVES_GLOVES", "glove"), W),
    (("GLOVES", "ITEM_GLOVES_MITTENS", "mitten"), W),
    (("SHOES", "ITEM_SHOES_SOCKS", "sock"), W),
    (("SHOES", "ITEM_SHOES_SHOES", "shoe"), W),
    (("HELM", "ITEM_HELM_CAP", "cap"), W),
    (("HELM", "ITEM_HELM_HOOD", "hood"), W),
    (("BAG", "", "bag"), W),
    (("CHAIN", "", "rope"), W),
];
const STONE_CRAFTS: &[Entry] = &[
    (("FIGURINE", "", "figurine"), W),
    (("AMULET", "", "amulet"), W),
    (("SCEPTER", "", "scepter"), W),
    (("CROWN", "", "crown"), W),
    (("RING", "", "ring"), W),
    (("EARRING", "", "earring"), W),
    (("BRACELET", "", "bracelet"), W),
    (("GOBLET", "", "goblet"), W),
    (("INSTRUMENT", "", "instrument"), W),
    (("TOY", "", "toy"), W),
];
const LEATHER: &[Entry] = &[
    (("ARMOR", "ITEM_ARMOR_DRESS", "dress"), W),
    (("ARMOR", "ITEM_ARMOR_SHIRT", "shirt"), W),
    (("ARMOR", "ITEM_ARMOR_VEST", "vest"), W),
    (("ARMOR", "ITEM_ARMOR_ROBE", "robe"), W),
    (("ARMOR", "ITEM_ARMOR_COAT", "coat"), W),
    (("ARMOR", "ITEM_ARMOR_CLOAK", "cloak"), W),
    (("ARMOR", "ITEM_ARMOR_LEATHER", "leather armor"), W),
    (("PANTS", "ITEM_PANTS_TROUSERS", "trousers"), W),
    (("PANTS", "ITEM_PANTS_LEGGINGS", "leggings"), W),
    (("GLOVES", "ITEM_GLOVES_GLOVES", "glove"), W),
    (("SHOES", "ITEM_SHOES_SHOES", "shoe"), W),
    (("SHOES", "ITEM_SHOES_BOOTS_LOW", "low boot"), W),
    (("SHOES", "ITEM_SHOES_BOOTS", "high boot"), W),
    (("HELM", "ITEM_HELM_CAP", "cap"), W),
    (("HELM", "ITEM_HELM_HOOD", "hood"), W),
    (("SHIELD", "ITEM_SHIELD_SHIELD", "shield"), W),
    (("SHIELD", "ITEM_SHIELD_BUCKLER", "buckler"), W),
    (("BAG", "", "bag"), W),
    (("BACKPACK", "", "backpack"), W),
    (("QUIVER", "", "quiver"), W),
    (("INSTRUMENT", "", "instrument"), W),
];
const GEM_GLASS: &[Entry] = &[
    (("DOOR", "", "door"), W),
    (("BED", "", "bed"), W),
    (("CHAIR", "", "throne"), W),
    (("TABLE", "", "table"), W),
    (("STATUE", "", "statue"), W),
    (("BOX", "", "coffer"), W),
    (("ARMORSTAND", "", "armor stand"), W),
    (("WEAPONRACK", "", "weapon rack"), W),
    (("CABINET", "", "cabinet"), W),
    (("COFFIN", "", "coffin"), W),
    (("FLOODGATE", "", "floodgate"), W),
    (("HATCH_COVER", "", "hatch cover"), W),
    (("GRATE", "", "grate"), W),
    (("FIGURINE", "", "figurine"), W),
    (("AMULET", "", "amulet"), W),
    (("SCEPTER", "", "scepter"), W),
    (("CROWN", "", "crown"), W),
    (("RING", "", "ring"), W),
    (("EARRING", "", "earring"), W),
    (("BRACELET", "", "bracelet"), W),
    (("CHAIN", "", "chain"), W),
    (("FLASK", "", "flask"), W),
    (("GOBLET", "", "goblet"), W),
    (("CAGE", "", "cage"), W),
    (("BARREL", "", "barrel"), W),
    (("BUCKET", "", "bucket"), W),
    (("ANIMALTRAP", "", "animal trap"), W),
    (("WINDOW", "", "window"), W),
    (("INSTRUMENT", "", "instrument"), W),
    (("TOY", "", "toy"), W),
];
const MACABRE: &[Entry] = &[(("AMULET", "", "amulet"), W), (("BRACELET", "", "bracelet"), W), (("EARRING", "", "earring"), W)];
const MASON: &[Entry] = &[
    (("DOOR", "", "door"), W),
    (("BED", "", "bed"), W),
    (("CHAIR", "", "throne"), W),
    (("TABLE", "", "table"), W),
    (("STATUE", "", "statue"), W),
    (("QUERN", "", "quern"), W),
    (("MILLSTONE", "", "millstone"), W),
    (("BOX", "", "coffer"), W),
    (("ARMORSTAND", "", "armor stand"), W),
    (("WEAPONRACK", "", "weapon rack"), W),
    (("CABINET", "", "cabinet"), W),
    (("COFFIN", "", "sarcophagus"), W),
    (("FLOODGATE", "", "floodgate"), W),
    (("HATCH_COVER", "", "hatch cover"), W),
    (("GRATE", "", "grate"), W),
];
const MECHANIC: &[Entry] = &[(("TRAPPARTS", "", "mechanism"), W)];
const METAL_CRAFTS: &[Entry] = &[
    (("FIGURINE", "", "figurine"), W),
    (("AMULET", "", "amulet"), W),
    (("SCEPTER", "", "scepter"), W),
    (("CROWN", "", "crown"), W),
    (("RING", "", "ring"), W),
    (("EARRING", "", "earring"), W),
    (("BRACELET", "", "bracelet"), W),
    (("CHAIN", "", "chain"), W),
    (("FLASK", "", "flask"), W),
    (("GOBLET", "", "goblet"), W),
    (("INSTRUMENT", "", "instrument"), W),
    (("TOY", "", "toy"), W),
];
const METALSMITH: &[Entry] = &[
    (("DOOR", "", "door"), W),
    (("BED", "", "bed"), W),
    (("CHAIR", "", "throne"), W),
    (("TABLE", "", "table"), W),
    (("STATUE", "", "statue"), W),
    (("BOX", "", "chest"), W),
    (("ARMORSTAND", "", "armor stand"), W),
    (("WEAPONRACK", "", "weapon rack"), W),
    (("CABINET", "", "cabinet"), W),
    (("ANVIL", "", "anvil"), W),
    (("COFFIN", "", "coffin"), W),
    (("FLOODGATE", "", "floodgate"), W),
    (("HATCH_COVER", "", "hatch cover"), W),
    (("GRATE", "", "grate"), W),
    (("CAGE", "", "cage"), W),
    (("BARREL", "", "barrel"), W),
    (("BUCKET", "", "bucket"), W),
    (("ANIMALTRAP", "", "animal trap"), W),
    (("PIPE_SECTION", "", "pipe section"), LOW),
    (("SPLINT", "", "splint"), W),
    (("CRUTCH", "", "crutch"), W),
];
const WEAPONSMITH: &[Entry] = &[
    (("WEAPON", "ITEM_WEAPON_AXE_BATTLE", "battle axe"), W),
    (("WEAPON", "ITEM_WEAPON_HAMMER_WAR", "war hammer"), W),
    (("WEAPON", "ITEM_WEAPON_SWORD_SHORT", "short sword"), W),
    (("WEAPON", "ITEM_WEAPON_SPEAR", "spear"), W),
    (("WEAPON", "ITEM_WEAPON_MACE", "mace"), W),
    (("TRAPCOMP", "*", "trap component"), LOW),
];

/// "any" entries (`*`): one random subtype.
const ANY_WEAPON: &[Art] = &[
    ("WEAPON", "ITEM_WEAPON_AXE_BATTLE", "battle axe"),
    ("WEAPON", "ITEM_WEAPON_HAMMER_WAR", "war hammer"),
    ("WEAPON", "ITEM_WEAPON_SWORD_SHORT", "short sword"),
    ("WEAPON", "ITEM_WEAPON_SPEAR", "spear"),
    ("WEAPON", "ITEM_WEAPON_MACE", "mace"),
];
const ANY_TRAPCOMP: &[Art] = &[
    ("TRAPCOMP", "ITEM_TRAPCOMP_GIANTAXEBLADE", "giant axe blade"),
    ("TRAPCOMP", "ITEM_TRAPCOMP_ENORMOUSCORKSCREW", "enormous corkscrew"),
    ("TRAPCOMP", "ITEM_TRAPCOMP_SPIKEDBALL", "spiked ball"),
    ("TRAPCOMP", "ITEM_TRAPCOMP_LARGESERRATEDDISC", "large serrated disc"),
    ("TRAPCOMP", "ITEM_TRAPCOMP_MENACINGSPIKE", "menacing spike"),
];

/// Artifact list of a mood skill (DF `job_skill` names). `shell`: bone carver working shells.
pub(crate) fn artifact_list(skill: &str, shell: bool, macabre: bool) -> &'static [Entry] {
    if macabre {
        return MACABRE;
    }
    match skill {
        "FORGE_ARMOR" => ARMORSMITH,
        "BONECARVE" => {
            if shell {
                SHELL
            } else {
                BONE
            }
        }
        "BOWYER" => BOWYER,
        "CARPENTRY" => CARPENTER,
        "CLOTHESMAKING" | "WEAVING" => CLOTHIER,
        "ENGRAVE_STONE" | "STONECRAFT" | "CUT_STONE" | "CARVE_STONE" | "WOODCRAFT" => STONE_CRAFTS,
        "LEATHERWORK" | "TANNER" => LEATHER,
        "CUTGEM" | "ENCRUSTGEM" | "GLASSMAKER" => GEM_GLASS,
        "MASONRY" | "MINING" => MASON,
        "MECHANICS" => MECHANIC,
        "METALCRAFT" => METAL_CRAFTS,
        "FORGE_FURNITURE" => METALSMITH,
        "FORGE_WEAPON" => WEAPONSMITH,
        _ => STONE_CRAFTS,
    }
}

/// Resolve an "any" entry to a concrete subtype.
fn concrete(a: Art, r: &mut Rng) -> Art {
    match (a.0, a.1) {
        ("WEAPON", "*") => ANY_WEAPON[r.below(ANY_WEAPON.len() as u32) as usize],
        ("TRAPCOMP", "*") => ANY_TRAPCOMP[r.below(ANY_TRAPCOMP.len() as u32) as usize],
        _ => a,
    }
}

/// Weighted random pick from a list.
pub(crate) fn random_art(list: &[Entry], r: &mut Rng) -> Art {
    let total: u32 = list.iter().map(|e| e.1).sum();
    let mut k = r.below(total.max(1));
    for &(a, w) in list {
        if k < w {
            return concrete(a, r);
        }
        k -= w;
    }
    concrete(list[0].0, r)
}

pub(crate) fn no_mat() -> u16 {
    NO_MAT
}

/// Skills that never ask for rock blocks as decorations (wiki).
pub(crate) fn no_block_decor(skill: &str) -> bool {
    matches!(skill, "MINING" | "ENGRAVE_STONE" | "MASONRY" | "STONECRAFT")
}

/// Metalworking mood skills (base material: bars).
fn metal_skill(skill: &str) -> bool {
    matches!(skill, "FORGE_ARMOR" | "FORGE_WEAPON" | "FORGE_FURNITURE" | "METALCRAFT")
}

impl Sim {
    /// Artifact for a mood: a liked item type on the skill's list (random among matches), else
    /// a weighted random entry. Returns (artifact, forced by a preference).
    pub(crate) fn prefs_mood_art(&mut self, s: usize, list: &[Entry], r: &mut Rng) -> (Art, bool) {
        let mut liked: Vec<Art> = Vec::new();
        for &(a, _) in list {
            let hit = if a.1 == "*" {
                // "any weapon": a preference for any of its subtypes.
                let subs: &[Art] = if a.0 == "WEAPON" { ANY_WEAPON } else { ANY_TRAPCOMP };
                subs.iter().filter(|x| self.prefs_likes_item(s, x.0, x.1)).copied().next()
            } else if self.prefs_likes_item(s, a.0, a.1) {
                Some(a)
            } else {
                None
            };
            if let Some(h) = hit {
                if !liked.contains(&h) {
                    liked.push(h);
                }
            }
        }
        if !liked.is_empty() {
            self.prefs.rep.mood_forced += 1;
            return (liked[r.below(liked.len() as u32) as usize], true);
        }
        (random_art(list, r), false)
    }

    /// Does the moody dwarf in slot `s` like a shell material? (bone carvers then use shells)
    pub(crate) fn prefs_likes_shell(&self, s: usize) -> bool {
        let Some(u) = self.ai.get(s).map(|a| a.uidx) else { return false };
        self.world.units[u].preferences.iter().any(|p| p.active && p.kind == "LikeMaterial" && p.mat_token.ends_with(":SHELL"))
    }

    /// Wanted base metal of a metalworking mood: the first liked metal the fort has smelted.
    pub(crate) fn prefs_mood_metal(&self, s: usize, skill: &str) -> u16 {
        if self.prefs.off || !metal_skill(skill) {
            return NO_MAT;
        }
        let Some(u) = self.ai.get(s).map(|a| a.uidx) else { return NO_MAT };
        for p in &self.world.units[u].preferences {
            if p.active && p.kind == "LikeMaterial" && p.mat != NO_MAT && self.prefs.smelted.contains(&p.mat) {
                if self.world.material(p.mat).map(|m| m.is_metal).unwrap_or(false) {
                    return p.mat;
                }
            }
        }
        NO_MAT
    }
}
