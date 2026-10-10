//! [app10] Round-10 screens and glue.
//!
//! - **Tasks** (DF `D_JOBLIST`, Steam "Tasks"): every current job (designations, detailing,
//!   constructions, workshop jobs, hauling, medical care, soldiers, personal needs) with the
//!   assigned worker and the place. A click jumps there; workshop jobs can be suspended,
//!   done next ("do now") or cancelled; designations and planned constructions can be
//!   cancelled.
//! - **Hauling** (DF `D_HAULING`): the hauling routes of the imported fort with their stops,
//!   conditions and linked stockpiles ([app12]: now an editor, see `round12`).
//! - **Build hotkey chains** from the player's `interface.txt`: `b`, then the category
//!   (`HOTKEY_BUILDING_WORKSHOP` ...), then the building (`HOTKEY_BUILDING_WORKSHOP_MASON` ...),
//!   with DF's workshop sub-groups (farming, clothing/leather, furnaces).
//! - **Map context menu** on right-click: select a creature, open a building, cancel a
//!   designation, smoothing/engraving or planned construction, look at the tile.
//! - **Look lines** for a tile: rough, smoothed or constructed surface, the engraving with its
//!   quality, engraver and value, and items grouped ("12 x granite boulder").
//! - **Crash guard**: a periodic in-memory snapshot of the world; when the simulation or the
//!   interface panics, a dialog shows the error and saves the last good state.
//! - **Title-screen announcement settings** (the in-game page is `round9::draw_ann_settings`).

use super::building::building_title;
use super::panels::{item_label, unit_name};
use super::round8::{frame, scroll, tabs, tabs_x};
use super::round9::{AnnFlags, Src, ANN_TYPES};
use super::{checkbox, clip, col, list_row, panel, text_button, BtnState, UiCtx};
use crate::font::{Font, GH, GW};
use crate::game::{Game, Screen, Selected, Submenu, Tool};
use macroquad::prelude::*;
use rf_core::{Pos, World};
use rf_sim::{ConstructionKind, DesignationKind, DetailKind};

// ====================================================================== state

#[derive(Default)]
pub struct R10Ui {
    // ---- tasks
    pub tasks: Vec<TaskRow>,
    /// Real time and sim tick of the last rebuild of `tasks`.
    pub tasks_at: f64,
    pub tasks_dirty: bool,
    /// Citizens with nothing to do (shown in the header).
    pub idle: usize,
    pub task_tab: usize,
    pub task_scroll: usize,
    pub task_unassigned: bool,
    // ---- hauling
    pub haul_sel: Option<i32>,
    pub haul_scroll: usize,
    // ---- build hotkey chain: the path of DF menu groups (`[]` = top of the build menu)
    pub bpath: Vec<&'static str>,
    /// Build tab when the path was last set (a clicked tab resets the path).
    pub bcat_seen: Option<usize>,
    // ---- map context menu
    pub ctx: Option<Ctx>,
    // ---- crash guard
    pub crash: Option<Crash>,
    pub snap: Option<Snap>,
    /// Test hook (`--ui crash:ui`): panic once in the next interface frame.
    pub test_panic: bool,
}

// ====================================================================== tasks

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskCat {
    Designation,
    Building,
    Workshop,
    Hauling,
    Medical,
    Military,
    Personal,
    Other,
}

/// What a task row refers to (decides the buttons).
#[derive(Clone, Debug, PartialEq)]
pub enum TaskRef {
    /// Dig / chop / gather designation.
    Designation,
    /// Smooth / engrave / carve designation.
    Detail,
    /// Planned construction (wall, floor, ...).
    Construction,
    /// Other construction-type job (deconstruction): no cancel API.
    ConstructionOther,
    /// Carry an item marked for dumping to the garbage dump (cancel = unmark the item).
    Dump,
    /// A building waiting to be built.
    Unbuilt(i32),
    /// Workshop queue entry `k` of building `bid`.
    Shop { bid: i32, k: usize },
    /// What a unit is doing that is not one of the jobs above.
    Unit,
}

#[derive(Clone, Debug)]
pub struct TaskRow {
    pub cat: TaskCat,
    pub label: String,
    pub worker: Option<i32>,
    pub pos: Pos,
    pub what: TaskRef,
    pub suspended: bool,
    /// Where (building name), if not just the tile.
    pub place: String,
}

/// Tabs of the Tasks screen: label and the category (None = all).
pub const TASK_TABS: &[(&str, Option<TaskCat>)] = &[
    ("All", None),
    ("Designations", Some(TaskCat::Designation)),
    ("Building", Some(TaskCat::Building)),
    ("Workshops", Some(TaskCat::Workshop)),
    ("Hauling", Some(TaskCat::Hauling)),
    ("Medical", Some(TaskCat::Medical)),
    ("Military", Some(TaskCat::Military)),
    ("Personal", Some(TaskCat::Personal)),
    ("Other", Some(TaskCat::Other)),
];

/// Category of a unit activity (`Sim::unit_status`) that is not a listed job.
pub fn activity_cat(a: &str) -> TaskCat {
    match a {
        "Store Item in Stockpile" | "Bring corpse to butcher" | "Dump Item" => TaskCat::Hauling,
        "Diagnose Patient" | "Suture" | "Dress Wound" | "Set Bone" | "Surgery" | "Give Water" | "Give Food" | "Recover Wounded" | "Rest" => TaskCat::Medical,
        "Fight" | "Train" | "Station" | "Patrol" | "Soldier" | "Attack" | "Lurk" => TaskCat::Military,
        "Eat" | "Drink" | "Sleep" | "Flee" | "Go to Burrow" | "Unconscious" | "Leave" => TaskCat::Personal,
        "Construct Building" => TaskCat::Building,
        a if a.starts_with("Construct ") || a.starts_with("Remove Construction") => TaskCat::Building,
        _ => TaskCat::Other,
    }
}

fn construction_ref(caption: &str) -> (TaskCat, TaskRef) {
    if caption.starts_with("Smooth") || caption.starts_with("Detail") || caption.starts_with("Carve") || caption.starts_with("Engrave") {
        (TaskCat::Designation, TaskRef::Detail)
    } else if caption.starts_with("Construct ") {
        (TaskCat::Building, TaskRef::Construction)
    } else if caption.contains("Dump") {
        (TaskCat::Hauling, TaskRef::Dump)
    } else {
        (TaskCat::Building, TaskRef::ConstructionOther)
    }
}

fn inside(b: &rf_core::Building, p: Pos) -> bool {
    p.z == b.z && p.x >= b.x1.min(b.x2) && p.x <= b.x1.max(b.x2) && p.y >= b.y1.min(b.y2) && p.y <= b.y1.max(b.y2)
}

/// All current tasks of the fort, in DF's order: designations, constructions, buildings,
/// workshop queues, then what the citizens are doing otherwise. Also returns the idle count.
pub fn collect_tasks(g: &Game) -> (Vec<TaskRow>, usize) {
    let w = &g.sim.world;
    let mut rows: Vec<TaskRow> = Vec::new();
    let mut listed: Vec<i32> = Vec::new();
    for (p, cap, who) in g.sim.jobs() {
        rows.push(TaskRow { cat: TaskCat::Designation, label: cap.to_string(), worker: who, pos: p, what: TaskRef::Designation, suspended: false, place: String::new() });
        listed.extend(who);
    }
    for (p, cap, who) in g.sim.construction_jobs() {
        let (cat, what) = construction_ref(cap);
        rows.push(TaskRow { cat, label: cap.to_string(), worker: who, pos: p, what, suspended: false, place: String::new() });
        listed.extend(who);
    }
    // What every citizen does right now (one status per unit).
    let status: Vec<(i32, String, Option<Pos>)> = w
        .citizens()
        .filter(|u| !u.flags.dead)
        .map(|u| {
            let st = g.sim.unit_status(u.id);
            (u.id, st.activity, st.job_pos)
        })
        .collect();
    let templates = g.sim.order_templates();
    for b in &w.buildings {
        let shop = matches!(b.kind.as_str(), "Workshop" | "Furnace");
        let unbuilt = !b.built && !matches!(b.kind.as_str(), "Construction" | "Civzone" | "Stockpile");
        if !shop && !unbuilt {
            continue;
        }
        let title = building_title(g, b);
        // Units whose current job target is inside this building (not eating or sleeping).
        let workers: Vec<&(i32, String, Option<Pos>)> = status
            .iter()
            .filter(|(id, a, jp)| jp.map(|p| inside(b, p)).unwrap_or(false) && activity_cat(a) != TaskCat::Personal && !listed.contains(id))
            .collect();
        if unbuilt {
            let who = workers.iter().find(|(_, a, _)| a == "Construct Building").map(|w| w.0);
            rows.push(TaskRow { cat: TaskCat::Building, label: format!("Construct {title}"), worker: who, pos: b.center, what: TaskRef::Unbuilt(b.id), suspended: false, place: title.clone() });
            listed.extend(who);
            continue;
        }
        let texts: Vec<String> = b.jobs.iter().map(|j| super::building::job_text(&templates, j)).collect();
        // The worker's activity names the job (recipe label); else the first active job.
        let mut taken: Vec<usize> = Vec::new();
        for (wid, act, _) in &workers {
            let k = texts.iter().enumerate().position(|(k, t)| !taken.contains(&k) && !b.jobs[k].suspended && t.eq_ignore_ascii_case(act)).or_else(|| (0..b.jobs.len()).find(|&k| !taken.contains(&k) && !b.jobs[k].suspended));
            if let Some(k) = k {
                taken.push(k);
                rows.push(TaskRow {
                    cat: TaskCat::Workshop,
                    label: texts[k].clone(),
                    worker: Some(*wid),
                    pos: b.center,
                    what: TaskRef::Shop { bid: b.id, k },
                    suspended: false,
                    place: title.clone(),
                });
                listed.push(*wid);
            }
        }
        for (k, j) in b.jobs.iter().enumerate() {
            if taken.contains(&k) {
                continue;
            }
            rows.push(TaskRow { cat: TaskCat::Workshop, label: texts[k].clone(), worker: None, pos: b.center, what: TaskRef::Shop { bid: b.id, k }, suspended: j.suspended, place: title.clone() });
        }
    }
    let mut idle = 0;
    for (id, act, jp) in &status {
        if listed.contains(id) {
            continue;
        }
        if act.is_empty() || act == "No Job" {
            idle += 1;
            continue;
        }
        let pos = jp.or_else(|| w.unit(*id).map(|u| u.pos)).unwrap_or_default();
        rows.push(TaskRow { cat: activity_cat(act), label: act.clone(), worker: Some(*id), pos, what: TaskRef::Unit, suspended: false, place: String::new() });
    }
    (rows, idle)
}

