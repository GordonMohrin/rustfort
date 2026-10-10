//! Creature graphics: `[CREATURE_GRAPHICS]` simple sprites and layered sets (dwarves etc.).
//!
//! Layered sets are evaluated like DF does: layers outside groups are drawn when their
//! conditions hold; inside a `[LAYER_GROUP]` the first matching layer wins. Unit details the
//! export does not have yet (skin/hair color, hair length, clothing) are picked
//! deterministically from the unit id, with a default outfit (shirt, trousers, shoes, and the
//! profession's tool), so dwarves look like DF dwarves.

use crate::{Gfx, Sprite};
use rf_core::{Unit, World};
use rf_raws::{Raws, Token};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub(crate) struct ItemReq {
    pub part: String,
    pub kind: String,
    pub subtypes: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TissueCond {
    pub tissue: String,
    pub colors: Vec<String>,
    pub min_len: Option<i32>,
    pub max_len: Option<i32>,
    pub shapings: Vec<String>,
    pub not_shaped: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum Cond {
    Caste(Vec<String>),
    ProfCat(Vec<String>),
    Never,
    Worn(ItemReq),
    Tissue(TissueCond),
    Random { idx: u32, count: u32, salt: u32 },
    MatFlag(bool),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Pal {
    None,
    /// Index into the layer set's palettes + row.
    Ls(u8, u16),
    StdFromItem,
}

#[derive(Clone, Debug)]
pub(crate) struct Layer {
    pub sprite: Sprite,
    pub conds: Vec<Cond>,
    pub shutoff: Vec<ItemReq>,
    pub pal: Pal,
}

#[derive(Clone, Debug)]
pub(crate) enum Entry {
    Single(Layer),
    Group(Vec<Layer>),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LayerSet {
    pub state: String,
    /// (name, palette index in Gfx::palettes, default row)
    pub palettes: Vec<(String, u16, u16)>,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CreatureGfx {
    pub default: Option<Sprite>,
    pub child: Option<Sprite>,
    /// `[CORPSE:...]` / `[REMAINS:...]` sprites (simple creatures).
    pub corpse: Option<Sprite>,
    pub remains: Option<Sprite>,
    pub sets: Vec<LayerSet>,
    /// [gfx6] `[CREATURE_CASTE_GRAPHICS:<race>:<caste>]` sprites (chickens, ducks, sheep, deer,
    /// ...): (caste, default, child, corpse). Picked before the race-wide ones.
    pub castes: Vec<(String, Option<Sprite>, Option<Sprite>, Option<Sprite>)>,
}

/// The evaluated, palette-resolved sprites of one unit (cached by `Gfx::prepare`).
#[derive(Clone, Debug, Default)]
pub struct UnitLook {
    pub sprites: Vec<Sprite>,
}

/// Parse a sprite reference `PAGE:x:y` or `PAGE:LARGE_IMAGE:x1:y1:x2:y2` starting at arg `i`.
fn sprite_ref(g: &Gfx, t: &Token, i: usize) -> Option<Sprite> {
    let page = g.page_id(t.arg(i))?;
    if t.arg(i + 1) == "LARGE_IMAGE" {
        let (x1, y1, x2, y2) = (t.arg_i32(i + 2)?, t.arg_i32(i + 3)?, t.arg_i32(i + 4)?, t.arg_i32(i + 5)?);
        let tw = (x2 - x1 + 1).max(1) as u16;
        let th = (y2 - y1 + 1).max(1) as u16;
        // Multi-tile unit sprites are bottom-aligned (dy) and centered horizontally on the
        // tile; the half-tile centering is left to the renderer, see `Sprite::unit_center_px`.
        Some(Sprite { page, tx: x1 as u16, ty: y1 as u16, tw, th, tint: [255; 4], dx: 0, dy: -(th as i16 - 1) })
    } else {
        Some(Sprite::simple(page, t.arg_i32(i + 1)? as u16, t.arg_i32(i + 2)? as u16))
    }
}

fn item_req(t: &Token) -> ItemReq {
    // BY_CATEGORY:part:kind:subtypes... | BY_TOKEN:part:kind:subtypes...
    ItemReq { part: t.arg(1).to_string(), kind: t.arg(2).to_string(), subtypes: t.args.iter().skip(3).cloned().collect() }
}

pub(crate) fn parse_all(g: &mut Gfx, raws: &Raws) -> HashMap<String, CreatureGfx> {
    let mut out: HashMap<String, CreatureGfx> = HashMap::new();
    for o in raws.of_kind("CREATURE_GRAPHICS") {
        let dir = raws.file_dir(o);
        // Several CREATURE_GRAPHICS objects per creature add up (sprites, portraits, ...).
        let mut cg = out.remove(&o.id).unwrap_or_default();
        let mut cur_set: Option<LayerSet> = None;
        let mut skip_set = false;
        let mut in_group = false;
        let mut layer: Option<Layer> = None;
        // Finishes the current layer into the set.
        fn flush(set: &mut Option<LayerSet>, layer: &mut Option<Layer>, in_group: bool) {
            if let (Some(s), Some(l)) = (set.as_mut(), layer.take()) {
                if in_group {
                    if let Some(Entry::Group(v)) = s.entries.last_mut() {
                        v.push(l);
                        return;
                    }
                }
                s.entries.push(Entry::Single(l));
            }
        }
        for t in &o.tokens[1..] {
            if skip_set && t.name != "LAYER_SET" {
                continue;
            }
            match t.name.as_str() {
                "DEFAULT" if cur_set.is_none() => cg.default = sprite_ref(g, t, 0),
                "CORPSE" if cur_set.is_none() && cg.corpse.is_none() => cg.corpse = sprite_ref(g, t, 0),
                "REMAINS" if cur_set.is_none() && cg.remains.is_none() => cg.remains = sprite_ref(g, t, 0),
                "CHILD" if cur_set.is_none() && t.args.last().map(|s| s == "DEFAULT").unwrap_or(false) => cg.child = sprite_ref(g, t, 0),
                "LAYER_SET" => {
                    flush(&mut cur_set, &mut layer, in_group);
                    if let Some(s) = cur_set.take() {
                        cg.sets.push(s);
                    }
                    in_group = false;
                    // Portrait sets are for the unit sheet, not the map.
                    skip_set = t.args.iter().any(|a| a == "PORTRAIT");
                    if !skip_set {
                        cur_set = Some(LayerSet { state: t.arg(0).to_string(), ..Default::default() });
                    }
                }
                "LS_PALETTE" => {
                    if let Some(s) = cur_set.as_mut() {
                        s.palettes.push((t.arg(0).to_string(), u16::MAX, 0));
                    }
                }
                "LS_PALETTE_FILE" => {
                    let path = dir.join(t.arg(0));
                    let pi = g.palette_index(&path);
                    if let (Some(s), Some(pi)) = (cur_set.as_mut(), pi) {
                        if let Some(last) = s.palettes.last_mut() {
                            last.1 = pi;
                        }
                    }
                }
                "LS_PALETTE_DEFAULT" => {
                    if let Some(last) = cur_set.as_mut().and_then(|s| s.palettes.last_mut()) {
                        last.2 = t.arg_i32(0).unwrap_or(0).max(0) as u16;
                    }
                }
                "LAYER_GROUP" => {
                    flush(&mut cur_set, &mut layer, in_group);
                    if let Some(s) = cur_set.as_mut() {
                        s.entries.push(Entry::Group(Vec::new()));
                    }
                    in_group = true;
                }
                "END_LAYER_GROUP" => {
                    flush(&mut cur_set, &mut layer, in_group);
                    in_group = false;
                }
                "LAYER" => {
                    flush(&mut cur_set, &mut layer, in_group);
                    if let Some(sp) = sprite_ref(g, t, 1) {
                        layer = Some(Layer { sprite: sp, conds: Vec::new(), shutoff: Vec::new(), pal: Pal::None });
                    }
                }
                name => {
                    let Some(l) = layer.as_mut() else { continue };
                    match name {
                        "CONDITION_CASTE" => l.conds.push(Cond::Caste(t.args.clone())),
                        "CONDITION_PROFESSION_CATEGORY" => l.conds.push(Cond::ProfCat(t.args.clone())),
                        "CONDITION_SYN_CLASS" | "CONDITION_GHOST" | "CONDITION_MATERIAL_TYPE" | "CONDITION_DYE"
                        | "CONDITION_NOT_DYED" | "CONDITION_BP" | "CONDITION_HAUL_COUNT_MIN" => l.conds.push(Cond::Never),
                        "CONDITION_NOT_CHILD" | "CONDITION_HAUL_COUNT_MAX" => {}
                        "CONDITION_ITEM_WORN" => l.conds.push(Cond::Worn(item_req(t))),
                        "SHUT_OFF_IF_ITEM_PRESENT" => l.shutoff.push(item_req(t)),
                        "CONDITION_TISSUE_LAYER" => l.conds.push(Cond::Tissue(TissueCond { tissue: t.arg(2).to_string(), ..Default::default() })),
                        "TISSUE_MAY_HAVE_COLOR" | "TISSUE_MIN_LENGTH" | "TISSUE_MAX_LENGTH" | "TISSUE_MAY_HAVE_SHAPING" | "TISSUE_NOT_SHAPED" => {
                            if let Some(Cond::Tissue(tc)) = l.conds.last_mut() {
                                match name {
                                    "TISSUE_MAY_HAVE_COLOR" => tc.colors.extend(t.args.iter().cloned()),
                                    "TISSUE_MIN_LENGTH" => tc.min_len = t.arg_i32(0),
                                    "TISSUE_MAX_LENGTH" => tc.max_len = t.arg_i32(0),
                                    "TISSUE_MAY_HAVE_SHAPING" => tc.shapings.extend(t.args.iter().cloned()),
                                    _ => tc.not_shaped = true,
                                }
                            }
                        }
                        "CONDITION_RANDOM_PART_INDEX" => {
                            let salt = t.arg(0).bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
                            l.conds.push(Cond::Random {
                                idx: t.arg_i32(1).unwrap_or(0).max(0) as u32,
                                count: t.arg_i32(2).unwrap_or(1).max(1) as u32,
                                salt,
                            })
                        }
                        "CONDITION_MATERIAL_FLAG" => {
                            let ok = t.args.iter().all(|a| matches!(a.as_str(), "NOT_ARTIFACT" | "WOVEN_ITEM"));
                            l.conds.push(Cond::MatFlag(ok))
                        }
                        "USE_PALETTE" => {
                            if let Some(s) = cur_set.as_ref() {
                                if let Some(i) = s.palettes.iter().position(|p| p.0 == t.arg(0)) {
                                    l.pal = Pal::Ls(i as u8, t.arg_i32(1).unwrap_or(0).max(0) as u16);
                                }
                            }
                        }
                        "USE_STANDARD_PALETTE_FROM_ITEM" => l.pal = Pal::StdFromItem,
                        _ => {}
                    }
                }
            }
        }
        flush(&mut cur_set, &mut layer, in_group);
        if let Some(s) = cur_set.take() {
            cg.sets.push(s);
        }
        out.insert(o.id.clone(), cg);
    }
    // Caste-specific simple sprites (rooster/hen, drake/duck, ram/ewe, ...).
    for o in raws.of_kind("CREATURE_CASTE_GRAPHICS") {
        let caste = o.header().arg(1).to_string();
        let (mut d, mut c, mut k) = (None, None, None);
        for t in &o.tokens[1..] {
            match t.name.as_str() {
                "DEFAULT" if d.is_none() => d = sprite_ref(g, t, 0),
                "CHILD" if c.is_none() && t.args.last().map(|s| s == "DEFAULT").unwrap_or(false) => c = sprite_ref(g, t, 0),
                "CORPSE" if k.is_none() => k = sprite_ref(g, t, 0),
                "LAYER_SET" => break,
                _ => {}
            }
        }
        if d.is_none() && c.is_none() && k.is_none() {
            continue;
        }
        let cg = out.entry(o.id.clone()).or_default();
        match cg.castes.iter_mut().find(|e| e.0 == caste) {
            Some(e) => {
                e.1 = e.1.or(d);
                e.2 = e.2.or(c);
                e.3 = e.3.or(k);
            }
            None => cg.castes.push((caste, d, c, k)),
        }
        // A race whose graphics are all per caste still gets a default (the first caste's).
        cg.default = cg.default.or(d);
        cg.corpse = cg.corpse.or(k);
    }
    out
}

/// [gfx6] Body plan of a procedural creature (forgotten beasts, demons, titans, ...), matching
/// DF Premium's `BEAST_<TYPE>` sprites.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BeastBody {
    Amorphous,
    Snake,
    WormShort,
    Insect,
    Spider,
    Scorpion,
    BipedalDinosaur,
    Humanoid,
    FrontGrasp,
    QuadrupedBulky,
    QuadrupedSlinky,
}

impl BeastBody {
    fn token(self) -> &'static str {
        match self {
            BeastBody::Amorphous => "AMORPHOUS",
            BeastBody::Snake => "SNAKE",
            BeastBody::WormShort => "WORM_SHORT",
            BeastBody::Insect => "INSECT",
            BeastBody::Spider => "SPIDER",
            BeastBody::Scorpion => "SCORPION",
            BeastBody::BipedalDinosaur => "BIPEDAL_DINOSAUR",
            BeastBody::Humanoid => "HUMANOID",
            BeastBody::FrontGrasp => "FRONT_GRASP",
            BeastBody::QuadrupedBulky => "QUADRUPED_BULKY",
            BeastBody::QuadrupedSlinky => "QUADRUPED_SLINKY",
        }
    }
}

/// Classify a body from its parts (`STANCE` limbs = legs, `GRASP` parts = hands, wings).
/// Returns (body, has wings).
pub(crate) fn beast_body(parts: &[rf_core::BodyPartRaw], seed: u32) -> (BeastBody, bool) {
    let has = |p: &rf_core::BodyPartRaw, f: &str| p.flags.iter().any(|x| x == f);
    let legs = parts.iter().filter(|p| has(p, "STANCE") && has(p, "LIMB")).count();
    let hands = parts.iter().filter(|p| has(p, "GRASP")).count();
    let wings = parts.iter().any(|p| p.category.contains("WING") || (has(p, "FLIER") && has(p, "LIMB")));
    let head = parts.iter().any(|p| has(p, "HEAD") && !has(p, "UPPERBODY"));
    let tail = parts.iter().any(|p| p.category.contains("TAIL"));
    let body = match legs {
        0 if head && tail => BeastBody::Snake,
        0 if head => BeastBody::WormShort,
        0 => BeastBody::Amorphous,
        1..=2 if hands > 0 => BeastBody::Humanoid,
        1..=2 => BeastBody::BipedalDinosaur,
        3..=4 if hands > 0 => BeastBody::FrontGrasp,
        3..=4 => {
            if seed % 2 == 0 {
                BeastBody::QuadrupedBulky
            } else {
                BeastBody::QuadrupedSlinky
            }
        }
        5..=6 => BeastBody::Insect,
        7..=8 => BeastBody::Spider,
        _ => BeastBody::Scorpion,
    };
    (body, wings)
}

/// [gfx6] Look of a creature without graphics raws: DF Premium's procedural beast sprites
/// (`BEAST_<BODY>` 3x2 tiles, the `BEAST_ORGANIC_<BODY>` details on top for flesh, plus wing
/// layers), chosen from
/// the creature's exported body parts; the sim's forgotten beasts (`FORGOTTEN_BEAST_n`, no
/// raws) by their quadruped flag. Plain (stone-like) for forgotten beasts (most are made of a
/// mineral),
/// organic for demons and other generated creatures. Not recolored (the unit carries no
/// material). `None` if the beast pages are missing.
pub(crate) fn procedural_look(g: &Gfx, world: &World, u: &Unit) -> Option<Vec<Sprite>> {
    let seed = u.race.bytes().fold(0x811C_9DC5u32, |a, b| (a ^ b as u32).wrapping_mul(0x0100_0193));
    let raw = world.creature_raws.iter().find(|c| c.token == u.race);
    let (body, wings) = match raw {
        Some(c) => {
            let caste = c.castes.iter().find(|k| k.token == u.caste).or(c.castes.first());
            match caste {
                Some(k) if !k.body_parts.is_empty() => beast_body(&k.body_parts, seed),
                _ => (BeastBody::Humanoid, c.flags.iter().any(|f| f == "HAS_ANY_FLIER")),
            }
        }
        None if u.race.starts_with("FORGOTTEN_BEAST") || u.race.starts_with("TITAN") => {
            let b = if u.flags.animal {
                if seed % 2 == 0 {
                    BeastBody::QuadrupedBulky
                } else {
                    BeastBody::QuadrupedSlinky
                }
            } else {
                BeastBody::Humanoid
            };
            (b, false)
        }
        None => return None,
    };
    let stone = u.race.starts_with("FORGOTTEN_BEAST");
    let n = |s: String| g.named.get(&s).copied();
    let base = n(format!("BEAST_{}", body.token()))?;
    // Flesh creatures: the organic details (mouths, teeth, flesh folds) on top of the body.
    let organic = if stone { None } else { n(format!("BEAST_ORGANIC_{}", body.token())) };
    let kind = match (raw.map(|c| c.flags.iter().any(|f| f == "HAS_ANY_DEMON")).unwrap_or(false), body) {
        (true, _) => "BAT",
        (_, BeastBody::Insect | BeastBody::Spider | BeastBody::Scorpion) => "LACY",
        _ => "FEATHERED",
    };
    let wing = |side: &str| if wings { n(format!("BEAST_{}_WINGS_{kind}_{side}", body.token())) } else { None };
    let mut v = Vec::with_capacity(3);
    v.extend(wing("BACK"));
    v.push(base);
    v.extend(organic);
    v.extend(wing("FRONT"));
    // Bottom-aligned like DF's large creature sprites (centered by `Sprite::unit_center_px`).
    for s in &mut v {
        s.dy = -(s.th as i16 - 1);
    }
    Some(v)
}

/// DF profession -> graphics profession category.
pub(crate) fn profession_category(p: &str) -> &'static str {
    match p {
        "MINER" => "MINER",
        "WOODWORKER" | "CARPENTER" | "BOWYER" | "WOODCUTTER" => "WOODWORKER",
        "STONEWORKER" | "ENGRAVER" | "MASON" => "STONEWORKER",
        "RANGER" | "ANIMAL_TRAINER" | "HUNTER" | "TRAPPER" | "ANIMAL_DISSECTOR" | "ANIMAL_CARETAKER" => "RANGER",
        "METALSMITH" | "FURNACE_OPERATOR" | "WEAPONSMITH" | "ARMORER" | "BLACKSMITH" | "METALCRAFTER" => "METALSMITH",
        "JEWELER" | "GEM_CUTTER" | "GEM_SETTER" => "JEWELER",
        "CRAFTSMAN" | "WOODCRAFTER" | "STONECRAFTER" | "LEATHERWORKER" | "BONE_CARVER" | "WEAVER" | "CLOTHIER"
        | "GLASSMAKER" | "POTTER" | "GLAZER" | "WAX_WORKER" | "STRAND_EXTRACTOR" | "PAPERMAKER" | "BOOKBINDER" => "CRAFTSMAN",
        "FISHERY_WORKER" | "FISHERMAN" | "FISH_DISSECTOR" | "FISH_CLEANER" => "FISHERY_WORKER",
        "FARMER" | "CHEESE_MAKER" | "MILKER" | "COOK" | "THRESHER" | "MILLER" | "BUTCHER" | "TANNER" | "DYER"
        | "PLANTER" | "HERBALIST" | "BREWER" | "SOAP_MAKER" | "POTASH_MAKER" | "LYE_MAKER" | "WOOD_BURNER"
        | "SHEARER" | "SPINNER" | "PRESSER" | "BEEKEEPER" | "GELDER" => "FARMER",
        "ENGINEER" | "MECHANIC" | "SIEGE_ENGINEER" | "SIEGE_OPERATOR" | "PUMP_OPERATOR" => "ENGINEER",
        "MERCHANT" | "TRADER" => "MERCHANT",
        _ => "STANDARD",
    }
}

struct Ctx {
    caste: String,
    prof_cat: &'static str,
    id: u32,
    skin: &'static str,
    hair: (&'static str, i32, &'static str),
    beard: Option<(&'static str, i32, &'static str)>,
    items: Vec<(String, String, String)>, // (part, kind, subtype)
}

fn h32(x: u32, salt: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h
}

fn has_item(ctx: &Ctx, r: &ItemReq) -> bool {
    ctx.items.iter().any(|(part, kind, sub)| *part == r.part && *kind == r.kind && (r.subtypes.is_empty() || r.subtypes.iter().any(|s| s == sub)))
}

fn cond_ok(ctx: &Ctx, c: &Cond) -> bool {
    match c {
        Cond::Caste(v) => v.iter().any(|x| *x == ctx.caste),
        Cond::ProfCat(v) => v.iter().any(|x| x == ctx.prof_cat),
        Cond::Never => false,
        Cond::Worn(r) => has_item(ctx, r),
        Cond::MatFlag(b) => *b,
        Cond::Random { idx, count, salt } => h32(ctx.id, *salt) % count == *idx,
        Cond::Tissue(tc) => {
            let (color, len, shape) = match tc.tissue.as_str() {
                "SKIN" => (ctx.skin, 1, ""),
                "HAIR" => ctx.hair,
                "CHIN_WHISKERS" | "MOUSTACHE" | "SIDEBURNS" => match ctx.beard {
                    Some(b) => b,
                    None => return false,
                },
                _ => return false,
            };
            (tc.colors.is_empty() || tc.colors.iter().any(|c| c == color))
                && tc.min_len.map(|m| len >= m).unwrap_or(true)
                && tc.max_len.map(|m| len <= m).unwrap_or(true)
                && (tc.shapings.is_empty() || tc.shapings.iter().any(|s| s == shape))
                && (!tc.not_shaped || shape.is_empty())
        }
    }
}

fn make_ctx(world: &World, u: &Unit) -> Ctx {
    let id = u.id as u32;
    const SKINS: [&str; 5] = ["PALE_PINK", "PEACH", "TAN", "BROWN", "DARK_BROWN"];
    const HAIRS: [&str; 6] = ["BLACK", "BROWN", "RUSSET", "GOLD", "AUBURN", "CHESTNUT"];
    const SHAPES: [&str; 5] = ["", "BRAIDED", "DOUBLE_BRAIDS", "NEATLY_COMBED", "PONY_TAILS"];
    let old = u.age > 120.0;
    let hair_c = if old { "GRAY" } else { HAIRS[(h32(id, 2) % HAIRS.len() as u32) as usize] };
    let hair = (hair_c, (h32(id, 3) % 260) as i32, SHAPES[(h32(id, 4) % SHAPES.len() as u32) as usize]);
    let beard = if u.caste == "MALE" {
        Some((hair_c, 60 + (h32(id, 5) % 220) as i32, SHAPES[(h32(id, 6) % SHAPES.len() as u32) as usize]))
    } else {
        None
    };
    let prof = if u.custom_profession.is_empty() { u.profession.as_str() } else { u.profession.as_str() };
    let mut items: Vec<(String, String, String)> = Vec::new();
    for it in world.items.iter().filter(|i| i.holder == u.id || u.inventory.contains(&i.id)) {
        let parts: &[&str] = match it.kind.as_str() {
            "ARMOR" => &["BODY_UPPER"],
            "PANTS" => &["BODY_LOWER"],
            "SHOES" => &["RF", "LF"],
            "HELM" => &["HEAD"],
            "GLOVES" => &["RH", "LH"],
            "WEAPON" | "TOOL" => &["RH"],
            "SHIELD" => &["LH"],
            _ => &[],
        };
        for p in parts {
            items.push((p.to_string(), it.kind.clone(), it.subtype.clone()));
        }
    }
    if items.is_empty() && !u.flags.animal {
        // Default outfit (the export does not carry worn items yet).
        for (p, k, s) in [
            ("BODY_UPPER", "ARMOR", "ITEM_ARMOR_SHIRT"),
            ("BODY_LOWER", "PANTS", "ITEM_PANTS_TROUSERS"),
            ("RF", "SHOES", "ITEM_SHOES_SHOES"),
            ("LF", "SHOES", "ITEM_SHOES_SHOES"),
        ] {
            items.push((p.into(), k.into(), s.into()));
        }
        let tool = match prof {
            "MINER" => Some("ITEM_WEAPON_PICK"),
            "WOODCUTTER" => Some("ITEM_WEAPON_AXE_BATTLE"),
            _ => None,
        };
        if let Some(t) = tool {
            items.push(("RH".into(), "WEAPON".into(), t.into()));
        }
    }
    Ctx {
        caste: u.caste.clone(),
        prof_cat: profession_category(prof),
        id,
        skin: SKINS[(h32(id, 1) % SKINS.len() as u32) as usize],
        hair,
        beard,
        items,
    }
}

/// Evaluate a unit's layered graphics: sprites back to front, each with an optional palette
/// swap (palette index, row) to bake. `None` if the race has no layered set.
pub(crate) fn evaluate(g: &Gfx, world: &World, u: &Unit) -> Option<Vec<(Sprite, Option<(u16, u16)>)>> {
    let cg = g.creatures.get(&u.race)?;
    if cg.sets.is_empty() {
        return None;
    }
    let state = match u.profession.as_str() {
        "CHILD" => "CHILD",
        "BABY" => "BABY",
        _ => "DEFAULT",
    };
    let set = cg.sets.iter().find(|s| s.state == state).or_else(|| cg.sets.iter().find(|s| s.state == "DEFAULT"))?;
    let ctx = make_ctx(world, u);
    let ok = |l: &Layer| l.conds.iter().all(|c| cond_ok(&ctx, c)) && !l.shutoff.iter().any(|r| has_item(&ctx, r));
    let mut out = Vec::new();
    let mut emit = |l: &Layer| {
        let pal = match l.pal {
            Pal::Ls(i, row) => set.palettes.get(i as usize).filter(|p| p.1 != u16::MAX && row != p.2).map(|p| (p.1, row)),
            _ => None,
        };
        out.push((l.sprite, pal));
    };
    for e in &set.entries {
        match e {
            Entry::Single(l) => {
                if ok(l) {
                    emit(l)
                }
            }
            Entry::Group(v) => {
                if let Some(l) = v.iter().find(|l| ok(l)) {
                    emit(l)
                }
            }
        }
    }
    Some(out)
}

pub(crate) fn unit(g: &Gfx, _world: &World, u: &Unit, out: &mut Vec<Sprite>) {
    if let Some(look) = g.look(u.id) {
        out.extend_from_slice(&look.sprites);
        return;
    }
    let Some(cg) = g.creatures.get(&u.race) else { return };
    let child = matches!(u.profession.as_str(), "CHILD" | "BABY");
    // Caste-specific sprites first (rooster vs hen).
    if let Some(c) = cg.castes.iter().find(|c| c.0 == u.caste) {
        if let Some(s) = if child { c.2.or(c.1) } else { c.1 } {
            out.push(s);
            return;
        }
    }
    if let Some(s) = if child { cg.child.or(cg.default) } else { cg.default } {
        out.push(s);
    }
}
