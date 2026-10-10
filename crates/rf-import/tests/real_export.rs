//! Loads a real fort export if present (`RUSTFORT_EXPORT` or the newest one under `export/`) and
//! checks it against its manifest. Skips (passes) when no export exists, so CI without DF
//! data stays green.

use rf_core::{Shape, NO_MAT};

#[test]
fn real_export_matches_manifest() {
    let Some(dir) = rf_import::find_export(None) else {
        eprintln!("SKIP: no export found (RUSTFORT_EXPORT or export/)");
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

    // round-4 data (exporter rev 2026-10-10c and later)
    if m.count("entities").is_some() {
        assert!(rep.settings, "round-4 export without settings.json");
        assert!(w.settings.population_cap > 0 && w.settings.strict_population_cap > 0);
        // the fort's civ and site government are Own; every relation is a known word
        assert!(w.entities.iter().any(|e| e.id == w.meta.civ_id && e.relation == "Own" && e.kind == "Civilization"));
        assert!(w.entities.iter().any(|e| e.id == w.meta.group_id && e.relation == "Own"));
        assert!(w.entities.iter().all(|e| ["Own", "Peace", "War", "Unknown"].contains(&e.relation.as_str())));
        // nobles: the fort government has positions, holders are exported units
        assert!(w.nobles.iter().any(|n| n.entity_id == w.meta.group_id));
        for n in w.nobles.iter().filter(|n| n.holder >= 0) {
            assert!(w.unit(n.holder).is_some(), "noble {} holder {} not exported", n.token, n.holder);
        }
        // links are symmetric and point at buildings; levers link to something they operate
        for b in &w.buildings {
            for l in &b.links {
                let o = w.building(*l).unwrap_or_else(|| panic!("building {} links to missing {l}", b.id));
                assert!(o.links.contains(&b.id), "link {} -> {l} one-sided", b.id);
            }
            if b.kind == "Bridge" {
                assert!(["N", "S", "E", "W", "RETRACT"].contains(&b.direction.as_str()), "bridge {} dir {}", b.id, b.direction);
                assert!(b.flags.iter().any(|f| f == "RAISED" || f == "LOWERED"));
            }
            if b.kind == "Door" {
                assert!(b.flags.iter().any(|f| f == "OPEN" || f == "CLOSED"), "door {} {:?}", b.id, b.flags);
            }
        }
        assert_eq!(rep.building_links as i64, m.count("building_links").unwrap_or(0));
        // hist fig names cover every relationship target and squad member figure
        let hf: std::collections::HashSet<i32> = w.hist_figs.iter().map(|h| h.id).collect();
        for u in &w.units {
            for r in u.relationships.iter().filter(|r| r.hist_fig_id >= 0) {
                assert!(hf.contains(&r.hist_fig_id), "unit {} relationship hf {} has no name", u.id, r.hist_fig_id);
            }
        }
        assert!(w.hist_figs.iter().all(|h| !h.name.is_empty()));
    }

    // round-8 data (exporter rev 2026-10-10e and later)
    if rep.round8 {
        assert!(!w.work_details.is_empty(), "round-8 export without work details");
        let ids: std::collections::HashSet<i32> = w.units.iter().map(|u| u.id).collect();
        let blds: std::collections::HashMap<i32, &str> = w.buildings.iter().map(|b| (b.id, b.kind.as_str())).collect();
        for d in &w.work_details {
            assert!(["Default", "EverybodyDoesThis", "NobodyDoesThis", "OnlySelectedDoesThis"].contains(&d.mode.as_str()), "{}: {}", d.name, d.mode);
            assert!(d.assigned.iter().all(|u| ids.contains(u)));
            for &u in &d.assigned {
                assert!(w.units.iter().find(|x| x.id == u).unwrap().work_details.contains(&d.name));
            }
        }
        for b in w.buildings.iter().filter(|b| b.kind == "Stockpile") {
            let s = b.stockpile.as_ref().expect("stockpile settings");
            for f in &s.filters {
                let parts: Vec<&str> = f.splitn(3, '/').collect();
                assert_eq!(parts.len(), 3, "pile {} filter {f}", b.id);
                assert!(parts[0] == "misc" || s.categories.iter().any(|c| c == parts[0]), "pile {} filter {f} of a disabled category", b.id);
            }
            // every enabled category has at least one filter string (DF always has settings)
            for c in &s.categories {
                assert!(s.filters.iter().any(|f| f.starts_with(&format!("{c}/"))), "pile {} category {c} without filters", b.id);
            }
            assert!(s.give_to.iter().chain(&s.take_from).all(|l| blds.contains_key(l)));
        }
        for l in &w.locations {
            assert!(["Temple", "Tavern", "Library", "Guildhall", "Hospital"].contains(&l.kind.as_str()) || !l.kind.is_empty(), "location {}", l.id);
            assert!(l.zones.iter().all(|z| blds.get(z) == Some(&"Civzone")), "location {} zones {:?}", l.id, l.zones);
            assert!(l.occupations.iter().all(|o| o.unit == -1 || ids.contains(&o.unit)));
        }
        assert!(w.crimes.iter().all(|c| !c.kind.is_empty() && c.year > 0));
        for u in &w.units {
            assert!(u.pet_owner == -1 || ids.contains(&u.pet_owner));
            assert!(u.pasture == -1 || blds.get(&u.pasture) == Some(&"Civzone"));
            assert!(u.pregnancy >= 0);
            if u.flags.animal {
                assert!(!u.training.is_empty(), "animal {} without training level", u.id);
            }
        }
        assert_eq!(rep.stockpile_filters as i64, m.count("stockpile_filters").unwrap_or(0));
        let x = rf_import::load_extras(&dir);
        assert!(x.rooms.iter().all(|r| r.room_value == -1 || r.location_id >= 0));
        assert!(x.animal_products.iter().any(|p| p.milkable.is_some()), "no milkable caste in the raws");
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