fn refresh_tasks(g: &mut Game) {
    let now = get_time();
    if !g.r10.tasks_dirty && now - g.r10.tasks_at < 0.5 && !g.r10.tasks.is_empty() {
        return;
    }
    let (rows, idle) = collect_tasks(g);
    g.r10.tasks = rows;
    g.r10.idle = idle;
    g.r10.tasks_at = now;
    g.r10.tasks_dirty = false;
}

enum TaskAct {
    None,
    Jump(usize),
    Cancel(usize),
    Suspend(usize),
    DoNow(usize),
}

fn task_matches(t: &TaskRow, tab: usize, unassigned: bool) -> bool {
    let cat_ok = match TASK_TABS.get(tab).and_then(|t| t.1) {
        None => true,
        Some(c) => t.cat == c,
    };
    cat_ok && (!unassigned || t.worker.is_none())
}

pub fn draw_tasks(g: &mut Game) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let Some(c) = frame(g, "Tasks", 1240.0) else { return };
    refresh_tasks(g);
    // Tabs with counts.
    let labels: Vec<String> = TASK_TABS
        .iter()
        .enumerate()
        .map(|(i, (l, _))| {
            let n = g.r10.tasks.iter().filter(|t| task_matches(t, i, false)).count();
            format!("{l} {n}")
        })
        .collect();
    let lrefs: Vec<&str> = labels.iter().map(|l| l.as_str()).collect();
    let (hit, tx) = tabs(g, tabs_x(g, c, "Tasks"), c.y - 3.0 * s, &lrefs, g.r10.task_tab);
    if let Some(i) = hit {
        g.r10.task_tab = i;
        g.r10.task_scroll = 0;
    }
    let _ = tx;
    let top = c.y + 2.0 * lh + 4.0 * s;
    draw_line(c.x, top - 6.0 * s, c.x + c.w, top - 6.0 * s, 1.0, col::BORDER);
    let assigned = g.r10.tasks.iter().filter(|t| t.worker.is_some()).count();
    let head = format!(
        "{} tasks, {} with a worker, {} waiting. {} citizens without a job. Click a task to go there.",
        g.r10.tasks.len(),
        assigned,
        g.r10.tasks.len() - assigned,
        g.r10.idle
    );
    g.font.draw(&clip(&head, ((c.w - 30.0 * 8.0 * s) / (GW * s)) as usize), c.x, top, s, col::TEXT_DIM);
    let un = g.r10.task_unassigned;
    if checkbox(&mut g.ui, &g.font, c.x + c.w - 26.0 * 8.0 * s, top, un, "Only without worker") {
        g.r10.task_unassigned = !un;
        g.r10.task_scroll = 0;
    }
    let y0 = top + lh + 6.0 * s;
    let xs = [c.x + 4.0 * s, c.x + 0.36 * c.w, c.x + 0.60 * c.w, c.x + 0.74 * c.w];
    for (x, t) in xs.iter().zip(["Task", "Assigned to", "Where", ""]) {
        g.font.draw(t, *x, y0, s, col::TEXT_DIM);
    }
    let list = Rect::new(c.x, y0 + lh + 2.0 * s, c.w, c.y + c.h - (y0 + lh + 2.0 * s));
    let row_h = lh + 6.0 * s;
    let vis = ((list.h / row_h) as usize).max(1);
    let idx: Vec<usize> = (0..g.r10.tasks.len()).filter(|&i| task_matches(&g.r10.tasks[i], g.r10.task_tab, un)).collect();
    g.r10.task_scroll = scroll(g, list, g.r10.task_scroll.min(idx.len().saturating_sub(1)), idx.len(), vis);
    if idx.is_empty() {
        g.font.draw("Nothing here.", list.x + 4.0 * s, list.y + 4.0 * s, s, col::TEXT_DIM);
    }
    let mut act = TaskAct::None;
    let bw = 10.0 * 8.0 * s;
    for (k, &i) in idx.iter().enumerate().skip(g.r10.task_scroll).take(vis) {
        let y = list.y + (k - g.r10.task_scroll) as f32 * row_h;
        let t = g.r10.tasks[i].clone();
        if k % 2 == 0 {
            draw_rectangle(list.x, y, list.w, row_h - 2.0 * s, Color::new(1.0, 1.0, 1.0, 0.035));
        }
        let ty = y + 3.0 * s;
        let row = Rect::new(list.x, y, xs[3] - list.x - 4.0 * s, row_h - 2.0 * s);
        if list_row(&mut g.ui, row, false) {
            act = TaskAct::Jump(i);
        }
        let lc = if t.suspended { col::TEXT_DIM } else if t.worker.is_some() { col::TEXT } else { col::TEXT_HI };
        let label = if t.suspended { format!("{} (suspended)", t.label) } else { t.label.clone() };
        let c0 = ((xs[1] - xs[0] - 8.0 * s) / (GW * s)) as usize;
        g.font.draw(&clip(&label, c0), xs[0], ty, s, lc);
        let who = match t.worker.and_then(|id| g.sim.world.unit(id)) {
            Some(u) => unit_name(u),
            None if t.suspended => "-".to_string(),
            None => "Inactive (no worker yet)".to_string(),
        };
        let c1 = ((xs[2] - xs[1] - 8.0 * s) / (GW * s)) as usize;
        g.font.draw(&clip(&who, c1), xs[1], ty, s, if t.worker.is_some() { col::TEXT_CYAN } else { col::TEXT_DIM });
        let wh = if t.place.is_empty() { format!("z {} ({}, {})", t.pos.z, t.pos.x, t.pos.y) } else { format!("{}, z {}", t.place, t.pos.z) };
        let c2 = ((xs[3] - xs[2] - 8.0 * s) / (GW * s)) as usize;
        g.font.draw(&clip(&wh, c2), xs[2], ty, s, col::TEXT_DIM);
        // Buttons: suspend / do now (workshop jobs), cancel.
        let by = y + 1.0 * s;
        let bh = row_h - 4.0 * s;
        let mut bx = xs[3];
        // Suspend and "do now" exist for workshop jobs (DF's job list has them there).
        let shop = matches!(t.what, TaskRef::Shop { .. });
        if shop {
            let sst = if t.suspended { BtnState::Active } else { BtnState::Normal };
            if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), if t.suspended { "Resume" } else { "Suspend" }, sst) {
                act = TaskAct::Suspend(i);
            }
            let first = matches!(t.what, TaskRef::Shop { k: 0, .. });
            let dst = if !first && !t.suspended { BtnState::Normal } else { BtnState::Disabled };
            if text_button(&mut g.ui, &g.font, Rect::new(bx + bw + 4.0 * s, by, bw, bh), "Do now", dst) {
                act = TaskAct::DoNow(i);
            }
            if g.ui.hovered(Rect::new(bx + bw + 4.0 * s, by, bw, bh)) {
                g.ui.tooltip = Some(("Move this job to the front of the workshop queue".into(), vec2(bx, by - 4.0)));
            }
        }
        bx += 2.0 * (bw + 4.0 * s);
        let can_cancel = !matches!(t.what, TaskRef::Unit | TaskRef::ConstructionOther);
        if text_button(&mut g.ui, &g.font, Rect::new(bx, by, bw, bh), "Cancel", if can_cancel { BtnState::Normal } else { BtnState::Disabled }) {
            act = TaskAct::Cancel(i);
        }
        if g.ui.hovered(Rect::new(bx, by, bw, bh)) {
            let tip = match t.what {
                TaskRef::Unit => "A creature's own activity cannot be cancelled here",
                TaskRef::ConstructionOther => "No cancel for this job yet",
                TaskRef::Unbuilt(_) => "Cancel: remove the planned building",
                TaskRef::Dump => "Cancel: the items here are no longer marked for dumping",
                _ => "Cancel this job (the designation is removed)",
            };
            g.ui.tooltip = Some((tip.into(), vec2(bx, by - 4.0)));
        }
    }
    if idx.len() > vis {
        let t = format!("{}-{} of {} (wheel scrolls)", g.r10.task_scroll + 1, (g.r10.task_scroll + vis).min(idx.len()), idx.len());
        g.font.draw(&t, c.x + c.w - g.font.width(&t, s), c.y + c.h - GH * s, s, col::TEXT_DIM);
    }
    match act {
        TaskAct::None => {}
        TaskAct::Jump(i) => {
            let t = g.r10.tasks[i].clone();
            jump_to_task(g, &t);
        }
        TaskAct::Cancel(i) => {
            let t = g.r10.tasks[i].clone();
            cancel_task(g, &t);
        }
        TaskAct::Suspend(i) => {
            let t = g.r10.tasks[i].clone();
            if let Some((bid, k)) = shop_job_index(g, &t) {
                if let Some(j) = g.sim.world.buildings.iter_mut().find(|b| b.id == bid).and_then(|b| b.jobs.get_mut(k)) {
                    j.suspended = !j.suspended;
                }
            }
            g.r10.tasks_dirty = true;
        }
        TaskAct::DoNow(i) => {
            let t = g.r10.tasks[i].clone();
            if let Some((bid, k)) = shop_job_index(g, &t) {
                if let Some(b) = g.sim.world.buildings.iter_mut().find(|b| b.id == bid) {
                    let j = b.jobs.remove(k);
                    b.jobs.insert(0, j);
                }
            }
            g.r10.tasks_dirty = true;
        }
    }
}

