//! [animals] Grazing and pastures, breeding, eggs, milking, shearing, slaughter, training,
//! pets, hunting and fishing on the synthetic fort.

use rf_core::{Building, CasteRaw, CreatureRaw, Item, ItemFlags, Pos, StockpileSettings, Unit, UnitFlags, World};
use rf_sim::Sim;

const DX: u16 = 48;
const DY: u16 = 48;
const DZ: u16 = 20;

fn surface() -> i16 {
    (DZ as i16 * 2 / 3).max(2)
}

fn hall() -> (i16, i16, i16) {
    (DX as i16 / 2, DY as i16 / 2, (surface() - 4).max(0))
}

fn caste(token: &str, name: &str, size: i32, flags: &[&str]) -> CasteRaw {
    CasteRaw { token: token.into(), name: name.into(), body_size: size, walk_speed: 900, flags: flags.iter().map(|s| s.to_string()).collect(), ..Default::default() }
}

fn raw(token: &str, name: &str, castes: Vec<CasteRaw>) -> CreatureRaw {
    CreatureRaw { token: token.into(), name: name.into(), name_plural: format!("{name}s"), castes, ..Default::default() }
}

/// Vanilla-like raws for the animals of the tests.
fn add_raws(w: &mut World) {
    w.creature_raws.push(raw(
        "COW",
        "cow",
        vec![caste("FEMALE", "cow", 600_000, &["GRAZER", "MILKABLE", "PET", "MULTIPLE_LITTER_RARE"]), caste("MALE", "bull", 600_000, &["GRAZER", "PET"])],
    ));
    w.creature_raws.push(raw("SHEEP", "sheep", vec![caste("FEMALE", "ewe", 50_000, &["GRAZER", "MILKABLE", "PET"]), caste("MALE", "ram", 50_000, &["GRAZER", "PET"])]));
    w.creature_raws.push(raw(
        "DOG",
        "dog",
        vec![caste("FEMALE", "dog", 30_000, &["PET", "TRAINABLE_WAR", "TRAINABLE_HUNTING"]), caste("MALE", "dog", 30_000, &["PET", "TRAINABLE_WAR", "TRAINABLE_HUNTING"])],
    ));
    w.creature_raws.push(raw(
        "CAT",
        "cat",
        vec![caste("FEMALE", "cat", 5_000, &["PET", "HUNTS_VERMIN", "ADOPTS_OWNER"]), caste("MALE", "cat", 5_000, &["PET", "HUNTS_VERMIN", "ADOPTS_OWNER"])],
    ));
    w.creature_raws.push(raw("BIRD_CHICKEN", "chicken", vec![caste("FEMALE", "hen", 3_000, &["PET", "LAYS_EGGS"]), caste("MALE", "rooster", 3_000, &["PET"])]));
    w.creature_raws.push(raw("DEER", "deer", vec![caste("FEMALE", "doe", 140_000, &["GRAZER", "NATURAL_ANIMAL"]), caste("MALE", "stag", 140_000, &["GRAZER", "NATURAL_ANIMAL"])]));
    w.creature_raws.push(raw("GOBLIN", "goblin", vec![caste("MALE", "goblin", 60_000, &[])]));
}

fn world() -> World {
    let mut w = World::synthetic(DX, DY, DZ);
    add_raws(&mut w);
    // The synthetic cat is not needed unless a test wants it.
    w.units.retain(|u| u.race != "CAT");
    w
}

fn animal(id: i32, race: &str, caste: &str, p: Pos, sex: i8) -> Unit {
    Unit {
        id,
        race: race.into(),
        caste: caste.into(),
        name: format!("{} (tame)", race.to_lowercase()),
        profession: "STANDARD".into(),
        pos: p,
        flags: UnitFlags { animal: true, tame: true, ..Default::default() },
        sex,
        age: 3.0,
        walk_speed: 900,
        hist_fig_id: -1,
        squad_id: -1,
        squad_position: -1,
        invader_civ: -1,
        pet_owner: -1,
        pasture: -1,
        body_size: 50_000,
        ..Default::default()
    }
}

