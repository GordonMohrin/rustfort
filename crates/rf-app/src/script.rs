//! Scripted input for screenshot tests: `--script "5:move 700,400; 6:ldown; 9:move 800,450; 10:lup"`.
//! When a script (or `--mouse`) is given, the real mouse and keyboard are ignored, so runs are
//! reproducible.
//!
//! Commands: `move x,y`, `click x,y`, `ldown`, `lup`, `rclick`, `mdown`, `mup`, `wheel n`,
//! `key k` (a-z, 0-9, esc, space, enter, pgup, pgdn, period, backspace, tab, f5, f9), `char c`,
//! `mods shift|ctrl|none`.

use crate::keys::{InputFrame, CTRL, SHIFT};
use macroquad::prelude::*;

#[derive(Clone, Debug, PartialEq)]
enum Ev {
    Move(f32, f32),
    Click(f32, f32),
    LDown,
    LUp,
    RClick,
    MDown,
    MUp,
    Wheel(i32),
    Key(KeyCode),
    Char(char),
    Mods(u8),
}

#[derive(Default)]
pub struct Script {
    events: Vec<(u32, Ev)>,
    mouse: Vec2,
    ldown: bool,
    mdown: bool,
    mods: u8,
    release_next: bool,
}

fn key(name: &str) -> Option<KeyCode> {
    use KeyCode as K;
    Some(match name {
        "esc" => K::Escape,
        "space" => K::Space,
        "enter" => K::Enter,
        "pgup" => K::PageUp,
        "pgdn" => K::PageDown,
        "period" => K::Period,
        "backspace" => K::Backspace,
        "tab" => K::Tab,
        "f5" => K::F5,
        "f9" => K::F9,
        "up" => K::Up,
        "down" => K::Down,
        "left" => K::Left,
        "right" => K::Right,
        s if s.len() == 1 => {
            let c = s.chars().next()?;
            let m = crate::keys::parse(&format!("[BIND:X:REPEAT_NOT][SYM:0:{c}]"));
            match m.get("X")?.first()? {
                crate::keys::Trigger::Key { key, .. } => *key,
                _ => return None,
            }
        }
        _ => return None,
    })
}

impl Script {
    pub fn parse(s: &str, start_mouse: Option<(f32, f32)>) -> Result<Script, String> {
        let mut sc = Script { mouse: start_mouse.map(|(x, y)| vec2(x, y)).unwrap_or(vec2(800.0, 450.0)), ..Default::default() };
        for part in s.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            let (f, cmd) = part.split_once(':').ok_or_else(|| format!("script: '{part}' needs frame:command"))?;
            let frame: u32 = f.trim().parse().map_err(|e| format!("script frame '{f}': {e}"))?;
            let cmd = cmd.trim();
            let (name, arg) = cmd.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((cmd, ""));
            let xy = || -> Result<(f32, f32), String> {
                let v: Vec<f32> = arg.split(',').filter_map(|t| t.trim().parse().ok()).collect();
                (v.len() == 2).then(|| (v[0], v[1])).ok_or_else(|| format!("script: '{cmd}' needs x,y"))
            };
            let ev = match name {
                "move" => {
                    let (x, y) = xy()?;
                    Ev::Move(x, y)
                }
                "click" => {
                    let (x, y) = xy()?;
                    Ev::Click(x, y)
                }
                "ldown" => Ev::LDown,
                "lup" => Ev::LUp,
                "rclick" => Ev::RClick,
                "mdown" => Ev::MDown,
                "mup" => Ev::MUp,
                "wheel" => Ev::Wheel(arg.parse().map_err(|e| format!("script wheel: {e}"))?),
                "key" => Ev::Key(key(arg).ok_or_else(|| format!("script: unknown key '{arg}'"))?),
                "char" => Ev::Char(arg.chars().next().ok_or("script: char needs a character")?),
                "mods" => Ev::Mods(match arg {
                    "shift" => SHIFT,
                    "ctrl" => CTRL,
                    _ => 0,
                }),
                _ => return Err(format!("script: unknown command '{name}'")),
            };
            sc.events.push((frame, ev));
        }
        Ok(sc)
    }

    /// Replace the real input of this frame with the scripted one.
    pub fn apply(&mut self, frame: u32, f: &mut InputFrame) {
        let mut out = InputFrame { mods: self.mods, ..Default::default() };
        if self.release_next {
            self.release_next = false;
            self.ldown = false;
            out.lreleased = true;
        }
        for (_, ev) in self.events.iter().filter(|(fr, _)| *fr == frame) {
            match ev {
                Ev::Move(x, y) => self.mouse = vec2(*x, *y),
                Ev::Click(x, y) => {
                    self.mouse = vec2(*x, *y);
                    out.lpressed = true;
                    self.ldown = true;
                    self.release_next = true;
                }
                Ev::LDown => {
                    out.lpressed = true;
                    self.ldown = true;
                }
                Ev::LUp => {
                    out.lreleased = true;
                    self.ldown = false;
                }
                Ev::RClick => out.rpressed = true,
                Ev::MDown => {
                    out.mpressed = true;
                    self.mdown = true;
                }
                Ev::MUp => self.mdown = false,
                Ev::Wheel(n) => out.wheel = n.signum(),
                Ev::Key(k) => out.fired.push(*k),
                Ev::Char(c) => out.chars.push(*c),
                Ev::Mods(m) => {
                    self.mods = *m;
                    out.mods = *m;
                }
            }
        }
        out.mouse = self.mouse;
        out.ldown = self.ldown;
        out.mdown = self.mdown;
        *f = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_replays() {
        let mut s = Script::parse("1:move 10,20; 2:ldown; 3:move 30,40; 4:lup; 5:key m; 6:wheel -1; 7:click 5,5", None).unwrap();
        let mut f = InputFrame::default();
        s.apply(1, &mut f);
        assert_eq!(f.mouse, vec2(10.0, 20.0));
        s.apply(2, &mut f);
        assert!(f.lpressed && f.ldown);
        s.apply(3, &mut f);
        assert!(!f.lpressed && f.ldown);
        s.apply(4, &mut f);
        assert!(f.lreleased && !f.ldown);
        s.apply(5, &mut f);
        assert_eq!(f.fired, vec![KeyCode::M]);
        s.apply(6, &mut f);
        assert_eq!(f.wheel, -1);
        s.apply(7, &mut f);
        assert!(f.lpressed);
        s.apply(8, &mut f);
        assert!(f.lreleased);
        assert!(Script::parse("x", None).is_err());
    }
}