/// The current queue index of a workshop task row. The list is rebuilt twice a second, so the
/// queue may have changed since (a job finished): the row's index is used only while it still
/// holds the same job, else the first job with the same text.
fn shop_job_index(g: &Game, t: &TaskRow) -> Option<(i32, usize)> {
    let TaskRef::Shop { bid, k } = t.what else { return None };
    let b = g.sim.world.building(bid)?;
    let templates = g.sim.order_templates();
    let same = |j: &rf_core::BuildingJob| super::building::job_text(&templates, j) == t.label;
    if b.jobs.get(k).map(same).unwrap_or(false) {
        return Some((bid, k));
    }
    b.jobs.iter().position(same).map(|k| (bid, k))
}

/// Close the screen and show the task on the map (its worker, building or tile selected).
pub fn jump_to_task(g: &mut Game, t: &TaskRow) {
    g.screen = Screen::None;
    g.cam.focus(t.pos);
    match (&t.what, t.worker) {
        (TaskRef::Shop { bid, .. } | TaskRef::Unbuilt(bid), None) => {
            g.selected = Selected::Building(*bid);
            g.bui = Default::default();
        }
        (_, Some(id)) => {
            g.select_unit(id);
            g.unit_tab = 0;
        }
        _ => g.selected = Selected::Tile(t.pos),
    }
}

/// Cancel a task with the sim's own APIs (designations, detailing, planned constructions,
/// planned buildings, workshop queue entries).
pub fn cancel_task(g: &mut Game, t: &TaskRow) {
    let p = t.pos;
    match &t.what {
        TaskRef::Designation => g.sim.designate(p, p, DesignationKind::Remove),
        TaskRef::Detail => {
            g.sim.designate_detail(p, p, DetailKind::Remove);
        }
        TaskRef::Construction => {
            g.sim.build_construction(ConstructionKind::Remove, p, p);
        }
        TaskRef::Unbuilt(bid) => g.sim.remove_building(*bid),
        TaskRef::Dump => {
            let ids: Vec<i32> = g.sim.world.items.iter().filter(|it| it.pos == p && it.flags.dump).map(|it| it.id).collect();
            for id in ids {
                g.sim.set_item_flag(id, "dump", false);
            }
        }
        TaskRef::Shop { .. } => {
            let Some((bid, k)) = shop_job_index(g, t) else { return };
            if let Some(b) = g.sim.world.buildings.iter_mut().find(|b| b.id == bid) {
                b.jobs.remove(k);
            }
        }
        TaskRef::Unit | TaskRef::ConstructionOther => return,
    }
    g.set_status(format!("Cancelled: {}", t.label));
    g.r10.tasks_dirty = true;
}

// ====================================================================== hauling

// [app12] The Hauling screen is the route editor in `round12::draw_hauling`.

// ====================================================================== build hotkey chains

/// What a key in the build menu leads to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BTarget {
    /// Open a DF menu group (pushed on the path); the Rustfort tab it shows (category name).
    Group(&'static str, &'static str),
    /// A building of `Sim::build_options()` (kind, subtype).
    Opt(&'static str, &'static str),
    /// A construction of the Constructions tab.
    Constr(ConstructionKind),
}

/// The DF build menu tree (group -> (binding, target)). `""` is the top level.
pub const BUILD_KEYS: &[(&str, &str, BTarget)] = &[
    ("", "HOTKEY_BUILDING_WORKSHOP", BTarget::Group("Workshops", "Workshops")),
    ("", "HOTKEY_BUILDING_FURNITURE", BTarget::Group("Furniture", "Furniture")),
    ("", "HOTKEY_BUILDING_CONSTRUCTION", BTarget::Group("Constructions", "Constructions")),
    ("", "HOTKEY_BUILDING_FARMPLOT", BTarget::Opt("FarmPlot", "")),
    ("", "HOTKEY_BUILDING_WELL", BTarget::Opt("Well", "")),
    // Workshops and their DF sub-groups.
    ("Workshops", "HOTKEY_BUILDING_WORKSHOP_MASON", BTarget::Opt("Workshop", "Masons")),
    ("Workshops", "HOTKEY_BUILDING_WORKSHOP_CARPENTER", BTarget::Opt("Workshop", "Carpenters")),
    ("Workshops", "HOTKEY_BUILDING_WORKSHOP_CRAFTSMAN", BTarget::Opt("Workshop", "Craftsdwarfs")),
    ("Workshops", "HOTKEY_BUILDING_WORKSHOP_MECHANIC", BTarget::Opt("Workshop", "Mechanics")),
    ("Workshops", "HOTKEY_BUILDING_WORKSHOP_METALSMITH", BTarget::Opt("Workshop", "MetalsmithsForge")),
    ("Workshops", "HOTKEY_BUILDING_WORKSHOP_JEWELER", BTarget::Opt("Workshop", "Jewelers")),
    ("Workshops", "HOTKEY_BUILDING_WORKSHOP_BOWYER", BTarget::Opt("Workshop", "Bowyers")),
    ("Workshops", "HOTKEY_BUILDING_WORKSHOP_SIEGE", BTarget::Opt("Workshop", "Siege")),
    ("Workshops", "HOTKEY_BUILDING_WORKSHOP_ASHERY", BTarget::Opt("Workshop", "Ashery")),
    ("Workshops", "HOTKEY_BUILDING_KENNEL", BTarget::Opt("Workshop", "Kennels")),
    ("Workshops", "HOTKEY_BUILDING_FURNACE", BTarget::Group("Furnaces", "Furnaces")),
    ("Workshops", "HOTKEY_BUILDING_FARMING", BTarget::Group("Farming", "Workshops")),
    ("Workshops", "HOTKEY_BUILDING_WORKSHOPS_CLOTHING_LEATHER", BTarget::Group("Clothing", "Workshops")),
    ("Farming", "HOTKEY_BUILDING_WORKSHOP_FARMER", BTarget::Opt("Workshop", "Farmers")),
    ("Farming", "HOTKEY_BUILDING_WORKSHOP_QUERN", BTarget::Opt("Workshop", "Quern")),
    ("Farming", "HOTKEY_BUILDING_WORKSHOP_MILLSTONE", BTarget::Opt("Workshop", "Millstone")),
    ("Farming", "HOTKEY_BUILDING_WORKSHOP_STILL", BTarget::Opt("Workshop", "Still")),
    ("Farming", "HOTKEY_BUILDING_WORKSHOP_KITCHEN", BTarget::Opt("Workshop", "Kitchen")),
    ("Farming", "HOTKEY_BUILDING_WORKSHOP_FISHERY", BTarget::Opt("Workshop", "Fishery")),
    ("Farming", "HOTKEY_BUILDING_WORKSHOP_BUTCHER", BTarget::Opt("Workshop", "Butchers")),
    ("Clothing", "HOTKEY_BUILDING_WORKSHOP_LEATHER", BTarget::Opt("Workshop", "Leatherworks")),
    ("Clothing", "HOTKEY_BUILDING_WORKSHOP_LOOM", BTarget::Opt("Workshop", "Loom")),
    ("Clothing", "HOTKEY_BUILDING_WORKSHOP_CLOTHES", BTarget::Opt("Workshop", "Clothiers")),
    ("Clothing", "HOTKEY_BUILDING_WORKSHOP_DYER", BTarget::Opt("Workshop", "Dyers")),
    ("Clothing", "HOTKEY_BUILDING_WORKSHOP_TANNER", BTarget::Opt("Workshop", "Tanners")),
    ("Furnaces", "HOTKEY_BUILDING_FURNACE_WOOD", BTarget::Opt("Furnace", "WoodFurnace")),
    ("Furnaces", "HOTKEY_BUILDING_FURNACE_SMELTER", BTarget::Opt("Furnace", "Smelter")),
    ("Furnaces", "HOTKEY_BUILDING_FURNACE_GLASS", BTarget::Opt("Furnace", "GlassFurnace")),
    ("Furnaces", "HOTKEY_BUILDING_FURNACE_KILN", BTarget::Opt("Furnace", "Kiln")),
    // Furniture (DF binds the chair and the door both to `r`: pressing it again cycles).
    ("Furniture", "HOTKEY_BUILDING_BED", BTarget::Opt("Bed", "")),
    ("Furniture", "HOTKEY_BUILDING_CHAIR", BTarget::Opt("Chair", "")),
    ("Furniture", "HOTKEY_BUILDING_TABLE", BTarget::Opt("Table", "")),
    ("Furniture", "HOTKEY_BUILDING_DOOR", BTarget::Opt("Door", "")),
    ("Furniture", "HOTKEY_BUILDING_CABINET", BTarget::Opt("Cabinet", "")),
    ("Furniture", "HOTKEY_BUILDING_BOX", BTarget::Opt("Box", "")),
    ("Furniture", "HOTKEY_BUILDING_COFFIN", BTarget::Opt("Coffin", "")),
    // Constructions.
    ("Constructions", "HOTKEY_BUILDING_CONSTRUCTION_WALL", BTarget::Constr(ConstructionKind::Wall)),
    ("Constructions", "HOTKEY_BUILDING_CONSTRUCTION_FLOOR", BTarget::Constr(ConstructionKind::Floor)),
    ("Constructions", "HOTKEY_BUILDING_CONSTRUCTION_RAMP", BTarget::Constr(ConstructionKind::Ramp)),
    ("Constructions", "HOTKEY_BUILDING_CONSTRUCTION_STAIR_UPDOWN", BTarget::Constr(ConstructionKind::UpDownStair)),
    ("Constructions", "HOTKEY_BUILDING_CONSTRUCTION_FORTIFICATION", BTarget::Constr(ConstructionKind::Fortification)),
    ("Constructions", "HOTKEY_BUILDING_CONSTRUCTION_TRACK", BTarget::Constr(ConstructionKind::Track)),
    // [app12] Machines (DF `m`), siege engines (DF Military `y`), the track stop (Shift+K).
    ("Constructions", "HOTKEY_BUILDING_CONSTRUCTION_TRACK_STOP", BTarget::Opt("Trap", "TrackStop")),
    ("", "HOTKEY_BUILDING_MACHINE", BTarget::Group("Machines", "Machines")),
    ("", "HOTKEY_BUILDING_MILITARY", BTarget::Group("Military", "Siege engines")),
    ("Machines", "HOTKEY_BUILDING_MACHINE_SCREW_PUMP", BTarget::Opt("ScrewPump", "")),
    ("Machines", "HOTKEY_BUILDING_MACHINE_WATER_WHEEL", BTarget::Opt("WaterWheel", "")),
    ("Machines", "HOTKEY_BUILDING_MACHINE_WINDMILL", BTarget::Opt("Windmill", "")),
    ("Machines", "HOTKEY_BUILDING_MACHINE_GEAR_ASSEMBLY", BTarget::Opt("GearAssembly", "")),
    ("Machines", "HOTKEY_BUILDING_MACHINE_AXLE_HORIZONTAL", BTarget::Opt("AxleHorizontal", "")),
    ("Machines", "HOTKEY_BUILDING_MACHINE_AXLE_VERTICAL", BTarget::Opt("AxleVertical", "")),
    ("Machines", "HOTKEY_BUILDING_MACHINE_ROLLERS", BTarget::Opt("Rollers", "")),
    ("Military", "HOTKEY_BUILDING_SIEGEENGINE_BALLISTA", BTarget::Opt("SiegeEngine", "Ballista")),
    ("Military", "HOTKEY_BUILDING_SIEGEENGINE_CATAPULT", BTarget::Opt("SiegeEngine", "Catapult")),
    ("Military", "HOTKEY_BUILDING_SIEGEENGINE_BOLT_THROWER", BTarget::Opt("SiegeEngine", "BoltThrower")),
];