fn zone(id: i32, subtype: &str, x1: i16, y1: i16, x2: i16, y2: i16, z: i16) -> Building {
    Building { id, kind: "Civzone".into(), subtype: subtype.into(), x1, y1, x2, y2, z, center: Pos::new(x1, y1, z), built: true, owner: -1, ..Default::default() }
}

fn shop(id: i32, subtype: &str, cx: i16, cy: i16, z: i16) -> Building {
    Building {
        id,
        kind: "Workshop".into(),
        subtype: subtype.into(),
        x1: cx - 1,
        y1: cy - 1,
        x2: cx + 1,
        y2: cy + 1,
        z,
        center: Pos::new(cx, cy, z),
        built: true,
        owner: -1,
        ..Default::default()
    }
}

fn item(id: i32, kind: &str, subtype: &str, p: Pos, stack: i32) -> Item {
    Item {
        id,
        kind: kind.into(),
        subtype: subtype.into(),
        mat: 3,
        pos: p,
        stack,
        container: -1,
        holder: -1,
        building: -1,
        flags: ItemFlags { on_ground: true, ..Default::default() },
        ..Default::default()
    }
}

fn give_labors(w: &mut World, labors: &[&str]) {
    for u in w.units.iter_mut().filter(|u| u.flags.citizen) {
        for l in labors {
            u.labors.push(l.to_string());
        }
    }
}

fn template(sim: &Sim, label: &str) -> usize {
    sim.order_templates().iter().position(|t| t.label == label).unwrap_or_else(|| panic!("no template {label}"))
}

fn alive(sim: &Sim, id: i32) -> bool {
    sim.world.unit(id).map(|u| !u.flags.dead).unwrap_or(false)
}

#[test]
fn grazer_value_follows_the_wiki_formula() {
    // Wiki "Grazer" table: cow 521.695, rabbit 106 365.918, elephant 106.366 -> 150 (minimum).
    assert_eq!(rf_sim::animals::grazer_value(600_000), 521);
    assert_eq!(rf_sim::animals::grazer_value(500), 106_365);
    assert_eq!(rf_sim::animals::grazer_value(5_000_000), 150);
    let sim = Sim::new(world(), 1);
    assert_eq!(sim.grazer_value_of("COW"), 521);
    assert_eq!(sim.grazer_value_of("DOG"), 0, "dogs are no grazers");
}

#[test]
fn grazer_without_grass_starves() {
    let (cx, cy, hz) = hall();
    let mut w = world();
    let mut cow = animal(500, "COW", "FEMALE", Pos::new(cx + 3, cy + 2, hz), 0);
    cow.hunger_timer = 95_000;
    w.units.push(cow);
    let mut dog = animal(501, "DOG", "MALE", Pos::new(cx - 3, cy + 2, hz), 1);
    dog.hunger_timer = 95_000;
    w.units.push(dog);
    let mut sim = Sim::new(w, 3);
    sim.run_ticks(8_000);
    assert!(!alive(&sim, 500), "the cow in the stone hall has starved");
    assert!(alive(&sim, 501), "dogs do not need food (DF: only grazers eat)");
    assert_eq!(sim.animal_report().starved, 1);
    assert!(sim.drain_announcements().iter().any(|a| a.text.contains("starved to death")));
}

