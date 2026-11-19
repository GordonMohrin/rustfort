//! [audio14] Settings > Audio: mute, the five DF volumes (shown as 0-100 %), the average
//! time between songs, the optional extra sounds, what plays now and where the sounds come
//! from. Shared by the title screen and the in-game Esc menu.

use super::{checkbox, clip, col, panel, text_button, BtnState, UiCtx};
use crate::audio::settings::pct;
use crate::font::{Font, GH, GW};
use crate::settings::Settings;
use macroquad::prelude::*;

/// Draw the page. Returns (back clicked, settings changed).
pub fn draw_audio_page(ui: &mut UiCtx, font: &Font, settings: &mut Settings) -> (bool, bool) {
    let s = ui.scale;
    let lh = ui.line_h();
    let (sw, sh) = (screen_width(), screen_height());
    let (w, h) = ((720.0 * s).min(sw - 40.0), (520.0 * s).min(sh - 60.0));
    let r = Rect::new((sw - w) / 2.0, (sh - h) / 2.0, w, h);
    panel(ui, r);
    let pad = 16.0 * s;
    let (x, mut y) = (r.x + pad, r.y + pad);
    font.draw("Audio", x, y, 1.5 * s, col::TEXT_HI);
    y += 1.5 * GH * s + 10.0 * s;
    let maxc = ((w - 2.0 * pad) / (GW * s)) as usize;
    let st = crate::audio::status();
    let df = st.df.clone().unwrap_or_default();
    let a = &mut settings.audio;
    let eff = a.effective(&df);
    let mut changed = false;
    let bh = GH * s + 10.0 * s;
    let vx = x + 250.0 * s;
    // Mute.
    font.draw("Sound", x, y + 5.0 * s, s, col::TEXT);
    if checkbox(ui, font, vx, y + 5.0 * s, eff.muted, "Mute all sound") {
        a.toggle_mute(&df);
        changed = true;
    }
    y += bh + 6.0 * s;
    // Volumes: - value + (5 % steps).
    let rows: [(&str, u8); 5] = [("Master volume", eff.master), ("Music", eff.music), ("Ambience", eff.ambience), ("Sound effects", eff.sfx), ("Interface", eff.ui)];
    for (k, (label, v)) in rows.iter().enumerate() {
        font.draw(label, x, y + 5.0 * s, s, col::TEXT);
        let mut bx = vx;
        let mut nv: Option<u8> = None;
        if text_button(ui, font, Rect::new(bx, y, 34.0 * s, bh), "-", BtnState::Normal) {
            nv = Some(v.saturating_sub(13));
        }
        bx += 38.0 * s;
        let t = format!("{} %", pct(*v));
        let tw = 8.0 * GW * s;
        font.draw(&t, bx + (tw - font.width(&t, s)) / 2.0, y + 5.0 * s, s, col::TEXT_HI);
        bx += tw;
        if text_button(ui, font, Rect::new(bx, y, 34.0 * s, bh), "+", BtnState::Normal) {
            nv = Some(v.saturating_add(13));
        }
        // A bar for the level.
        let barx = bx + 44.0 * s;
        let barw = (r.x + w - pad - barx).max(40.0 * s);
        draw_rectangle(barx, y + 6.0 * s, barw, bh - 12.0 * s, Color::new(0.08, 0.08, 0.09, 1.0));
        draw_rectangle(barx, y + 6.0 * s, barw * *v as f32 / 255.0, bh - 12.0 * s, col::BUTTON_ACTIVE);
        if let Some(n) = nv {
            a.take_over(&df);
            match k {
                0 => a.master = n,
                1 => a.music = n,
                2 => a.ambience = n,
                3 => a.sfx = n,
                _ => a.ui = n,
            }
            changed = true;
        }
        y += bh + 4.0 * s;
    }
    y += 6.0 * s;
    font.draw("Average time between songs", x, y + 5.0 * s, s, col::TEXT);
    let mut bx = vx;
    let avg = eff.avg_song_secs;
    let mut na = None;
    if text_button(ui, font, Rect::new(bx, y, 34.0 * s, bh), "-", BtnState::Normal) {
        na = Some(avg.saturating_sub(10).max(10));
    }
    bx += 38.0 * s;
    let t = format!("{avg} s");
    let tw = 8.0 * GW * s;
    font.draw(&t, bx + (tw - font.width(&t, s)) / 2.0, y + 5.0 * s, s, col::TEXT_HI);
    bx += tw;
    if text_button(ui, font, Rect::new(bx, y, 34.0 * s, bh), "+", BtnState::Normal) {
        na = Some((avg + 10).min(600));
    }
    font.draw(&format!("(songs every {}-{} s)", avg as u32 * 3 / 4, avg as u32 * 5 / 4), bx + 44.0 * s, y + 5.0 * s, s, col::TEXT_DIM);
    if let Some(n) = na {
        a.take_over(&df);
        a.avg_song_secs = n;
        changed = true;
    }
    y += bh + 8.0 * s;
    if checkbox(ui, font, vx, y, a.extra_sfx, "Extra sounds (adventure-mode combat hits)") {
        a.extra_sfx = !a.extra_sfx;
        changed = true;
    }
    font.draw("Rustfort extra", x, y, s, col::TEXT);
    y += lh + 10.0 * s;
    if !a.from_df {
        if text_button(ui, font, Rect::new(vx, y, 22.0 * GW * s, bh), "Use the DF values again", BtnState::Normal) {
            a.from_df = true;
            changed = true;
        }
        y += bh + 8.0 * s;
    }
    // Status lines.
    draw_line(x, y, r.x + w - pad, y, 1.0, col::BORDER);
    y += 8.0 * s;
    let src = if a.from_df {
        if st.df_source.is_empty() {
            "Volumes: DF's defaults (no DF init file found)".to_string()
        } else {
            format!("Volumes: from your DF settings ({})", st.df_source)
        }
    } else {
        "Volumes: changed in Rustfort (saved in settings.json)".to_string()
    };
    for (t, c) in [
        (format!("Audio: {}", st.line), if st.line == "on" { col::TEXT_GOOD } else { col::TEXT_HI }),
        (if st.now_playing.is_empty() { "Now playing: -".to_string() } else { format!("Now playing: {}", st.now_playing) }, col::TEXT),
        (src, col::TEXT_DIM),
        (format!("Sound folder: {} ({} files, read from your DF install)", st.sound_dir, st.n_files), col::TEXT_DIM),
        ("--mute or RUSTFORT_MUTE=1 start without sound; screenshot and script runs are always silent.".to_string(), col::TEXT_DIM),
    ] {
        font.draw(&clip(&t, maxc), x, y, s, c);
        y += lh;
    }
    if changed {
        crate::audio::apply(&settings.audio);
    }
    let bh2 = 26.0 * s;
    let back = text_button(ui, font, Rect::new(x, r.y + h - pad - bh2, 14.0 * GW * s, bh2), "Back", BtnState::Normal);
    (back, changed)
}