/// The group the build menu is in (last path element, `""` at the top).
fn cur_group(g: &Game) -> &'static str {
    g.r10.bpath.last().copied().unwrap_or("")
}

/// The Rustfort tab (category name) a DF group shows.
fn group_tab(group: &str) -> Option<&'static str> {
    BUILD_KEYS.iter().find_map(|(_, _, t)| match t {
        BTarget::Group(gname, tab) if *gname == group => Some(*tab),
        _ => None,
    })
}

fn set_tab(g: &mut Game, tab: &str) {
    let cats = super::build::menu_categories(&g.build_opts);
    if let Some(i) = cats.iter().position(|c| *c == tab) {
        g.build_cat = i;
    }
    g.r10.bcat_seen = Some(g.build_cat);
}

/// Index of a build option, if the sim offers it.
fn opt_index(g: &Game, kind: &str, sub: &str) -> Option<usize> {
    g.build_opts.iter().position(|o| o.kind == kind && o.subtype == sub)
}

fn target_available(g: &Game, t: BTarget) -> bool {
    match t {
        BTarget::Opt(k, sub) => opt_index(g, k, sub).is_some(),
        _ => true,
    }
}

/// Keys inside the build menu. Returns true when a key was used.
fn build_keys(g: &mut Game) -> bool {
    // A tab clicked with the mouse puts the chain into that tab's group.
    if g.r10.bcat_seen.is_none() {
        g.r10.bcat_seen = Some(g.build_cat);
    } else if g.r10.bcat_seen != Some(g.build_cat) {
        let cats = super::build::menu_categories(&g.build_opts);
        let tab = cats.get(g.build_cat).copied().unwrap_or("");
        g.r10.bpath = match tab {
            "Workshops" => vec!["Workshops"],
            "Furnaces" => vec!["Workshops", "Furnaces"],
            "Furniture" => vec!["Furniture"],
            "Constructions" => vec!["Constructions"],
            "Machines" => vec!["Machines"],     // [app12]
            "Siege engines" => vec!["Military"], // [app12]
            _ => Vec::new(),
        };
        g.r10.bcat_seen = Some(g.build_cat);
    }
    let group = cur_group(g);
    let hits: Vec<BTarget> = BUILD_KEYS.iter().filter(|(gr, b, t)| *gr == group && g.input.hit(&g.binds, b) && target_available(g, *t)).map(|e| e.2).collect();
    if hits.is_empty() {
        return false;
    }
    // Several entries on one key (DF: chair and door on `r`): the next one after the current.
    let cur = match g.tool {
        Tool::Build(i) => hits.iter().position(|t| matches!(t, BTarget::Opt(k, sub) if opt_index(g, k, sub) == Some(i))),
        Tool::Construct(c) => hits.iter().position(|t| *t == BTarget::Constr(c)),
        _ => None,
    };
    let t = hits[cur.map(|k| (k + 1) % hits.len()).unwrap_or(0)];
    g.drag = None;
    g.build_check = None;
    match t {
        BTarget::Group(name, tab) => {
            g.r10.bpath.push(name);
            set_tab(g, tab);
            g.tool = Tool::None;
        }
        BTarget::Opt(k, sub) => {
            if let Some(i) = opt_index(g, k, sub) {
                let cat = g.build_opts[i].category;
                set_tab(g, cat);
                g.tool = Tool::Build(i);
            }
        }
        BTarget::Constr(c) => {
            set_tab(g, super::round5::CONSTR_CAT);
            g.tool = Tool::Construct(c);
        }
    }
    true
}

/// Key label of the build-menu entry that leads to option `i` at the current level, e.g.
/// `t` for the mason's workshop inside Workshops ("" if no key reaches it from here).
pub fn option_key(g: &Game, i: usize) -> String {
    let group = cur_group(g);
    let o = &g.build_opts[i];
    BUILD_KEYS
        .iter()
        .find(|(gr, _, t)| *gr == group && matches!(t, BTarget::Opt(k, sub) if *k == o.kind && *sub == o.subtype))
        .map(|(_, b, _)| g.binds.label(b))
        .unwrap_or_default()
}

/// Build menu button label with its key: `Mason's workshop (t)`.
pub fn option_label(g: &Game, i: usize) -> String {
    let k = option_key(g, i);
    let l = g.build_opts[i].label;
    if k.is_empty() {
        l.to_string()
    } else {
        format!("{l} ({k})")
    }
}

/// The key bar of the build menu: where the chain is and what the keys do from here.
fn draw_build_keys(g: &mut Game) {
    let s = g.ui.scale;
    let group = cur_group(g);
    let mut parts: Vec<String> = Vec::new();
    for (gr, b, t) in BUILD_KEYS {
        if *gr != group || !target_available(g, *t) {
            continue;
        }
        let k = g.binds.label(b);
        if k.is_empty() {
            continue;
        }
        let what = match t {
            BTarget::Group(name, _) => format!("{name}..."),
            BTarget::Opt(kind, sub) => opt_index(g, kind, sub).map(|i| g.build_opts[i].label.to_string()).unwrap_or_default(),
            BTarget::Constr(c) => super::round5::constr_label(*c).to_string(),
        };
        parts.push(format!("{k} {what}"));
    }
    if parts.is_empty() {
        return;
    }
    let path = if g.r10.bpath.is_empty() { "Build".to_string() } else { format!("Build > {}", g.r10.bpath.join(" > ")) };
    let back = if g.r10.bpath.is_empty() { "" } else { "   Esc: back" };
    let text = format!("{path}:  {}{back}", parts.join("  "));
    let maxc = ((screen_width() - 80.0 * s) / (GW * s)) as usize;
    let lines = super::round9::wrap(&text, maxc);
    let lh = g.ui.line_h();
    let w = lines.iter().map(|l| g.font.width(l, s)).fold(0.0, f32::max) + 16.0 * s;
    let h = lines.len() as f32 * lh + 10.0 * s;
    let r = Rect::new((screen_width() - w) / 2.0, super::topbar::TOPBAR_H * s + 8.0 * s, w, h);
    panel(&mut g.ui, r);
    for (k, l) in lines.iter().enumerate() {
        g.font.draw(l, r.x + 8.0 * s, r.y + 5.0 * s + k as f32 * lh, s, if k == 0 { col::TEXT_HI } else { col::TEXT });
    }
}