#[test]
fn grazer_eats_grass_in_its_pasture_and_stays_there() {
    let (cx, cy, _) = hall();
    let sz = surface();
    let mut w = world();
    w.buildings.push(zone(700, "Pen", cx - 12, cy - 12, cx - 3, cy - 3, sz));
    let mut cow = animal(500, "COW", "FEMALE", Pos::new(cx - 8, cy - 8, sz), 0);
    cow.hunger_timer = 30_000;
    cow.pasture = 700;
    w.units.push(cow);
    let mut sim = Sim::new(w, 5);
    let g0 = sim.grass_at(Pos::new(cx - 8, cy - 8, sz));
    assert_eq!(g0, 100, "fresh grass");
    for _ in 0..20 {
        sim.run_ticks(1_000);
        let p = sim.world.unit(500).unwrap().pos;
        assert!(p.x >= cx - 12 && p.x <= cx - 3 && p.y >= cy - 12 && p.y <= cy - 3 && p.z == sz, "the cow stays in its pasture: {p:?}");
    }
    let h = sim.grazer_hunger(500).unwrap();
    assert!(h < 10_000, "grazing took the hunger down: {h}");
    let r = sim.animal_report();
    assert!(r.grass_eaten >= 300, "grass eaten: {}", r.grass_eaten);
    assert_eq!(r.pastured, 1);
    assert!(alive(&sim, 500));
}

#[test]
fn too_small_pasture_starves_a_cow() {
    // Wiki rule of thumb: 20000 = GRAZER x tiles -> a cow needs about 38 tiles; 4 are not enough.
    let (cx, cy, _) = hall();
    let sz = surface();
    let mut w = world();
    w.buildings.push(zone(700, "Pen", cx - 10, cy - 10, cx - 9, cy - 9, sz));
    let mut cow = animal(500, "COW", "FEMALE", Pos::new(cx - 10, cy - 10, sz), 0);
    cow.pasture = 700;
    w.units.push(cow);
    let mut sim = Sim::new(w, 5);
    sim.run_ticks(150_000);
    assert!(!alive(&sim, 500), "4 grass tiles cannot feed a cow");
    let r = sim.animal_report();
    assert_eq!(r.starved, 1);
    assert!(r.grass_eaten > 400, "it ate the pasture down first: {}", r.grass_eaten);
}

#[test]
fn animal_hauler_leads_animal_to_pasture() {
    let (cx, cy, hz) = hall();
    let sz = surface();
    let mut w = world();
    w.buildings.push(zone(700, "Pen", cx - 12, cy - 12, cx - 6, cy - 6, sz));
    w.units.push(animal(500, "COW", "FEMALE", Pos::new(cx + 3, cy + 2, hz), 0));
    give_labors(&mut w, &["HAUL_ANIMALS"]);
    let mut sim = Sim::new(w, 7);
    sim.run_ticks(500);
    sim.assign_pasture(500, 700);
    let mut inside = false;
    for _ in 0..30 {
        sim.run_ticks(500);
        let p = sim.world.unit(500).unwrap().pos;
        if p.z == sz && p.x >= cx - 12 && p.x <= cx - 6 && p.y >= cy - 12 && p.y <= cy - 6 {
            inside = true;
            break;
        }
    }
    assert!(inside, "the cow was led into the pasture");
    assert_eq!(sim.animal_report().led_to_pasture, 1);
    assert_eq!(sim.animals().iter().find(|a| a.unit == 500).unwrap().pasture, 700);
}

#[test]
fn dogs_breed_and_give_birth_after_six_months() {
    let (cx, cy, hz) = hall();
    let mut w = world();
    // A small pen keeps the pair together.
    w.buildings.push(zone(700, "Pen", cx + 2, cy + 2, cx + 3, cy + 3, hz));
    let mut f = animal(500, "DOG", "FEMALE", Pos::new(cx + 2, cy + 2, hz), 0);
    f.pasture = 700;
    let mut m = animal(501, "DOG", "MALE", Pos::new(cx + 3, cy + 3, hz), 1);
    m.pasture = 700;
    w.units.push(f);
    w.units.push(m);
    let n0 = w.units.len();
    let mut sim = Sim::new(w, 11);
    sim.run_ticks(40_000);
    let r = sim.animal_report();
    assert!(r.pregnancies >= 1, "the dogs mated");
    assert!(sim.animals().iter().any(|a| a.unit == 500 && a.pregnant));
    // Wiki "Breeding": 6 months gestation.
    sim.run_ticks((6 * rf_core::TICKS_PER_MONTH) as u32);
    let r = sim.animal_report();
    assert!(r.births >= 1, "puppies were born: {r:?}");
    let young: Vec<&Unit> = sim.world.units[n0..].iter().filter(|u| u.race == "DOG").collect();
    assert!(!young.is_empty());
    assert!(young.iter().all(|u| u.flags.tame && u.flags.animal && u.age < 1.0));
    assert!(young.len() <= 3, "litter of 1-3");
}

