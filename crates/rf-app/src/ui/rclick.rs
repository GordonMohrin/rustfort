//! [feel-ui] Right-click backs out like DF Steam: it closes the topmost thing that is open,
//! one per click, anywhere in the window:
//!
//! 1. a modal popup, dialog or box (help, hostile alert, caravan box, announcement boxes, the
//!    map context menu, the Esc menu, pickers and name fields, new work order, move goods,
//!    appoint, add job, crop season), and the "pick on the map" modes that belong to a
//!    screen or sheet (lever linking, stockpile links, squad station/kill/patrol: back to
//!    where they came from; a patrol route loses its last waypoint first);
//! 2. a big screen (orders, labor, squads, reports, places, justice, objects, world, tasks,
//!    hauling, petitions, trade, nobles, animals, stockpile settings) and the creatures list;
//! 3. the side panel (unit, building, zone or tile sheet);
//! 4. on the map: the pending drag box, then the tool (build, zone, burrow: back to their
//!    menu, a hotkey chain one level up), then the menu (dig, build, zone, stockpile,
//!    smooth, traffic, items, burrows);
//! 5. the patrol route display.
//!
//! Only when nothing is open does a right-click on the map open the context menu
//! (`round10::context_click`, see [`map_right_click`]).

use crate::game::{Game, MenuPage, Screen, Selected, Submenu, Tool};

/// Call once per frame after the global keys, before the interface is drawn. A right-click
/// that closed something is consumed (the interface and the map do not see it).
pub fn frame(g: &mut Game) {
    if g.input.rpressed && back(g) {
        g.input.rpressed = false;
    }
}

/// Close the topmost open thing (see the module docs). False when nothing was open.
pub fn back(g: &mut Game) -> bool {
    modal(g) || big_screen(g) || side_panel(g) || map_tool(g) || g.r9.show_route.take().is_some()
}

fn modal(g: &mut Game) -> bool {
    if g.help_open {
        g.help_open = false;
    } else if g.alert.is_some() {
        g.alert = None;
    } else if g.r4.popup.is_some() {
        g.r4.popup = None;
    } else if g.r9.popups.pop_front().is_some() {
        // the front announcement box
    } else if g.r10.ctx.take().is_some() {
        // the map context menu
    } else if g.menu_open {
        match g.menu.page {
            MenuPage::Main => g.menu_open = false,
            MenuPage::Announcements => g.menu.page = MenuPage::Settings,
            _ => g.menu.page = MenuPage::Main,
        }
    } else if !matches!(g.tool, Tool::PileLink(..)) && crate::ui::round8::escape(g) {
        // pickers, the work detail name field and editor, convict, location pickers
    } else if g.screen == Screen::Military && (g.mil.picker.is_some() || g.mil.kill_list) {
        g.mil.picker = None;
        g.mil.kill_list = false;
    } else if g.screen == Screen::Military && g.r9.sq_tab == 1 && g.r9.eq_pos.is_some() {
        g.r9.eq_pos = None;
    } else if g.screen == Screen::Trade && g.r4.depot_open {
        g.r4.depot_open = false;
        g.r4.depot_typing = false;
    } else if g.screen == Screen::Nobles && g.r4.appoint.is_some() {
        g.r4.appoint = None;
    } else if g.orders.new_open {
        g.orders.new_open = false;
    } else if g.labor.typing {
        g.labor.typing = false;
    } else if g.bui.add_open || g.bui.crop_season.is_some() {
        g.bui.add_open = false;
        g.bui.crop_season = None;
    } else {
        return pick_mode(g);
    }
    true
}

/// Map pick modes that belong to a screen or sheet: back to it.
fn pick_mode(g: &mut Game) -> bool {
    match g.tool {
        Tool::Link(_) => g.tool = Tool::None,
        Tool::PileLink(id, _) => crate::ui::round8::open_pile(g, id),
        Tool::Patrol(_) => crate::ui::round9::patrol_back(g),
        Tool::SquadStation(_) | Tool::SquadKill(_) => {
            g.tool = Tool::None;
            g.screen = Screen::Military;
        }
        _ => return false,
    }
    g.drag = None;
    true
}

fn big_screen(g: &mut Game) -> bool {
    if g.screen != Screen::None {
        g.screen = Screen::None;
        g.labor.typing = false;
        g.mil.picker = None;
        g.mil.kill_list = false;
    } else if g.citizens_open {
        g.citizens_open = false;
    } else {
        return false;
    }
    true
}

fn side_panel(g: &mut Game) -> bool {
    if g.selected == Selected::None {
        return false;
    }
    g.selected = Selected::None;
    g.bui.add_open = false;
    g.bui.crop_season = None;
    true
}

fn map_tool(g: &mut Game) -> bool {
    if g.drag.is_some() {
        g.drag = None;
        return true;
    }
    // Build menu: placing -> list -> one hotkey-chain level up (round 10's Esc).
    if g.submenu == Submenu::Build && crate::ui::round10::escape(g) {
        return true;
    }
    match g.tool {
        Tool::None => {}
        Tool::Build(_) | Tool::Construct(_) | Tool::Zone(_) | Tool::Burrow if g.submenu != Submenu::None => {
            // Back to the build/zone/burrow menu, like DF.
            g.tool = Tool::None;
            return true;
        }
        _ => {
            // Dig, stockpile, smooth, traffic and item tools are their menu.
            g.tool = Tool::None;
            g.submenu = Submenu::None;
            return true;
        }
    }
    if g.submenu != Submenu::None {
        g.submenu = Submenu::None;
        g.build_check = None;
        return true;
    }
    false
}

/// The map part of a right-click that nothing consumed (no window, tool or menu open): the
/// context menu of the tile under the mouse, or nothing for a bare tile.
pub fn map_right_click(g: &mut Game) {
    if !crate::ui::round10::context_click(g) {
        g.selected = Selected::None;
    }
}