/// Round-10 hotkeys (called from `Game::global_keys` before the global menu keys).
pub fn keys(g: &mut Game) -> bool {
    if g.submenu == Submenu::Build {
        if build_keys(g) {
            return true;
        }
    } else if !g.r10.bpath.is_empty() || g.r10.bcat_seen.is_some() {
        g.r10.bpath.clear();
        g.r10.bcat_seen = None;
    }
    let (b, inp) = (&g.binds, &g.input);
    if inp.hit(b, "D_JOBLIST") {
        g.toggle_screen(Screen::Tasks);
        g.r10.tasks_dirty = true;
        return true;
    }
    if inp.hit(b, "D_HAULING") {
        g.toggle_screen(Screen::Hauling);
        return true;
    }
    false
}

/// Esc: context menu, then back one level in the build menu. Returns true when used.
pub fn escape(g: &mut Game) -> bool {
    if g.r10.ctx.is_some() {
        g.r10.ctx = None;
        return true;
    }
    if g.submenu == Submenu::Build {
        if matches!(g.tool, Tool::Build(_) | Tool::Construct(_)) {
            // Back to the list, like DF.
            g.tool = Tool::None;
            g.drag = None;
            return true;
        }
        if g.r10.bpath.pop().is_some() {
            let tab = g.r10.bpath.last().and_then(|gr| group_tab(gr));
            if let Some(t) = tab {
                set_tab(g, t);
            } else {
                g.r10.bcat_seen = Some(g.build_cat);
            }
            return true;
        }
    }
    false
}

// ====================================================================== map context menu

#[derive(Clone, Debug, PartialEq)]
pub enum CtxAct {
    SelectUnit(i32),
    OpenBuilding(i32),
    CancelDig,
    CancelDetail,
    CancelConstruction,
    Look,
}

#[derive(Clone, Debug)]
pub struct Ctx {
    pub pos: Pos,
    pub at: Vec2,
    pub items: Vec<(String, CtxAct)>,
}

/// The context entries for a tile.
pub fn context_items(g: &Game, p: Pos) -> Vec<(String, CtxAct)> {
    let w = &g.sim.world;
    let mut v: Vec<(String, CtxAct)> = Vec::new();
    for u in w.units.iter().filter(|u| u.pos == p && !u.flags.dead) {
        // Unspotted raiders stay invisible (as on the map).
        if !(u.flags.hostile && g.sim.unit_hidden(u.id)) {
            v.push((format!("Select {}", unit_name(u)), CtxAct::SelectUnit(u.id)));
        }
    }
    for b in w.buildings.iter().filter(|b| b.contains(p)) {
        let t = if b.kind == "Civzone" { format!("{} zone", super::words(&b.subtype)) } else { building_title(g, b) };
        if b.kind == "Construction" && !b.built {
            v.push((format!("Cancel construction: {t}"), CtxAct::CancelConstruction));
        } else if b.kind != "Construction" {
            v.push((format!("Open {t}"), CtxAct::OpenBuilding(b.id)));
        }
    }
    if let Some(t) = w.tile(p) {
        if t.desig != 0 {
            v.push((format!("Cancel designation ({:?})", t.dig()), CtxAct::CancelDig));
        }
        use rf_core::tile_flags as tf;
        if t.flags & (tf::DESIG_SMOOTH | tf::DESIG_ENGRAVE) != 0 || w.carve_designations.iter().any(|d| d.pos == p) {
            v.push(("Cancel smoothing/engraving".into(), CtxAct::CancelDetail));
        }
    }
    v.push(("Look at this tile".into(), CtxAct::Look));
    v
}

/// Right-click on the map without a tool: open the context menu (true), or false when the
/// tile has nothing but "look" (then the right-click deselects as before).
pub fn context_click(g: &mut Game) -> bool {
    if g.r10.ctx.is_some() {
        g.r10.ctx = None;
        return true;
    }
    let Some(p) = g.hover else { return false };
    let items = context_items(g, p);
    if items.len() <= 1 {
        return false;
    }
    g.r10.ctx = Some(Ctx { pos: p, at: g.mouse(), items });
    true
}

fn ctx_apply(g: &mut Game, p: Pos, a: CtxAct) {
    match a {
        CtxAct::SelectUnit(id) => {
            g.select_unit(id);
            g.unit_tab = 0;
        }
        CtxAct::OpenBuilding(id) => {
            g.selected = Selected::Building(id);
            g.bui = Default::default();
        }
        CtxAct::CancelDig => {
            g.sim.designate(p, p, DesignationKind::Remove);
            g.set_status(format!("Designation removed at {}, {}, z {}", p.x, p.y, p.z));
        }
        CtxAct::CancelDetail => {
            g.sim.designate_detail(p, p, DetailKind::Remove);
            g.set_status("Smoothing/engraving designation removed".into());
        }
        CtxAct::CancelConstruction => {
            g.sim.build_construction(ConstructionKind::Remove, p, p);
            g.set_status("Planned construction cancelled".into());
        }
        CtxAct::Look => g.selected = Selected::Tile(p),
    }
}

fn draw_ctx(g: &mut Game) {
    let Some(ctx) = g.r10.ctx.clone() else { return };
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let row_h = lh + 4.0 * s;
    let title = format!("{}, {}, z {}", ctx.pos.x, ctx.pos.y, ctx.pos.z);
    let w = ctx.items.iter().map(|(l, _)| g.font.width(l, s)).fold(g.font.width(&title, s), f32::max) + 20.0 * s;
    let h = (ctx.items.len() as f32 + 1.0) * row_h + 10.0 * s;
    let x = (ctx.at.x + 6.0 * s).min(screen_width() - w - 4.0).max(2.0);
    let y = (ctx.at.y + 4.0 * s).min(screen_height() - h - 4.0).max(2.0);
    let r = Rect::new(x, y, w, h);
    draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.05, 0.045, 0.04, 1.0));
    panel(&mut g.ui, r);
    g.font.draw(&title, x + 10.0 * s, y + 6.0 * s, s, col::TEXT_DIM);
    let mut pick = None;
    for (k, (l, a)) in ctx.items.iter().enumerate() {
        let rr = Rect::new(x + 4.0 * s, y + 5.0 * s + (k as f32 + 1.0) * row_h, w - 8.0 * s, row_h - 2.0 * s);
        if list_row(&mut g.ui, rr, false) {
            pick = Some(a.clone());
        }
        let c = if l.starts_with("Cancel") { col::TEXT_BAD } else { col::TEXT };
        g.font.draw(l, rr.x + 6.0 * s, rr.y + 3.0 * s, s, c);
    }
    if let Some(a) = pick {
        g.r10.ctx = None;
        ctx_apply(g, ctx.pos, a);
    } else if g.ui.lpressed && !g.ui.hovered(r) {
        // A click elsewhere closes the menu (and still works on the map).
        g.r10.ctx = None;
    }
}

// ====================================================================== look lines

/// DF item/engraving quality names (0 ordinary .. 5 masterful) and their marks.
pub const QUALITY: [(&str, &str); 6] = [("ordinary", ""), ("well-crafted", "-"), ("finely-crafted", "+"), ("superior", "*"), ("exceptional", "≡"), ("masterful", "☼")];

/// Generic picture subjects (DF's own engraving subjects are not exported).
const MOTIFS: &[&str] = &[
    "a mountain", "a dwarf", "the fortress", "a cave spider", "a tree", "a pick and a hammer", "the stars", "a mug of ale",
    "a cut gem", "a river", "a crown", "a battle axe", "a plump helmet", "a bearded face", "an anvil", "a cavern",
];

fn motif(p: Pos) -> &'static str {
    let h = (p.x as u32).wrapping_mul(73_856_093) ^ (p.y as u32).wrapping_mul(19_349_663) ^ (p.z as u32).wrapping_mul(83_492_791);
    MOTIFS[(h % MOTIFS.len() as u32) as usize]
}

/// One-line engraving note for the hover box: `Masterful engraving ☼`.
pub fn engraving_line(g: &Game, p: Pos) -> Option<String> {
    let e = g.sim.engraving_at(p)?;
    let (q, m) = QUALITY[e.quality.min(5) as usize];
    let mut c = q.chars();
    let q = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
    Some(format!("{q} engraving {m}").trim().to_string())
}