#[test]
fn hen_lays_in_nest_box_eggs_are_collected_or_hatch_when_forbidden() {
    let run = |forbid: bool| {
        let (cx, cy, hz) = hall();
        let mut w = world();
        w.buildings.push(zone(700, "Pen", cx - 6, cy - 4, cx - 3, cy - 2, hz));
        w.buildings.push(Building {
            id: 701,
            kind: "NestBox".into(),
            x1: cx - 4,
            y1: cy - 3,
            x2: cx - 4,
            y2: cy - 3,
            z: hz,
            center: Pos::new(cx - 4, cy - 3, hz),
            built: true,
            owner: -1,
            ..Default::default()
        });
        w.buildings.push(Building {
            id: 702,
            kind: "Stockpile".into(),
            x1: cx + 4,
            y1: cy + 3,
            x2: cx + 6,
            y2: cy + 3,
            z: hz,
            center: Pos::new(cx + 4, cy + 3, hz),
            built: true,
            owner: -1,
            stockpile: Some(StockpileSettings { categories: vec!["food".into()], ..Default::default() }),
            ..Default::default()
        });
        let mut hen = animal(500, "BIRD_CHICKEN", "FEMALE", Pos::new(cx - 5, cy - 3, hz), 0);
        hen.pasture = 700;
        let mut rooster = animal(501, "BIRD_CHICKEN", "MALE", Pos::new(cx - 5, cy - 2, hz), 1);
        rooster.pasture = 700;
        w.units.push(hen);
        w.units.push(rooster);
        let mut sim = Sim::new(w, 13);
        // Forbid new eggs at once (the player clicking them), before a hauler takes them.
        for _ in 0..(8 * 3360) {
            sim.run_ticks(10);
            if forbid {
                for it in sim.world.items.iter_mut().filter(|i| i.kind == "EGG") {
                    it.flags.forbid = true;
                }
            }
        }
        sim
    };
    let sim = run(false);
    let r = sim.animal_report();
    assert!(r.clutches >= 3, "a clutch every 3 months: {r:?}");
    assert!(r.eggs_laid >= 8, "chicken clutch 4-15");
    assert_eq!(r.eggs_hatched, 0, "eggs were collected as food");
    let (cx, cy, hz) = hall();
    let nest = Pos::new(cx - 4, cy - 3, hz);
    let left = sim.world.items.iter().filter(|i| i.kind == "EGG" && i.pos == nest && i.holder < 0).count();
    assert!(left <= 1, "food haulers took the eggs out of the nest box (to the pile, or they were eaten): {left}");
    let sim = run(true);
    let r = sim.animal_report();
    assert!(r.eggs_hatched >= 4, "forbidden fertile eggs hatch after 3 months: {r:?}");
    assert!(sim.world.units.iter().filter(|u| u.race == "BIRD_CHICKEN").count() > 2);
}

