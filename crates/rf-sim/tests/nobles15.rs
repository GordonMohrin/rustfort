//! [nobles15] Checks against the real reference fort (skipped without `RUSTFORT_EXPORT`).

use rf_sim::Sim;

#[test]
fn reference_mayor_requirements_and_mandates() {
    let Some(dir) = rf_import::find_export(None) else {
        println!("no export found (set RUSTFORT_EXPORT): skipped");
        return;
    };
    let (world, _) = rf_import::load_export(&dir).expect("load export");
    let mayor = world.nobles.iter().find(|n| n.token == "MAYOR" && n.holder >= 0).cloned();
    let mut sim = Sim::new(world, 1);
    sim.run_ticks(2_000);
    let Some(mayor) = mayor else {
        println!("no mayor in this export: skipped");
        return;
    };
    // Numbers come from the export (nobles.json `required_*`), not only the table.
    let reqs = sim.noble_requirements();
    let m = reqs.iter().find(|r| r.unit == mayor.holder).expect("the mayor has requirements");
    assert_eq!(m.mandate_max, 1);
    assert!(m.rooms.iter().any(|r| r.kind == "office" && r.required == 500));
    // Imported mandates keep DF's numbers.
    if !mayor.mandates_detail.is_empty() {
        let ms = sim.mandates();
        assert!(!ms.is_empty(), "the exported mandate was taken over");
        assert!(ms.iter().all(|m| (1..=5).contains(&m.strikes) || m.strikes == 0));
    }
    // H1 calibration point: the mayor's bedroom (zone 11412 in Tileshtulon).
    if let Some(v) = sim.room_value(11412) {
        println!("mayor bedroom value {v}");
    }
    let r = sim.nobles15_report();
    println!("{r:?}");
}