/// DF-style description lines of a tile for the tile sheet: surface, engraving (quality,
/// engraver, value), what the tile is designated for.
pub fn look_lines(g: &Game, p: Pos) -> Vec<(String, Color)> {
    use rf_core::{Shape, TtMaterial, TtSpecial, NO_MAT};
    let w = &g.sim.world;
    let mut v: Vec<(String, Color)> = Vec::new();
    let Some(t) = w.tile(p) else { return v };
    if t.hidden() {
        return v;
    }
    let tt = w.tiletypes.get(t.tt);
    let mat = if t.mat != NO_MAT { w.material(t.mat).map(|m| m.name.clone()).unwrap_or_default() } else { String::new() };
    let what = match tt.shape {
        Shape::Wall => "wall",
        Shape::Floor => "floor",
        Shape::Ramp => "ramp",
        Shape::StairUp | Shape::StairDown | Shape::StairUpdown => "staircase",
        Shape::Fortification => "fortification",
        _ => "",
    };
    if !what.is_empty() && tt.material != TtMaterial::Tree {
        let smooth = matches!(tt.special, TtSpecial::Smooth | TtSpecial::SmoothDead);
        let m = if mat.is_empty() { String::new() } else { format!(" of {mat}") };
        let line = if tt.material == TtMaterial::Construction {
            format!("A constructed {what}{m}.")
        } else if smooth {
            format!("A smoothed {what}{m}.")
        } else if matches!(tt.material, TtMaterial::Stone | TtMaterial::Mineral | TtMaterial::Feature | TtMaterial::LavaStone) {
            format!("A rough {what}{m}, never smoothed.")
        } else {
            format!("A natural {what}{m}.")
        };
        v.push((line, col::TEXT));
    }
    if let Some(l) = super::round12::track_line(tt) {
        v.push((l, col::TEXT_CYAN)); // [app12] track directions
    }
    if let Some(e) = g.sim.engraving_at(p) {
        let (q, mark) = QUALITY[e.quality.min(5) as usize];
        let surf = if e.floor { "floor" } else { "wall" };
        let art = if q.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
        v.push((format!("The {surf} is engraved with {art} {q} image of {}. {mark}", motif(p)).trim().to_string(), Color::from_rgba(225, 205, 140, 255)));
        let by = w.unit(e.engraver).map(unit_name).unwrap_or_else(|| "an unknown engraver".into());
        let lvl = super::round9::level_name(e.skill);
        v.push((format!("Engraved by {by} ({lvl} engraver); value {}☼.", g.sim.engraving_value(e)), col::TEXT_DIM));
        v.push(("(DF does not export the picture itself; the subject is a Rustfort stand-in.)".into(), col::TEXT_DIM));
    }
    v
}

/// Items lying on a tile, grouped by their label: (text, item ids, forbidden).
pub fn item_groups(g: &Game, p: Pos) -> Vec<(String, Vec<i32>, bool)> {
    let mut groups: Vec<(String, Vec<i32>, bool)> = Vec::new();
    for it in g.sim.world.items.iter().filter(|it| it.pos == p && it.flags.on_ground) {
        let label = format!("{}{}", item_label(g, it), super::round5::item_tags(it));
        match groups.iter_mut().find(|(l, _, f)| *l == label && *f == it.flags.forbid) {
            Some(gr) => gr.1.push(it.id),
            None => groups.push((label, vec![it.id], it.flags.forbid)),
        }
    }
    groups
        .into_iter()
        .map(|(l, ids, f)| {
            let t = if ids.len() > 1 { format!("{} x {l}", ids.len()) } else { l };
            (t, ids, f)
        })
        .collect()
}

/// F (forbid) and D (dump) toggles for a group of items (on = all of them).
pub fn group_toggles(g: &mut Game, ids: &[i32], right: f32, y: f32) {
    let s = g.ui.scale;
    let all = |g: &Game, f: fn(&rf_core::Item) -> bool| ids.iter().all(|&id| g.sim.world.item(id).map(f).unwrap_or(false));
    let dump = all(g, |it| it.flags.dump);
    let forbid = all(g, |it| it.flags.forbid);
    if super::round5::flag_toggle(g, right - 16.0 * s, y, "D", dump, if dump { "Dump all of these: on (click to remove)" } else { "Mark all of these for dumping" }) {
        for &id in ids {
            g.sim.set_item_flag(id, "dump", !dump);
        }
    }
    if super::round5::flag_toggle(g, right - 36.0 * s, y, "F", forbid, if forbid { "All forbidden (click to claim)" } else { "Forbid all of these" }) {
        for &id in ids {
            g.sim.set_item_flag(id, "forbid", !forbid);
        }
    }
}

// ====================================================================== crash guard

/// Real seconds between in-memory snapshots while the game runs.
pub const SNAP_EVERY_S: f64 = 60.0;

/// The world as it was a moment ago (the crash dialog saves it).
pub struct Snap {
    pub world: Box<World>,
    pub seed: u64,
    pub tick: u64,
    pub date: String,
    /// Real time it was taken (`get_time`) and how long the copy took (ms).
    pub taken: f64,
    pub ms: f64,
}

#[derive(Clone, Debug)]
pub struct Crash {
    /// `simulation` or `interface`.
    pub origin: &'static str,
    pub msg: String,
    /// Thread and place from the panic hook.
    pub report: Option<String>,
    pub tick: u64,
    pub date: String,
    /// Result of the last dialog action (text, ok).
    pub result: Option<(String, bool)>,
}

fn take_snapshot(g: &mut Game) {
    let t = std::time::Instant::now();
    let world = Box::new(g.sim.world.clone());
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    g.r10.snap = Some(Snap { world, seed: g.seed, tick: g.sim.ticks(), date: crate::game::date_string(&g.sim.world.calendar), taken: get_time(), ms });
}

/// Per frame (from `Game::update_sim`): notice a crashed sim, keep the snapshot fresh.
pub fn update(g: &mut Game) {
    if g.r10.crash.is_none() {
        if let Some(m) = g.sim.crash().map(|m| m.to_string()) {
            on_crash(g, "simulation", m);
            return;
        }
    } else {
        return;
    }
    let now = get_time();
    let due = match &g.r10.snap {
        None => true,
        Some(s) => now - s.taken >= SNAP_EVERY_S && s.tick != g.sim.ticks(),
    };
    if due {
        take_snapshot(g);
    }
}

fn on_crash(g: &mut Game, origin: &'static str, msg: String) {
    let report = crate::crash::last_report();
    let date = crate::game::date_string(&g.sim.world.calendar);
    let tick = g.sim.ticks();
    g.paused = true;
    let snap = match &g.r10.snap {
        Some(s) => format!("last good state: {} (tick {}), {:.0} s before the crash", s.date, s.tick, get_time() - s.taken),
        None => "no snapshot yet".to_string(),
    };
    crate::crash::append_log(&format!(
        "--- caught {origin} panic: {msg}\n    fort {}, {date}, tick {tick}; {snap}\n",
        g.sim.world.meta.fort_name
    ));
    eprintln!("crash guard: {origin} panic caught ({msg}); see {}", crate::crash::log_path().display());
    g.r10.crash = Some(Crash { origin, msg, report, tick, date, result: None });
}

/// A panic in the frame's interface code (caught in `game::run`).
pub fn on_ui_panic(g: &mut Game, msg: String) {
    if g.r10.crash.is_some() {
        return;
    }
    // A sim panic on this thread was already recorded by `run_guarded`.
    if let Some(m) = g.sim.crash().map(|m| m.to_string()) {
        on_crash(g, "simulation", m);
    } else {
        on_crash(g, "interface", msg);
    }
}

/// The frame while the crash dialog is up (instead of the map and the normal interface).
pub fn crash_frame(g: &mut Game) {
    clear_background(Color::from_rgba(12, 10, 9, 255));
    let scale = g.ui_scale;
    g.ui.begin(&g.input, scale);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| draw_crash(g)));
    if res.is_err() {
        draw_text("Rustfort crashed and could not draw the crash dialog.", 20.0, 40.0, 24.0, WHITE);
        draw_text(&format!("Details: {}", crate::crash::log_path().display()), 20.0, 70.0, 20.0, LIGHTGRAY);
        draw_text("Close the window to quit.", 20.0, 100.0, 20.0, LIGHTGRAY);
    }
}

fn save_crash_world(world: &World, seed: u64) -> Result<crate::save::SaveInfo, String> {
    crate::save::save_world_tagged(world, seed, &crate::save::save_dir(), "crash")
}