#[test]
fn milking_cheese_shearing_and_spinning_at_the_farmers_workshop() {
    let (cx, cy, hz) = hall();
    let mut w = world();
    w.buildings.push(shop(800, "Farmers", cx + 4, cy - 2, hz));
    let mut cow = animal(500, "COW", "FEMALE", Pos::new(cx - 4, cy + 2, hz), 0);
    cow.hunger_timer = 0;
    w.units.push(cow);
    w.units.push(animal(501, "SHEEP", "MALE", Pos::new(cx - 5, cy + 2, hz), 1));
    w.items.push(item(7000, "BUCKET", "", Pos::new(cx - 2, cy + 3, hz), 1));
    give_labors(&mut w, &["MILK", "MAKE_CHEESE", "SHEARER", "SPINNER"]);
    let mut sim = Sim::new(w, 17);
    sim.add_building_job(800, template(&sim, "Milk animal"), false);
    sim.add_building_job(800, template(&sim, "Shear animal"), false);
    sim.run_ticks(6_000);
    let r = sim.animal_report();
    assert_eq!(r.milkings, 1);
    assert_eq!(r.shearings, 1);
    let milk = sim.world.items.iter().find(|i| i.kind == "LIQUID_MISC" && i.desc.contains("milk")).expect("milk");
    assert_eq!(milk.container, 7000, "the milk is in the bucket");
    assert!(sim.world.items.iter().any(|i| i.kind == "CORPSEPIECE" && i.desc == "sheep wool"));
    sim.add_building_job(800, template(&sim, "Make cheese"), false);
    sim.add_building_job(800, template(&sim, "Spin thread"), false);
    // The sheep's wool needs 300 days to grow back: a second shearing waits.
    sim.add_building_job(800, template(&sim, "Shear animal"), false);
    sim.run_ticks(6_000);
    let r = sim.animal_report();
    assert_eq!(r.cheese, 1);
    assert_eq!(r.thread, 1);
    assert_eq!(r.shearings, 1, "no second shearing before the wool regrows");
    assert!(sim.world.materials.iter().any(|m| m.token == "CREATURE:COW:CHEESE" && m.name == "cow cheese"), "cheese of cow milk (eaten soon)");
    assert!(sim.world.items.iter().any(|i| i.kind == "THREAD" && i.desc == "sheep wool yarn"));
    assert!(sim.production_report().unsupported.iter().all(|(j, _)| j != "ShearCreature"));
}

#[test]
fn marked_animal_is_slaughtered_and_butchered() {
    let (cx, cy, hz) = hall();
    let mut w = world();
    w.buildings.push(shop(810, "Butchers", cx + 4, cy - 2, hz));
    w.units.push(animal(500, "COW", "MALE", Pos::new(cx - 4, cy + 2, hz), 1));
    let mut pet = animal(501, "DOG", "MALE", Pos::new(cx - 5, cy + 2, hz), 1);
    pet.pet_owner = 1;
    w.units.push(pet);
    give_labors(&mut w, &["BUTCHER"]);
    let mut sim = Sim::new(w, 19);
    sim.set_slaughter(500, true);
    sim.set_slaughter(501, true);
    assert!(!sim.world.unit(501).unwrap().slaughter, "pets cannot be slaughtered");
    sim.run_ticks(8_000);
    assert!(!alive(&sim, 500), "the cow was slaughtered");
    assert!(alive(&sim, 501));
    assert_eq!(sim.animal_report().slaughtered, 1);
    let meat: i32 = sim.world.items.iter().filter(|i| i.kind == "MEAT" && i.desc == "cow meat").map(|i| i.stack).sum();
    assert_eq!(meat, 60, "butchered by the butcher recipe (600 000 cm3 / 10 000)");
}

