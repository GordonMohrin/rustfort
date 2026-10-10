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
    /// [feel-ui] `[ITEM_QUALITY:n]` after the condition (portraits): the worn item's quality.
    pub quality: Option<u8>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TissueCond {
    pub tissue: String,
    pub colors: Vec<String>,
    pub min_len: Option<i32>,
    pub max_len: Option<i32>,
    pub shapings: Vec<String>,
    pub not_shaped: bool,
    /// [feel-ui] `TISSUE_MIN_DENSITY` / `TISSUE_MAX_DENSITY` (portrait eyebrows).
    pub min_density: Option<i32>,
    pub max_density: Option<i32>,
}

/// [feel-ui] `[CONDITION_BP:BY_CATEGORY|BY_TOKEN:part]` with its `BP_APPEARANCE_MODIFIER_RANGE`s
/// (portrait faces: broad/narrow head, deep-set eyes, upturned nose, ...). `BP_MISSING` and
/// `BP_SCARRED` never hold (Rustfort has no lost parts or scars to show).
#[derive(Clone, Debug, Default)]
pub(crate) struct BpCond {
    pub part: String,
    pub mods: Vec<(String, i32, i32)>,
    pub never: bool,
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
    /// [feel-ui] body part condition (portraits).
    Bp(BpCond),
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
    /// [feel-ui] `TISSUE_SWAP:IF_MIN_CURLY:n:...`: (min curliness, sprite) for curly hair.
    pub swap: Option<(i32, Sprite)>,
}