fn draw_crash(g: &mut Game) {
    let Some(cr) = g.r10.crash.clone() else { return };
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let (sw, sh) = (screen_width(), screen_height());
    let (w, h) = ((860.0 * s).min(sw - 30.0), (560.0 * s).min(sh - 30.0));
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    panel(&mut g.ui, r);
    let pad = 16.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    let maxc = ((w - 2.0 * pad) / (GW * s)) as usize;
    g.font.draw("Rustfort hit an error", x, y, 1.5 * s, col::TEXT_BAD);
    y += 1.5 * GH * s + 10.0 * s;
    let what = if cr.origin == "simulation" {
        "The simulation stopped with an error. The game is paused and nothing is lost yet."
    } else {
        "The interface stopped with an error. The fort itself is fine; the game is paused."
    };
    g.font.draw(&clip(what, maxc), x, y, s, col::TEXT);
    y += lh + 4.0 * s;
    for l in super::round9::wrap(&format!("Error: {}", cr.msg), maxc).iter().take(4) {
        g.font.draw(l, x, y, s, col::TEXT_HI);
        y += lh;
    }
    if let Some(rep) = &cr.report {
        if let Some(place) = rep.lines().last() {
            g.font.draw(&clip(place, maxc), x, y, s, col::TEXT_DIM);
            y += lh;
        }
    }
    g.font.draw(&clip(&format!("At {} (tick {}). Details and backtrace: {}", cr.date, cr.tick, crate::crash::log_path().display()), maxc), x, y, s, col::TEXT_DIM);
    y += lh + 8.0 * s;
    let snap_txt = match &g.r10.snap {
        Some(sn) => format!("Last good state (kept in memory, copied in {:.0} ms): {}, tick {}, {:.0} s ago.", sn.ms, sn.date, sn.tick, get_time() - sn.taken),
        None => "There is no in-memory snapshot yet.".to_string(),
    };
    g.font.draw(&clip(&snap_txt, maxc), x, y, s, col::TEXT_GOOD);
    y += lh + 10.0 * s;
    let bw = (w - 2.0 * pad - 8.0 * s) / 2.0;
    let bh = 28.0 * s;
    let has_snap = g.r10.snap.is_some();
    let st = |on: bool| if on { BtnState::Normal } else { BtnState::Disabled };
    let mut result: Option<(String, bool)> = None;
    // Row 1: save the snapshot / save it and continue from it.
    if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw, bh), "Save last good state", st(has_snap)) {
        if let Some(sn) = &g.r10.snap {
            result = Some(match save_crash_world(&sn.world, sn.seed) {
                Ok(i) => (format!("Saved {} ({})", i.path.display(), crate::save::fmt_bytes(i.bytes)), true),
                Err(e) => (format!("Save failed: {e}"), false),
            });
        }
    }
    if text_button(&mut g.ui, &g.font, Rect::new(x + bw + 8.0 * s, y, bw, bh), "Save it and continue from there", st(has_snap)) {
        if let Some(sn) = g.r10.snap.take() {
            match save_crash_world(&sn.world, sn.seed) {
                Ok(i) => {
                    crate::crash::append_log(&format!("--- continued from the last good state {}", i.path.display()));
                    g.sim.clear_crash();
                    g.load_save(&i.path);
                    g.paused = true;
                    return;
                }
                Err(e) => {
                    result = Some((format!("Save failed: {e}"), false));
                    g.r10.snap = Some(sn);
                }
            }
        }
    }
    y += bh + 8.0 * s;
    // Row 2: the current state.
    if cr.origin == "simulation" {
        if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw, bh), "Save current state (may be broken)", BtnState::Normal) {
            result = Some(match save_crash_world(&g.sim.world, g.seed) {
                Ok(i) => (format!("Saved {} ({})", i.path.display(), crate::save::fmt_bytes(i.bytes)), true),
                Err(e) => (format!("Save failed: {e}"), false),
            });
        }
    } else {
        if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw, bh), "Save now", BtnState::Normal) {
            result = Some(match crate::save::save_world(&g.sim.world, g.seed, &crate::save::save_dir()) {
                Ok(i) => (format!("Saved {} ({})", i.path.display(), crate::save::fmt_bytes(i.bytes)), true),
                Err(e) => (format!("Save failed: {e}"), false),
            });
        }
        if text_button(&mut g.ui, &g.font, Rect::new(x + bw + 8.0 * s, y, bw, bh), "Try to continue", BtnState::Normal) {
            // Close the screens that were open; the panicking one is likely among them.
            g.screen = Screen::None;
            g.menu_open = false;
            g.selected = Selected::None;
            g.tool = Tool::None;
            g.submenu = Submenu::None;
            g.drag = None;
            g.r10.ctx = None;
            g.r10.crash = None;
            return;
        }
    }
    y += bh + 8.0 * s;
    // Row 3: leave.
    if text_button(&mut g.ui, &g.font, Rect::new(x, y, bw, bh), "Return to title", BtnState::Normal) {
        g.exit = Some(crate::game::Exit::MainMenu);
    }
    if text_button(&mut g.ui, &g.font, Rect::new(x + bw + 8.0 * s, y, bw, bh), "Quit", BtnState::Normal) {
        g.quit = true;
    }
    y += bh + 12.0 * s;
    if let Some(res) = result {
        crate::crash::append_log(&format!("--- crash dialog: {}", res.0));
        if let Some(c) = g.r10.crash.as_mut() {
            c.result = Some(res);
        }
    }
    if let Some((t, ok)) = g.r10.crash.as_ref().and_then(|c| c.result.clone()) {
        for l in super::round9::wrap(&t, maxc).iter().take(3) {
            g.font.draw(l, x, y, s, if ok { col::TEXT_GOOD } else { col::TEXT_BAD });
            y += lh;
        }
    }
    let _ = y;
}

// ====================================================================== glue

/// The unit/building sheet is not drawn under the round-10 screens.
pub fn hides_sheet(g: &Game) -> bool {
    matches!(g.screen, Screen::Tasks | Screen::Hauling)
}

/// Draw the round-10 screens and overlays (from `overlay::draw_ui`).
pub fn draw_screens(g: &mut Game) {
    if std::mem::take(&mut g.r10.test_panic) {
        panic!("test panic in the interface (--ui crash:ui)");
    }
    match g.screen {
        Screen::Tasks => draw_tasks(g),
        Screen::Hauling => super::round12::draw_hauling(g), // [app12] the route editor
        _ => {}
    }
    if g.submenu == Submenu::Build && g.screen == Screen::None && !g.menu_open {
        draw_build_keys(g);
    }
    if g.screen == Screen::None {
        draw_ctx(g);
    } else {
        g.r10.ctx = None;
    }
}

/// `--ui` states of round 10 (screenshots/tests). Returns false for unknown keys.
pub fn ui_arg(g: &mut Game, k: &str, v: &str) -> bool {
    match k {
        "tasks" => {
            g.toggle_screen(Screen::None);
            g.screen = Screen::Tasks;
            g.r10.task_tab = v.parse::<usize>().unwrap_or(0).min(TASK_TABS.len() - 1);
            g.r10.tasks_dirty = true;
        }
        "hauling" => {
            g.toggle_screen(Screen::None);
            g.screen = Screen::Hauling;
        }
        "fake-route" => {
            // A made-up route between the first two stockpiles (screenshots).
            let piles: Vec<(i32, Pos)> = g.sim.world.buildings.iter().filter(|b| b.kind == "Stockpile").take(2).map(|b| (b.id, b.center)).collect();
            if let (Some(a), Some(b)) = (piles.first(), piles.get(1)) {
                g.sim.world.haul_routes.push(rf_core::HaulRoute {
                    id: 0,
                    name: "Stone to the masons".into(),
                    stops: vec![
                        rf_core::HaulStop { name: "Quarry".into(), pos: a.1, take_from: vec![a.0], give_to: vec![], conditions: vec!["Leave when full".into()] },
                        rf_core::HaulStop { name: "Workshop pile".into(), pos: b.1, take_from: vec![], give_to: vec![b.0], conditions: vec!["Wait 1 day".into(), "Leave when empty".into()] },
                    ],
                    vehicles: vec![],
                });
            }
        }
        "ctx" => {
            // Context menu for the tile at the camera center (a scripted `rclick` tests the
            // real path).
            let p = Pos::new(g.cam.x as i16, g.cam.y as i16, g.cam.z as i16);
            let items = context_items(g, p);
            g.r10.ctx = Some(Ctx { pos: p, at: vec2(screen_width() / 2.0, screen_height() / 2.0), items });
        }
        "bpath" => {
            // Build menu at a chain position: `bpath:Workshops/Farming`.
            g.open_build();
            g.r10.bpath = Vec::new();
            for part in v.split('/').filter(|p| !p.is_empty()) {
                if let Some(BTarget::Group(name, tab)) = BUILD_KEYS.iter().map(|e| e.2).find(|t| matches!(t, BTarget::Group(n, _) if *n == part)) {
                    g.r10.bpath.push(name);
                    set_tab(g, tab);
                }
            }
            g.r10.bcat_seen = Some(g.build_cat);
        }
        "engraved" => {
            // Select the tile of the first engraving on the camera's level (or anywhere).
            let z = g.cam.z as i16;
            let e = g.sim.world.engravings.iter().find(|e| e.pos.z == z && e.quality >= 3).or_else(|| g.sim.world.engravings.iter().find(|e| e.quality >= 3)).map(|e| e.pos);
            if let Some(p) = e {
                g.selected = Selected::Tile(p);
                g.cam.x = p.x as f32 + 0.5;
                g.cam.y = p.y as f32 + 0.5;
                g.cam.tx = g.cam.x;
                g.cam.ty = g.cam.y;
                g.cam.z = p.z as i32;
            }
        }
        "items-tile" => {
            // Select the tile with the most items on the camera's level.
            let z = g.cam.z as i16;
            let mut best: Option<(usize, Pos)> = None;
            let mut counts: std::collections::BTreeMap<(i16, i16), usize> = Default::default();
            for it in g.sim.world.items.iter().filter(|it| it.pos.z == z && it.flags.on_ground) {
                *counts.entry((it.pos.x, it.pos.y)).or_default() += 1;
            }
            for ((x, y), n) in counts {
                if best.map(|b| n > b.0).unwrap_or(true) {
                    best = Some((n, Pos::new(x, y, z)));
                }
            }
            if let Some((_, p)) = best {
                g.selected = Selected::Tile(p);
                g.cam.focus(p);
                g.cam.x = g.cam.tx;
                g.cam.y = g.cam.ty;
            }
        }
        "crash" => match v {
            // Real panics through the real paths: `crash:sim` breaks the world behind the sim's
            // back so the next tick panics on the worker; `crash:ui` panics in the interface.
            "sim" => {
                take_snapshot(g);
                g.sim.world.units.clear();
                g.paused = false;
            }
            "ui" => g.r10.test_panic = true,
            // Only the dialog (no panic).
            _ => {
                take_snapshot(g);
                on_crash(g, "simulation", "test crash requested with --ui crash".into());
            }
        },
        _ => return false,
    }
    true
}

// ====================================================================== title-screen announcements

/// State of the title screen's announcement page.
#[derive(Default)]
pub struct AnnPage {
    pub flags: Vec<AnnFlags>,
    pub base: Vec<AnnFlags>,
    pub src: Vec<Src>,
    pub scroll: usize,
}