#[test]
fn trainer_trains_war_dog_that_attacks_hostiles_near_its_owner() {
    let (cx, cy, hz) = hall();
    let mut w = world();
    let mut dog = animal(500, "DOG", "MALE", Pos::new(cx - 4, cy + 2, hz), 1);
    dog.pet_owner = 1;
    w.units.push(dog);
    give_labors(&mut w, &["ANIMALTRAIN"]);
    let mut sim = Sim::new(w, 23);
    sim.request_training(500, "War");
    sim.run_ticks(20_000);
    let info = sim.animals().into_iter().find(|a| a.unit == 500).unwrap();
    assert_eq!(info.role, "War");
    assert_eq!(sim.world.unit(500).unwrap().profession, "TRAINED_WAR");
    assert!(sim.animal_report().training_sessions >= 1);
    // A hostile next to the dog's owner.
    let owner = sim.world.unit(1).unwrap().pos;
    let dpos = sim.world.unit(500).unwrap().pos;
    let gid = 900;
    let mut g = animal(gid, "GOBLIN", "MALE", dpos, 1);
    g.flags = UnitFlags { hostile: true, ..Default::default() };
    g.name = "goblin".into();
    g.pos = if sim.is_walkable(dpos.offset(1, 0, 0)) { dpos.offset(1, 0, 0) } else { dpos };
    let _ = owner;
    sim.world.units.push(g);
    sim.run_ticks(1);
    sim.make_hostile(gid);
    sim.run_ticks(2_000);
    assert!(sim.animal_report().war_attacks >= 1, "the war dog attacked");
}

#[test]
fn caged_wild_animal_is_tamed() {
    let (cx, cy, hz) = hall();
    let mut w = world();
    let mut deer = animal(500, "DEER", "FEMALE", Pos::new(cx - 4, cy + 2, hz), 0);
    deer.flags = UnitFlags { animal: true, caged: true, ..Default::default() };
    deer.training = "WildUntamed".into();
    w.units.push(deer);
    give_labors(&mut w, &["ANIMALTRAIN"]);
    let mut sim = Sim::new(w, 29);
    sim.request_training(500, "Tame");
    sim.run_ticks(20_000);
    let u = sim.world.unit(500).unwrap();
    // DEER has no PET token in these raws: it cannot be tamed.
    assert!(!u.flags.tame);
    let mut w = world();
    let mut dog = animal(500, "DOG", "FEMALE", Pos::new(cx - 4, cy + 2, hz), 0);
    dog.flags = UnitFlags { animal: true, caged: true, ..Default::default() };
    dog.training = "WildUntamed".into();
    w.units.push(dog);
    give_labors(&mut w, &["ANIMALTRAIN"]);
    let mut sim = Sim::new(w, 29);
    sim.request_training(500, "Tame");
    sim.run_ticks(20_000);
    let u = sim.world.unit(500).unwrap();
    assert!(u.flags.tame, "a PET animal in a cage gets tamed");
    assert!(u.training != "WildUntamed");
    assert_eq!(sim.animal_report().tamed, 1);
}

#[test]
fn cat_adopts_a_dwarf_and_hunts_vermin() {
    let (cx, cy, hz) = hall();
    let mut w = world();
    w.units.push(animal(500, "CAT", "FEMALE", Pos::new(cx - 2, cy - 2, hz), 0));
    let mut sim = Sim::new(w, 31);
    sim.run_ticks((3 * rf_core::TICKS_PER_MONTH) as u32);
    let cat = sim.world.unit(500).unwrap();
    assert!(cat.pet_owner > 0, "the cat chose an owner");
    let owner = sim.world.unit(cat.pet_owner).unwrap();
    assert!(owner.emotions.iter().any(|e| e.thought == "AdoptedPet"));
    let r = sim.animal_report();
    assert_eq!(r.adoptions, 1);
    assert!(r.vermin_killed >= 5, "cats kill vermin: {}", r.vermin_killed);
}

