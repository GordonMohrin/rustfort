//! [weather13] Tests: temperature by season, freezing/thawing, snow, rain refilling pools, evil weather.

use super::*;
use crate::locations::tests::world_with_zone;
use rf_core::{Shape as S, TiletypeInfo, TiletypeTable, TtMaterial as M, TtSpecial as P, World, TICKS_PER_MONTH};

fn surface_z() -> i16 {
    20i16 * 2 / 3
}

/// Synthetic world with weather on, ice and pool tiletypes, at the start of `month`.
fn world(month: i32) -> World {
    let mut w = world_with_zone("Halle");
    w.settings.weather = true;
    let mut infos = w.tiletypes.infos.clone();
    let mut id = infos.iter().map(|i| i.id).max().unwrap() + 1;
    for (n, s, m) in [("FrozenWall", S::Wall, M::FrozenLiquid), ("FrozenFloor1", S::Floor, M::FrozenLiquid), ("MurkyPool", S::Floor, M::Pool)] {
        infos.push(TiletypeInfo { id, name: n.into(), shape: s, material: m, special: P::Normal, variant: 0, direction: String::new() });
        id += 1;
    }
    w.tiletypes = TiletypeTable::new(infos);
    w.calendar.year_tick = month * TICKS_PER_MONTH;
    w
}

/// Dig an outdoor pit one level below the surface at (x, y) and fill it with water.
fn pit(w: &mut World, x: i16, y: i16, depth: u8, tt_name: &str) -> Pos {
    let p = Pos::new(x, y, surface_z() - 1);
    let tt = w.tiletypes.by_name(tt_name).unwrap();
    let open = w.tiletypes.by_name("OpenSpace").unwrap();
    {
        let t = w.tile_mut(p).unwrap();
        t.tt = tt;
        t.flags = tile_flags::OUTSIDE | tile_flags::LIGHT;
        t.liquid = depth;
    }
    let above = w.tile_mut(Pos::new(x, y, surface_z())).unwrap();
    above.tt = open;
    above.flags = tile_flags::OUTSIDE | tile_flags::LIGHT;
    p
}

#[test]
fn temperature_follows_the_seasons() {
    let mut sim = Sim::new(world(0), 1);
    let at = |sim: &mut Sim, m: i32| {
        sim.world.calendar.year_tick = m * TICKS_PER_MONTH;
        sim.surface_temperature()
    };
    assert!(at(&mut sim, 0) >= FREEZING, "early spring thawed");
    assert!(at(&mut sim, 4) > at(&mut sim, 1), "mid summer warmest");
    assert!(at(&mut sim, 9) < FREEZING, "early winter freezes");
    assert!(at(&mut sim, 10) < at(&mut sim, 9));
    // Underground keeps the mean; magma is hot.
    let deep = Pos::new(3, 3, 2);
    assert_eq!(sim.temperature_at(deep), DEFAULT_MEAN);
    sim.world.climate.temperature = 10_090; // hot desert
    sim.world.calendar.year_tick = 10 * TICKS_PER_MONTH;
    assert!(sim.weather_summary().temperature >= FREEZING);
    assert_eq!(Sim::urist_to_celsius(sim.weather_summary().temperature), ((10_090 - 30 - 10_000) * 5) / 9);
}

#[test]
fn water_freezes_in_winter_kills_swimmers_and_thaws_in_spring() {
    let mut w = world(9); // early winter
    let deep = pit(&mut w, 5, 5, 7, "SoilFloor1");
    let shallow = pit(&mut w, 40, 40, 2, "SoilFloor1");
    let victim = w.units[1].id;
    w.units[1].pos = deep;
    let mut sim = Sim::new(w, 3);
    sim.run_ticks(100);
    let info = sim.weather_summary();
    assert!(info.frozen);
    let rep = sim.weather_report();
    assert_eq!(rep.freezes, 1);
    assert_eq!(rep.frozen_deaths, 1);
    assert_eq!(sim.ai[sim.unit_slot[&victim]].kind, Kind::Dead);
    assert_eq!(sim.liquid_at(deep).0, 0);
    assert_eq!(sim.world.shape(deep), S::Wall, "deep water becomes an ice wall");
    assert_eq!(sim.world.shape(shallow), S::Floor, "shallow water becomes an ice floor");
    assert_eq!(sim.world.tiletypes.get(sim.world.tile(shallow).unwrap().tt).material, M::FrozenLiquid);
    // Spring.
    sim.world.calendar.year_tick = 0;
    sim.run_ticks(100);
    assert!(!sim.weather_summary().frozen);
    assert_eq!(sim.weather_report().thaws, 1);
    assert_eq!(sim.liquid_at(deep).0, 7);
    assert_eq!(sim.world.shape(deep), S::Floor);
    assert_eq!(sim.world.tiletypes.get(sim.world.tile(shallow).unwrap().tt).material, M::Soil);
}