#[derive(Clone, Debug)]
pub(crate) enum Entry {
    Single(Layer),
    Group(Vec<Layer>),
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LayerSet {
    pub state: String,
    /// [feel-ui] A `[LAYER_SET:...:PORTRAIT]` (unit sheet portrait), not a map sprite set.
    pub portrait: bool,
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
    /// [feel-ui] Portrait layer sets (`[LAYER_SET:PORTRAIT]`, `CHILD:PORTRAIT`, `BABY:PORTRAIT`).
    pub portraits: Vec<LayerSet>,
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
    ItemReq { part: t.arg(1).to_string(), kind: t.arg(2).to_string(), subtypes: t.args.iter().skip(3).cloned().collect(), quality: None }
}

pub(crate) fn parse_all(g: &mut Gfx, raws: &Raws) -> HashMap<String, CreatureGfx> {
    let mut out: HashMap<String, CreatureGfx> = HashMap::new();
    for o in raws.of_kind("CREATURE_GRAPHICS") {
        let dir = raws.file_dir(o);
        // Several CREATURE_GRAPHICS objects per creature add up (sprites, portraits, ...).
        let mut cg = out.remove(&o.id).unwrap_or_default();
        let mut cur_set: Option<LayerSet> = None;
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
        // [feel-ui] Finished sets go to the map sets or the portraits.
        fn push_set(cg: &mut CreatureGfx, s: LayerSet) {
            if s.portrait {
                cg.portraits.push(s);
            } else {
                cg.sets.push(s);
            }
        }
        for t in &o.tokens[1..] {
            match t.name.as_str() {
                "DEFAULT" if cur_set.is_none() => cg.default = sprite_ref(g, t, 0),
                "CORPSE" if cur_set.is_none() && cg.corpse.is_none() => cg.corpse = sprite_ref(g, t, 0),
                "REMAINS" if cur_set.is_none() && cg.remains.is_none() => cg.remains = sprite_ref(g, t, 0),
                "CHILD" if cur_set.is_none() && t.args.last().map(|s| s == "DEFAULT").unwrap_or(false) => cg.child = sprite_ref(g, t, 0),
                "LAYER_SET" => {
                    flush(&mut cur_set, &mut layer, in_group);
                    if let Some(s) = cur_set.take() {
                        push_set(&mut cg, s);
                    }
                    in_group = false;
                    // [feel-ui] Portrait sets (for the unit sheet): `[LAYER_SET:PORTRAIT]` is the
                    // adult one, `[LAYER_SET:CHILD:PORTRAIT]` the child's.
                    let portrait = t.args.iter().any(|a| a == "PORTRAIT");
                    let state = if portrait && t.args.len() < 2 { "DEFAULT".to_string() } else { t.arg(0).to_string() };
                    cur_set = Some(LayerSet { state, portrait, ..Default::default() });
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
                        layer = Some(Layer { sprite: sp, conds: Vec::new(), shutoff: Vec::new(), pal: Pal::None, swap: None });
                    }
                }
                name => {
                    let Some(l) = layer.as_mut() else { continue };
                    match name {
                        "CONDITION_CASTE" => l.conds.push(Cond::Caste(t.args.clone())),
                        "CONDITION_PROFESSION_CATEGORY" => l.conds.push(Cond::ProfCat(t.args.clone())),
                        "CONDITION_SYN_CLASS" | "CONDITION_GHOST" | "CONDITION_MATERIAL_TYPE" | "CONDITION_DYE"
                        | "CONDITION_NOT_DYED" | "CONDITION_HAUL_COUNT_MIN" => l.conds.push(Cond::Never),
                        // [feel-ui] body part conditions (portrait faces)
                        "CONDITION_BP" => l.conds.push(Cond::Bp(BpCond { part: t.arg(1).to_string(), ..Default::default() })),
                        "BP_APPEARANCE_MODIFIER_RANGE" | "BP_MISSING" | "BP_SCARRED" => {
                            if let Some(Cond::Bp(b)) = l.conds.last_mut() {
                                if name == "BP_APPEARANCE_MODIFIER_RANGE" {
                                    b.mods.push((t.arg(0).to_string(), t.arg_i32(1).unwrap_or(0), t.arg_i32(2).unwrap_or(i32::MAX)));
                                } else {
                                    b.never = true;
                                }
                            }
                        }
                        "ITEM_QUALITY" => {
                            if let Some(Cond::Worn(r)) = l.conds.iter_mut().rev().find(|c| matches!(c, Cond::Worn(_))) {
                                r.quality = t.arg_i32(0).map(|q| q.clamp(0, 6) as u8);
                            }
                        }
                        "CONDITION_NOT_CHILD" | "CONDITION_HAUL_COUNT_MAX" => {}
                        "CONDITION_ITEM_WORN" => l.conds.push(Cond::Worn(item_req(t))),
                        "SHUT_OFF_IF_ITEM_PRESENT" => l.shutoff.push(item_req(t)),
                        "CONDITION_TISSUE_LAYER" => l.conds.push(Cond::Tissue(TissueCond { tissue: t.arg(2).to_string(), ..Default::default() })),
                        "TISSUE_MAY_HAVE_COLOR" | "TISSUE_MIN_LENGTH" | "TISSUE_MAX_LENGTH" | "TISSUE_MAY_HAVE_SHAPING" | "TISSUE_NOT_SHAPED"
                        | "TISSUE_MIN_DENSITY" | "TISSUE_MAX_DENSITY" => {
                            if let Some(Cond::Tissue(tc)) = l.conds.last_mut() {
                                match name {
                                    "TISSUE_MAY_HAVE_COLOR" => tc.colors.extend(t.args.iter().cloned()),
                                    "TISSUE_MIN_LENGTH" => tc.min_len = t.arg_i32(0),
                                    "TISSUE_MAX_LENGTH" => tc.max_len = t.arg_i32(0),
                                    "TISSUE_MAY_HAVE_SHAPING" => tc.shapings.extend(t.args.iter().cloned()),
                                    "TISSUE_MIN_DENSITY" => tc.min_density = t.arg_i32(0), // [feel-ui]
                                    "TISSUE_MAX_DENSITY" => tc.max_density = t.arg_i32(0),
                                    _ => tc.not_shaped = true,
                                }
                            }
                        }
                        "TISSUE_SWAP" if t.arg(0) == "IF_MIN_CURLY" => {
                            // [feel-ui] curly hair uses another sprite (same palette)
                            if let (Some(min), Some(sp)) = (t.arg_i32(1), sprite_ref(g, t, 2)) {
                                l.swap = Some((min, sp));
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
            push_set(&mut cg, s);
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

/// A worn (or default-outfit) item: body part, item kind, subtype, material, quality.
#[derive(Clone, Debug)]
struct Worn {
    part: &'static str,
    kind: String,
    sub: String,
    /// `NO_MAT` for the default outfit.
    mat: u16,
    quality: u8,
}

struct Ctx<'a> {
    caste: String,
    prof_cat: &'static str,
    id: u32,
    skin: &'a str,
    hair: (&'a str, i32, &'static str),
    beard: Option<(&'a str, i32, &'static str)>,
    items: Vec<Worn>,
    /// [feel-ui] Hair curliness (portrait `TISSUE_SWAP:IF_MIN_CURLY`) and eyebrow density.
    curly: i32,
    brow_density: i32,
}

fn h32(x: u32, salt: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h
}

fn worn<'a>(ctx: &'a Ctx, r: &ItemReq) -> Option<&'a Worn> {
    ctx.items.iter().find(|w| {
        w.part == r.part && w.kind == r.kind && (r.subtypes.is_empty() || r.subtypes.iter().any(|s| *s == w.sub)) && r.quality.map(|q| q == w.quality).unwrap_or(true)
    })
}

fn has_item(ctx: &Ctx, r: &ItemReq) -> bool {
    worn(ctx, r).is_some()
}

/// [feel-ui] Deterministic appearance modifier (0..=200, DF's 100 = average) of a body part,
/// e.g. (HEAD, BROADNESS): the same value for every layer that asks, so face parts agree.
fn appearance(id: u32, part: &str, m: &str) -> i32 {
    let salt = part.bytes().chain(m.bytes()).fold(0x2545_F491u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let v = (h32(id, salt) % 201) as i32;
    if v == 100 {
        99
    } else {
        v
    }
}

fn cond_ok(ctx: &Ctx, c: &Cond) -> bool {
    match c {
        Cond::Caste(v) => v.iter().any(|x| *x == ctx.caste),
        Cond::ProfCat(v) => v.iter().any(|x| x == ctx.prof_cat),
        Cond::Never => false,
        Cond::Worn(r) => has_item(ctx, r),
        Cond::MatFlag(b) => *b,
        // [feel-ui] DF's part indices are 1-based (`CAPE:1:3` .. `CAPE:3:3`): every unit gets one.
        Cond::Random { idx, count, salt } => h32(ctx.id, *salt) % count + 1 == *idx,
        Cond::Bp(b) => {
            !b.never
                && b.mods.iter().all(|(m, lo, hi)| {
                    let v = appearance(ctx.id, &b.part, m);
                    v >= *lo && v <= *hi
                })
        }
        Cond::Tissue(tc) => {
            let mut density = 100;
            let (color, len, shape) = match tc.tissue.as_str() {
                "SKIN" => (ctx.skin, 1, ""),
                "HAIR" => ctx.hair,
                "CHIN_WHISKERS" | "MOUSTACHE" | "SIDEBURNS" => match ctx.beard {
                    Some(b) => b,
                    None => return false,
                },
                // [feel-ui] portrait eyebrows: hair color, own density
                "EYEBROW" => {
                    density = ctx.brow_density;
                    (ctx.hair.0, 1, "")
                }
                _ => return false,
            };
            (tc.colors.is_empty() || tc.colors.iter().any(|c| c == color))
                && tc.min_len.map(|m| len >= m).unwrap_or(true)
                && tc.max_len.map(|m| len <= m).unwrap_or(true)
                && tc.min_density.map(|m| density >= m).unwrap_or(true)
                && tc.max_density.map(|m| density <= m).unwrap_or(true)
                && (tc.shapings.is_empty() || tc.shapings.iter().any(|s| s == shape))
                && (!tc.not_shaped || shape.is_empty())
        }
    }
}

fn make_ctx(world: &World, u: &Unit) -> Ctx<'static> {
    let id = u.id as u32;
    const SKINS: [&str; 5] = ["PALE_PINK", "PEACH", "TAN", "BROWN", "DARK_BROWN"];
    const HAIRS: [&str; 6] = ["BLACK", "BROWN", "RUSSET", "GOLD", "AUBURN", "CHESTNUT"];
    const SHAPES: [&str; 5] = ["", "BRAIDED", "DOUBLE_BRAIDS", "NEATLY_COMBED", "PONY_TAILS"];
    let old = u.age > 120.0;
    let hair_c = if old { "GRAY" } else { HAIRS[(h32(id, 2) % HAIRS.len() as u32) as usize] };
    let hair = (hair_c, (h32(id, 3) % 260) as i32, SHAPES[(h32(id, 4) % SHAPES.len() as u32) as usize]);
    let beard = if u.caste == "MALE" {
        // [feel-ui] Beards have no pony tails in DF's raws (that shape matched no beard layer).
        let sh = match SHAPES[(h32(id, 6) % SHAPES.len() as u32) as usize] {
            "PONY_TAILS" => "",
            x => x,
        };
        Some((hair_c, 60 + (h32(id, 5) % 220) as i32, sh))
    } else {
        None
    };
    let prof = if u.custom_profession.is_empty() { u.profession.as_str() } else { u.profession.as_str() };
    let mut items: Vec<Worn> = Vec::new();
    for it in world.items.iter().filter(|i| i.holder == u.id || u.inventory.contains(&i.id)) {
        let parts: &[&'static str] = match it.kind.as_str() {
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
            items.push(Worn { part: p, kind: it.kind.clone(), sub: it.subtype.clone(), mat: it.mat, quality: it.quality.min(5) });
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
            items.push(Worn { part: p, kind: k.into(), sub: s.into(), mat: rf_core::NO_MAT, quality: 0 });
        }
        let tool = match prof {
            "MINER" => Some("ITEM_WEAPON_PICK"),
            "WOODCUTTER" => Some("ITEM_WEAPON_AXE_BATTLE"),
            _ => None,
        };
        if let Some(t) = tool {
            items.push(Worn { part: "RH", kind: "WEAPON".into(), sub: t.into(), mat: rf_core::NO_MAT, quality: 0 });
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
        curly: (h32(id, 7) % 250) as i32,
        brow_density: if h32(id, 8) % 2 == 0 { 50 + (h32(id, 9) % 41) as i32 } else { 110 + (h32(id, 9) % 41) as i32 },
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

/// One evaluated portrait layer: sprite plus the palette swap to bake ((palette, row)).
pub(crate) type PortraitLayer = (Sprite, Option<(u16, u16)>);

/// Cloth colors (standard palette names) for the default outfit, which has no material.
const OUTFIT_COLORS: [&str; 6] = ["BROWN", "TAUPE_MEDIUM", "DARK_TAN", "GRAY", "OCHRE", "PALE_BROWN"];

/// [feel-ui] Evaluate a unit's portrait (`[LAYER_SET:PORTRAIT]`) with the same layer rules as the
/// map sprites: skin, eyes, nose, ears, hair, beard and eyebrows from the unit's (stand-in)
/// tissues and appearance modifiers, clothing and armor from its worn items (recolored by
/// their material through the standard palette, `USE_STANDARD_PALETTE_FROM_ITEM`).
/// `None` if the race has no portrait set.
pub(crate) fn evaluate_portrait(g: &Gfx, world: &World, u: &Unit) -> Option<Vec<PortraitLayer>> {
    let cg = g.creatures.get(&u.race)?;
    if cg.portraits.is_empty() {
        return None;
    }
    let state = match u.profession.as_str() {
        "CHILD" => "CHILD",
        "BABY" => "BABY",
        _ => "DEFAULT",
    };
    let set = cg.portraits.iter().find(|s| s.state == state).or_else(|| cg.portraits.iter().find(|s| s.state == "DEFAULT"))?;
    let mut ctx = make_ctx(world, u);
    // Races whose portraits know other colors (green and gray goblins, elves, ...): when the
    // stand-in color matches none of the set's skin/hair conditions, take one of those.
    let lists = |tissue: &str| -> Vec<&Vec<String>> {
        let mut v: Vec<&Vec<String>> = Vec::new();
        for e in &set.entries {
            let ls: &[Layer] = match e {
                Entry::Single(l) => std::slice::from_ref(l),
                Entry::Group(g) => g,
            };
            for l in ls {
                for c in &l.conds {
                    if let Cond::Tissue(tc) = c {
                        if tc.tissue == tissue && !tc.colors.is_empty() && !v.iter().any(|x| **x == tc.colors) {
                            v.push(&tc.colors);
                        }
                    }
                }
            }
        }
        v
    };
    let skins = lists("SKIN");
    if !skins.is_empty() && !skins.iter().any(|l| l.iter().any(|c| c == ctx.skin)) {
        ctx.skin = skins[(h32(ctx.id, 21) % skins.len() as u32) as usize][0].as_str();
    }
    let hairs = lists("HAIR");
    if !hairs.is_empty() && !hairs.iter().any(|l| l.iter().any(|c| c == ctx.hair.0)) {
        let c = hairs[(h32(ctx.id, 22) % hairs.len() as u32) as usize][0].as_str();
        ctx.hair.0 = c;
        if let Some(b) = ctx.beard.as_mut() {
            b.0 = c;
        }
    }
    let ctx = ctx;
    let ok = |l: &Layer| l.conds.iter().all(|c| cond_ok(&ctx, c)) && !l.shutoff.iter().any(|r| has_item(&ctx, r));
    let mut out = Vec::new();
    let mut emit = |l: &Layer| {
        let sprite = match l.swap {
            Some((min, sp)) if ctx.curly >= min => sp,
            _ => l.sprite,
        };
        let pal = match l.pal {
            Pal::Ls(i, row) => set.palettes.get(i as usize).filter(|p| p.1 != u16::MAX && row != p.2).map(|p| (p.1, row)),
            Pal::StdFromItem => {
                // The item the layer is about (its first CONDITION_ITEM_WORN).
                let it = l.conds.iter().find_map(|c| match c {
                    Cond::Worn(r) => worn(&ctx, r),
                    _ => None,
                });
                let row = match it {
                    Some(w) if w.mat != rf_core::NO_MAT => g.mat_row(world, w.mat),
                    Some(w) => {
                        let salt = w.kind.bytes().fold(11u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
                        let name = OUTFIT_COLORS[(h32(ctx.id, salt) % OUTFIT_COLORS.len() as u32) as usize];
                        g.std_rows.get(name).copied().unwrap_or(0)
                    }
                    None => 0,
                };
                g.std_pal.filter(|_| row != 0).map(|p| (p, row))
            }
            Pal::None => None,
        };
        out.push((sprite, pal));
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