impl AnnPage {
    /// Resolve the flags from the DF files (read-only) and the saved changes.
    pub fn ensure(&mut self, settings: &crate::settings::Settings) {
        if self.flags.len() == ANN_TYPES.len() {
            return;
        }
        let df = rf_gfx::df_dir();
        let texts: Vec<(String, Src)> = super::round9::df_files(&df).into_iter().filter_map(|(p, s)| std::fs::read_to_string(p).ok().map(|t| (t, s))).collect();
        let base = super::round9::df_defaults(&texts);
        self.base = base.iter().map(|b| b.0).collect();
        self.flags.clear();
        self.src.clear();
        for (i, t) in ANN_TYPES.iter().enumerate() {
            let (mut f, mut s) = base[i];
            if let Some(o) = settings.announcements.iter().find(|o| o.id == t.id) {
                let of = AnnFlags { popup: o.popup, pause: o.pause, recenter: o.recenter, show: o.show };
                if of != f {
                    f = of;
                    s = Src::Changed;
                }
            }
            self.flags.push(f);
            self.src.push(s);
        }
    }

    fn set(&mut self, settings: &mut crate::settings::Settings, t: usize, f: AnnFlags) {
        let id = ANN_TYPES[t].id;
        settings.announcements.retain(|o| o.id != id);
        if f != self.base[t] {
            settings.announcements.push(crate::settings::AnnSetting { id: id.into(), popup: f.popup, pause: f.pause, recenter: f.recenter, show: f.show });
        }
        self.flags.clear();
        self.ensure(settings);
    }
}

/// The announcement table on the title screen (same as Esc > Settings > Announcements in the
/// game). Returns true when Back was clicked.
pub fn draw_ann_page(ui: &mut UiCtx, font: &Font, input: &mut crate::keys::InputFrame, settings: &mut crate::settings::Settings, st: &mut AnnPage) -> bool {
    st.ensure(settings);
    let s = ui.scale;
    let lh = ui.line_h();
    let (sw, sh) = (screen_width(), screen_height());
    let (w, h) = ((820.0 * s).min(sw - 40.0), (620.0 * s).min(sh - 60.0));
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    panel(ui, r);
    let pad = 16.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    font.draw("Announcements", x, y, 1.5 * s, col::TEXT_HI);
    y += 1.5 * GH * s + 8.0 * s;
    let maxc = ((w - 2.0 * pad) / (GW * s)) as usize;
    font.draw(&clip("Like DF's announcements.txt: Box = pop-up that pauses (BOX), Pause (P), Center (R), Log (D_D).", maxc), x, y, s, col::TEXT_DIM);
    y += lh;
    font.draw(&clip("Defaults come from your DF prefs/announcements.txt; changes are saved in saves/settings.json.", maxc), x, y, s, col::TEXT_DIM);
    y += lh + 8.0 * s;
    let cw = w - 2.0 * pad;
    let cx = [x, x + 0.40 * cw, x + 0.50 * cw, x + 0.60 * cw, x + 0.71 * cw, x + 0.80 * cw];
    for (i, hd) in ["Event", "Box", "Pause", "Center", "Log", "Default from"].iter().enumerate() {
        font.draw(hd, cx[i], y, s, col::BORDER_HI);
    }
    y += lh;
    draw_line(x, y, r.x + r.w - pad, y, 1.0, col::BORDER);
    y += 4.0 * s;
    let row_h = lh + 4.0 * s;
    let area = Rect::new(x, y, cw, r.y + r.h - pad - 34.0 * s - y);
    let rows = ((area.h / row_h) as usize).max(1);
    let step = super::wheel_over(ui, input, area);
    st.scroll = (st.scroll as i32 + step * 3).clamp(0, ANN_TYPES.len().saturating_sub(rows) as i32) as usize;
    let mut change = None;
    for (i, t) in ANN_TYPES.iter().enumerate().skip(st.scroll).take(rows) {
        if i % 2 == 0 {
            draw_rectangle(x - 4.0 * s, y - 2.0 * s, area.w + 8.0 * s, row_h, Color::new(1.0, 1.0, 1.0, 0.03));
        }
        let f = st.flags[i];
        font.draw(&clip(t.label, 36), cx[0], y, s, col::TEXT);
        let mut nf = f;
        if checkbox(ui, font, cx[1], y, f.popup, "") {
            nf.popup = !f.popup;
        }
        if checkbox(ui, font, cx[2], y, f.pause, "") {
            nf.pause = !f.pause;
        }
        if checkbox(ui, font, cx[3], y, f.recenter, "") {
            nf.recenter = !f.recenter;
        }
        if checkbox(ui, font, cx[4], y, f.show, "") {
            nf.show = !f.show;
        }
        if nf != f {
            change = Some((i, nf));
        }
        let (src, sc) = match st.src[i] {
            Src::DfPrefs => ("your DF prefs", col::TEXT_GOOD),
            Src::DfDefault => ("DF default", col::TEXT_DIM),
            Src::Changed => ("changed here", col::TEXT_HI),
            Src::Rustfort => ("Rustfort", col::TEXT_DIM),
        };
        font.draw(&clip(src, ((r.x + r.w - pad - cx[5]) / (GW * s)) as usize), cx[5], y, s, sc);
        y += row_h;
    }
    if let Some((i, f)) = change {
        st.set(settings, i, f);
    }
    let bh = 26.0 * s;
    let by = r.y + r.h - pad - bh;
    let back = text_button(ui, font, Rect::new(x, by, 14.0 * GW * s, bh), "Back", BtnState::Normal);
    if ANN_TYPES.len() > rows {
        let t = format!("{}-{} of {} (wheel scrolls)", st.scroll + 1, (st.scroll + rows).min(ANN_TYPES.len()), ANN_TYPES.len());
        font.draw(&t, r.x + r.w - pad - font.width(&t, s), by + 6.0 * s, s, col::TEXT_DIM);
    }
    let changed = !settings.announcements.is_empty();
    if text_button(ui, font, Rect::new(x + 16.0 * GW * s, by, 24.0 * GW * s, bh), "Reset to DF values", if changed { BtnState::Normal } else { BtnState::Disabled }) {
        settings.announcements.clear();
        st.flags.clear();
        st.ensure(settings);
    }
    back
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_categories() {
        assert_eq!(activity_cat("Store Item in Stockpile"), TaskCat::Hauling);
        assert_eq!(activity_cat("Suture"), TaskCat::Medical);
        assert_eq!(activity_cat("Train"), TaskCat::Military);
        assert_eq!(activity_cat("Sleep"), TaskCat::Personal);
        assert_eq!(activity_cat("Construct Wall"), TaskCat::Building);
        assert_eq!(activity_cat("Brew drink from plant"), TaskCat::Other);
        assert_eq!(construction_ref("Smooth Wall"), (TaskCat::Designation, TaskRef::Detail));
        assert_eq!(construction_ref("Construct Floor"), (TaskCat::Building, TaskRef::Construction));
        assert_eq!(construction_ref("Remove Construction"), (TaskCat::Building, TaskRef::ConstructionOther));
    }

    #[test]
    fn build_tree_is_consistent() {
        // Every group a key opens has entries, and no group binds one key twice except the
        // chair/door pair DF itself puts on `r`.
        let b = crate::keys::Bindings::load(&rf_gfx::df_dir());
        let groups: Vec<&str> = BUILD_KEYS.iter().filter_map(|(_, _, t)| if let BTarget::Group(n, _) = t { Some(*n) } else { None }).collect();
        for gname in &groups {
            assert!(BUILD_KEYS.iter().any(|(gr, _, _)| gr == gname), "group {gname} is empty");
        }
        for gname in std::iter::once(&"").chain(groups.iter()) {
            let mut seen: Vec<(String, &str)> = Vec::new();
            for (gr, bind, _) in BUILD_KEYS.iter().filter(|(gr, _, _)| gr == gname) {
                let _ = gr;
                for t in b.triggers(bind) {
                    let l = crate::keys::trigger_label(t);
                    if let Some((_, other)) = seen.iter().find(|(k, _)| *k == l) {
                        let pair = [*other, *bind];
                        assert!(pair.contains(&"HOTKEY_BUILDING_CHAIR") && pair.contains(&"HOTKEY_BUILDING_DOOR"), "{gname}: {l} used by {other} and {bind}");
                    }
                    seen.push((l, bind));
                }
            }
        }
    }

    #[test]
    fn tasks_of_a_synthetic_fort() {
        let mut w = World::synthetic(32, 32, 12);
        w.buildings.push(rf_core::Building {
            id: 900,
            kind: "Workshop".into(),
            subtype: "Still".into(),
            x1: 2,
            y1: 2,
            x2: 4,
            y2: 4,
            z: 6,
            center: Pos::new(3, 3, 6),
            built: true,
            jobs: vec![rf_core::BuildingJob { job: "CustomReaction".into(), reaction: "BREW_DRINK_FROM_PLANT".into(), order_id: -1, ..Default::default() }],
            ..Default::default()
        });
        let sim = rf_sim::Sim::new(w, 1);
        // `collect_tasks` needs a Game; check the parts it is built from instead.
        let t = sim.order_templates();
        let b = sim.world.building(900).unwrap();
        assert_eq!(super::super::building::job_text(&t, &b.jobs[0]), "Brew drink from plant");
        assert!(inside(b, Pos::new(4, 2, 6)));
        assert!(!inside(b, Pos::new(5, 2, 6)));
    }

    #[test]
    fn engraving_words() {
        assert_eq!(QUALITY[5].0, "masterful");
        assert_eq!(motif(Pos::new(3, 4, 5)), motif(Pos::new(3, 4, 5)));
    }
}