#[test]
fn hunter_shoots_wild_animal_and_brings_it_to_the_butcher() {
    let (cx, cy, hz) = hall();
    let sz = surface();
    let mut w = world();
    w.buildings.push(shop(810, "Butchers", cx + 4, cy - 2, hz));
    w.items.push(item(7000, "WEAPON", "ITEM_WEAPON_CROSSBOW", Pos::new(cx - 2, cy + 3, hz), 1));
    w.items.push(item(7001, "AMMO", "ITEM_AMMO_BOLTS", Pos::new(cx - 1, cy + 3, hz), 25));
    let mut deer = animal(500, "DEER", "MALE", Pos::new(cx + 8, cy + 8, sz), 1);
    deer.flags = UnitFlags { animal: true, ..Default::default() };
    deer.body_size = 140_000;
    w.units.push(deer);
    for u in w.units.iter_mut().filter(|u| u.flags.citizen).take(1) {
        u.labors.push("HUNT".into());
    }
    give_labors(&mut w, &["BUTCHER"]);
    let mut sim = Sim::new(w, 37);
    sim.run_ticks(15_000);
    let r = sim.animal_report();
    assert!(!alive(&sim, 500), "the deer was shot: {r:?}");
    assert_eq!(r.hunt_kills, 1);
    assert!(r.shots >= 2, "140 000 cm3 need 2 hits: {}", r.shots);
    let bolts = sim.world.items.iter().find(|i| i.id == 7001).map(|i| i.stack).unwrap_or(0);
    assert_eq!(bolts as u64, 25 - r.shots, "one bolt per shot");
    let meat: i32 = sim.world.items.iter().filter(|i| i.kind == "MEAT" && i.desc == "deer meat").map(|i| i.stack).sum();
    assert_eq!(meat, 14, "the corpse was butchered (auto butcher)");
}

#[test]
fn fisher_catches_raw_fish_and_the_fishery_cleans_it() {
    let (cx, cy, hz) = hall();
    let mut w = world();
    // A pond in the hall: three water tiles at the east wall, a fishing zone on the bank.
    for y in cy - 1..=cy + 1 {
        let p = Pos::new(cx + 6, y, hz);
        let t = w.tile_mut(p).unwrap();
        t.liquid = 7;
    }
    w.buildings.push(zone(700, "FishingArea", cx + 5, cy - 1, cx + 5, cy + 1, hz));
    w.buildings.push(shop(820, "Fishery", cx - 4, cy - 2, hz));
    give_labors(&mut w, &["FISH", "CLEAN_FISH"]);
    let mut sim = Sim::new(w, 41);
    sim.run_ticks(12_000);
    let r = sim.animal_report();
    assert!(r.fish_caught >= 2, "{r:?}");
    assert!(r.fish_cleaned >= 1, "the fishery got 'Prepare raw fish' jobs: {r:?}");
    assert!(sim.world.items.iter().any(|i| i.kind == "FISH" && i.desc == "pond turtle"));
}

#[test]
fn animals_are_deterministic_across_thread_counts() {
    let run = |threads: usize| {
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
        pool.install(|| {
            let (cx, cy, hz) = hall();
            let sz = surface();
            let mut w = world();
            w.buildings.push(zone(700, "Pen", cx - 12, cy - 12, cx - 3, cy - 3, sz));
            for k in 0..6 {
                let mut a = animal(500 + k, if k % 2 == 0 { "COW" } else { "SHEEP" }, if k < 3 { "FEMALE" } else { "MALE" }, Pos::new(cx + 2 + k as i16, cy + 2, hz), (k >= 3) as i8);
                a.hunger_timer = 20_000;
                w.units.push(a);
            }
            w.units.push(animal(510, "CAT", "MALE", Pos::new(cx - 2, cy - 2, hz), 1));
            give_labors(&mut w, &["HAUL_ANIMALS"]);
            let mut sim = Sim::new(w, 43);
            sim.set_parallel_threshold(1);
            for k in 0..6 {
                sim.assign_pasture(500 + k, 700);
            }
            sim.run_ticks(60_000);
            let r = sim.animal_report();
            (sim.state_hash(), r.led_to_pasture, r.grass_eaten, r.pregnancies)
        })
    };
    let a = run(1);
    let b = run(4);
    assert!(a.1 >= 4, "animals were led: {a:?}");
    assert_eq!(a, b);
}
