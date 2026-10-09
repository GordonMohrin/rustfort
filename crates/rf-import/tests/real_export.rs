//! Loads the real fort export if present (`export/tileshtulon` or `RUSTFORT_EXPORT`) and
//! checks it against its manifest. Skips (passes) when no export exists, so CI without DF
//! data stays green.

use rf_core::{Shape, NO_MAT};

#[test]
fn real_export_matches_manifest() {
    let Some(dir) = rf_import::find_export(None) else {
        eprintln!("SKIP: no export found (export/tileshtulon or RUSTFORT_EXPORT)");
        return;
    };
    let m = rf_import::load_manifest(&dir).expect("manifest");
    let (w, rep) = rf_import::load_export(&dir).expect("load_export");
    eprintln!("loaded {} in {:.3} s, {} warnings", dir.display(), rep.seconds, rep.warnings.len());

    // dims and tiles
    assert_eq!(w.dims, m.dims);
    assert_eq!(w.tiles.len(), m.dims.x as usize * m.dims.y as usize * m.dims.z as usize);
    // counts as written by the exporter
    for (key, got) in [
        ("units", rep.units),
        ("citizens", rep.citizens),
        ("items", rep.items),
        ("buildings", rep.buildings),
        ("plants", rep.plants),
        ("materials", w.materials.len()),
        ("tiletypes", w.tiletypes.infos.len()),
    ] {
        if let Some(want) = m.count(key) {
            assert_eq!(want, got as i64, "count {key}");
        }
    }
    assert!(!rep.warnings.iter().any(|s| s.starts_with("counts:")), "{:?}", rep.warnings);
    assert!(!rep.warnings.iter().any(|s| s.starts_with("map:")), "map warnings: {:?}", rep.warnings);

    // every reference used by other crates must resolve
    for t in &w.tiles {
        assert!(t.mat == NO_MAT || (t.mat as usize) < w.materials.len());
        assert!((t.tt as usize) < w.tiletypes.infos.len());
    }
    for i in &w.items {
        assert!(i.mat == NO_MAT || (i.mat as usize) < w.materials.len());
    }

    // round-2 data (exporter rev 2026-10-10a and later)
    if m.count("plant_raws").is_some() {
        assert!(!w.meta.mods.is_empty(), "round-2 export without mods");
        assert!(w.meta.mods.iter().all(|s| s.contains(':')), "{:?}", w.meta.mods);
        assert!(!w.plant_raws.is_empty());
        for p in &w.plant_raws {
            for mat in [p.mat_structural, p.mat_drink, p.mat_seed] {
                if mat != NO_MAT {
                    let tok = &w.materials[mat as usize].token;
                    assert!(tok.starts_with(&format!("PLANT:{}:", p.token)), "{}: material {tok}", p.token);
                }
            }
            if p.brewable && !p.is_tree {
                assert!(p.mat_drink != NO_MAT, "{} brewable without drink material", p.token);
            }
        }
        for b in w.buildings.iter().filter(|b| b.kind == "FarmPlot") {
            assert_eq!(b.crops.len(), 4, "farm plot {}", b.id);
            for c in b.crops.iter().filter(|c| !c.is_empty()) {
                assert!(w.plant_raws.iter().any(|p| &p.token == c), "farm plot {} crop {c} not in plant raws", b.id);
            }
        }
        for b in &w.buildings {
            if !b.jobs.is_empty() {
                assert!(b.kind == "Workshop" || b.kind == "Furnace", "jobs on {} {}", b.kind, b.id);
                assert!(b.jobs.iter().all(|j| !j.job.is_empty() && j.job != "ConstructBuilding"));
            }
        }
        for o in &w.work_orders {
            assert!(!o.job.is_empty() && !o.frequency.is_empty(), "order {}", o.id);
            for c in &o.conditions {
                assert!(
                    ["AtLeast", "AtMost", "GreaterThan", "LessThan", "Exactly", "Not"].contains(&c.compare.as_str()),
                    "order {} compare {}",
                    o.id,
                    c.compare
                );
            }
        }
        assert!(w.units.iter().all(|u| u.walk_speed > 0));
    }

    // round-3 data (exporter rev 2026-10-10b and later)
    if m.count("creature_raws").is_some() {
        assert_eq!(rep.creature_raws as i64, m.count("creature_raws").unwrap());
        // every unit's race has a raw with its caste, and wounds name body parts of that caste
        for u in w.units.iter().filter(|u| !u.flags.dead) {
            let cr = w.creature_raws.iter().find(|c| c.token == u.race).unwrap_or_else(|| panic!("no raw for {}", u.race));
            let caste = cr.castes.iter().find(|c| c.token == u.caste).unwrap_or_else(|| panic!("no caste {} {}", u.race, u.caste));
            for wd in u.wounds.iter().filter(|wd| !wd.body_part.is_empty()) {
                assert!(caste.body_parts.iter().any(|p| p.token == wd.body_part), "unit {} wound on {}", u.id, wd.body_part);
            }
            assert!(u.blood <= u.blood_max || u.blood_max == 0, "unit {} blood {}/{}", u.id, u.blood, u.blood_max);
        }
        let dwarf = w.creature_raws.iter().find(|c| c.token == "DWARF").expect("dwarf raw");
        let c = &dwarf.castes[0];
        assert_eq!(c.body_size, 60_000, "dwarf adult body size in cm3");
        assert!(c.body_parts.len() > 50 && !c.attacks.is_empty());
        for p in &c.body_parts {
            assert!(p.parent >= -1 && (p.parent as usize) < c.body_parts.len() || p.parent == -1);
            for l in &p.layers {
                assert!(l.mat != NO_MAT, "{} {} without material", p.token, l.tissue);
                assert!(w.materials[l.mat as usize].token.starts_with("CREATURE:DWARF:"), "{}", l.material);
            }
        }
        assert!(w.weapon_raws.iter().any(|x| x.token == "ITEM_WEAPON_AXE_BATTLE" && x.size == 800));
        assert!(w.armor_raws.iter().any(|x| x.token == "ITEM_ARMOR_BREASTPLATE" && x.layer == "ARMOR"));
        // material mechanics: most solid materials have a density
        let iron = w.materials.iter().find(|x| x.token == "INORGANIC:IRON");
        if let Some(iron) = iron {
            assert_eq!(iron.density, 7850);
            assert!(iron.impact_fracture > iron.impact_yield && iron.max_edge > 0);
        }
        // squads: members are squad members, burrows have tiles
        for s in &w.squads {
            for (pos, &uid) in s.members.iter().enumerate() {
                if let Some(u) = (uid >= 0).then(|| w.unit(uid)).flatten() {
                    assert_eq!(u.squad_id, s.id, "unit {uid} in squad {}", s.id);
                    assert_eq!(u.squad_position, pos as i32, "unit {uid} position");
                }
            }
        }
        assert_eq!(rep.burrow_tiles as i64, m.count("burrow_tiles").unwrap_or(0));
        let cits_with_needs = w.citizens().filter(|u| !u.needs.is_empty()).count();
        assert!(cits_with_needs * 10 >= w.citizens().count() * 9, "citizens with needs: {cits_with_needs}");
        assert!(!m.stress_cutoffs.is_empty());
    }

    // it is a fort: citizens exist, stand inside the map on walkable tiles (most of them)
    if m.map_complete {
        assert!(rep.citizens > 0);
        let on_map: Vec<_> = w.citizens().filter(|u| w.in_bounds(u.pos)).collect();
        assert!(!on_map.is_empty());
        let standing = on_map.iter().filter(|u| !w.shape(u.pos).is_solid()).count();
        assert!(standing * 10 >= on_map.len() * 9, "only {standing}/{} citizens on non-solid tiles", on_map.len());
        // the map has rock walls and floors, and most solid rock has a material
        let walls = w.tiles.iter().filter(|t| w.tiletypes.get(t.tt).shape == Shape::Wall).count();
        let walls_with_mat =
            w.tiles.iter().filter(|t| w.tiletypes.get(t.tt).shape == Shape::Wall && t.mat != NO_MAT).count();
        assert!(walls > w.tiles.len() / 4);
        assert!(walls_with_mat * 100 >= walls * 99, "walls with material: {walls_with_mat}/{walls}");
    }
}