#[test]
fn freezing_regions_get_snow_instead_of_rain() {
    let mut w = world(3);
    w.climate.temperature = 9_950;
    w.climate.temperature_swing = 20;
    let mut sim = Sim::new(w, 5);
    sim.run_ticks(30 * 1200);
    let rep = sim.weather_report();
    assert!(rep.snow_ticks > 0, "{rep:?}");
    assert_eq!(rep.rain_ticks, 0);
    assert_eq!(sim.lifestyle_report().rain_days, 0, "snow is no rain for the thoughts");
    assert!(sim.weather_summary().snow_cover);
    assert!(sim.snow_at(Pos::new(2, 2, surface_z())));
    assert!(!sim.snow_at(Pos::new(2, 2, 2)), "no snow underground");
}

#[test]
fn temperate_summer_rains_and_rain_refills_murky_pools() {
    let mut w = world(3);
    let pool = pit(&mut w, 6, 6, 1, "MurkyPool");
    let mut sim = Sim::new(w, 7);
    sim.run_ticks(30 * 1200);
    let rep = sim.weather_report();
    assert!(rep.rain_ticks > 0 && rep.snow_ticks == 0, "{rep:?}");
    assert!(sim.lifestyle_report().rain_days > 0);
    assert!(rep.pool_refills > 0, "{rep:?}");
    assert!(sim.liquid_at(pool).0 > 1 || sim.fluid_report().evaporated > 0);
    // Forced rain over the surface only.
    sim.force_weather(WeatherKind::Rain, 500);
    sim.run_ticks(100);
    assert_eq!(sim.weather_at(Pos::new(2, 2, surface_z())), WeatherKind::Rain);
    assert_eq!(sim.weather_at(Pos::new(2, 2, 2)), WeatherKind::Clear);
}

#[test]
fn evil_weather_comes_to_evil_regions() {
    let mut w = world(3);
    w.climate.evil = true;
    w.climate.evil_weather = vec!["blood rain".into(), "cloud of husking dust".into()];
    let mut sim = Sim::new(w, 9);
    let mut seen = Vec::new();
    for _ in 0..90 * 12 {
        sim.run_ticks(100);
        if let Some(e) = sim.evil_weather() {
            if !seen.contains(&e.effect) {
                seen.push(e.effect.clone());
            }
        }
    }
    assert!(sim.weather_report().evil_ticks > 0);
    assert!(!seen.is_empty());
    assert!(seen.iter().all(|e| e == "BLOOD" || e == "HUSK"), "{seen:?}");
}

#[test]
fn forced_evil_cloud_covers_only_its_radius() {
    let mut sim = Sim::new(world(3), 11);
    let e = EvilWeather { kind: WeatherKind::EvilCloud, name: "thrall mist".into(), effect: "THRALL".into(), syndrome: String::new(), center: Some(Pos::new(10, 10, surface_z())), radius: 3 };
    sim.force_evil_weather(e, 1000);
    sim.run_ticks(100);
    let ev = sim.evil_weather().expect("cloud");
    let c = ev.center.unwrap();
    assert_eq!(sim.weather_at(Pos::new(c.x, c.y, surface_z())), WeatherKind::EvilCloud);
    assert_ne!(sim.weather_at(Pos::new(45, 45, surface_z())), WeatherKind::EvilCloud);
    sim.run_ticks(1000);
    assert!(sim.evil_weather().is_none(), "the forced cloud passes");
}

#[test]
fn disabled_weather_does_nothing() {
    let mut w = world(9);
    let deep = pit(&mut w, 5, 5, 7, "SoilFloor1");
    let mut sim = Sim::new(w, 13);
    sim.set_weather_enabled(false);
    sim.run_ticks(5 * 1200);
    assert!(!sim.weather_summary().frozen);
    assert_eq!(sim.liquid_at(deep).0, 7);
    let rep = sim.weather_report();
    assert_eq!(rep.freezes + rep.rain_ticks + rep.snow_ticks, 0);
    let mut n = 0;
    sim.weather_hash(&mut |_| n += 1);
    assert_eq!(n, 0, "inactive weather mixes nothing into the hash");
}
