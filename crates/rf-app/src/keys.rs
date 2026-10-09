//! Keybindings read from DF's own `interface.txt` (the user's active prefs first, so Rustfort
//! reacts to keys and the mouse wheel exactly like their Steam DF does).
//!
//! Format: `[BIND:NAME:REPEAT_x]` followed by `[SYM:mods:Key]`, `[KEY:c]`, `[MOUSEWHEEL:mods:UP|DOWN]`.
//! mods bits: 1 = Shift, 2 = Ctrl, 4 = Alt.

use macroquad::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const SHIFT: u8 = 1;
pub const CTRL: u8 = 2;
pub const ALT: u8 = 4;

#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    Key { mods: u8, key: KeyCode },
    /// A typed character (layout independent, e.g. `<` and `>`).
    Char(char),
    Wheel { mods: u8, up: bool },
}

#[derive(Default)]
pub struct Bindings {
    map: HashMap<String, Vec<Trigger>>,
    /// Which file the bindings came from.
    pub source: String,
    pub hold_ms: f64,
    pub repeat_ms: f64,
}

/// Candidate interface.txt files, most specific first.
pub fn interface_files(df_dir: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(app) = std::env::var_os("APPDATA") {
        v.push(PathBuf::from(app).join("Bay 12 Games").join("Dwarf Fortress").join("prefs").join("interface.txt"));
    }
    v.push(df_dir.join("prefs").join("interface.txt"));
    v.push(df_dir.join("data").join("init").join("interface.txt"));
    v
}

fn sym_key(name: &str) -> Option<KeyCode> {
    use KeyCode as K;
    let k = match name {
        "Enter" => K::Enter,
        "ESC" => K::Escape,
        "Space" => K::Space,
        "Tab" => K::Tab,
        "Up" => K::Up,
        "Down" => K::Down,
        "Left" => K::Left,
        "Right" => K::Right,
        "Page Up" => K::PageUp,
        "Page Down" => K::PageDown,
        "Home" => K::Home,
        "End" => K::End,
        "Insert" => K::Insert,
        "Delete" => K::Delete,
        "Backspace" => K::Backspace,
        "Leftbracket" => K::LeftBracket,
        "Rightbracket" => K::RightBracket,
        "Minus" => K::Minus,
        "Equals" => K::Equal,
        "Comma" => K::Comma,
        "Period" => K::Period,
        "Slash" => K::Slash,
        "Backslash" => K::Backslash,
        "Numpad Plus" => K::KpAdd,
        "Numpad Minus" => K::KpSubtract,
        "Numpad Enter" => K::KpEnter,
        "F1" => K::F1,
        "F2" => K::F2,
        "F3" => K::F3,
        "F4" => K::F4,
        "F5" => K::F5,
        "F6" => K::F6,
        "F7" => K::F7,
        "F8" => K::F8,
        "F9" => K::F9,
        "F10" => K::F10,
        "F11" => K::F11,
        "F12" => K::F12,
        _ => {
            if let Some(n) = name.strip_prefix("Numpad ") {
                return match n {
                    "0" => Some(K::Kp0),
                    "1" => Some(K::Kp1),
                    "2" => Some(K::Kp2),
                    "3" => Some(K::Kp3),
                    "4" => Some(K::Kp4),
                    "5" => Some(K::Kp5),
                    "6" => Some(K::Kp6),
                    "7" => Some(K::Kp7),
                    "8" => Some(K::Kp8),
                    "9" => Some(K::Kp9),
                    _ => None,
                };
            }
            let mut cs = name.chars();
            let (c, rest) = (cs.next()?, cs.next());
            if rest.is_some() {
                return None;
            }
            return letter_key(c);
        }
    };
    Some(k)
}

fn letter_key(c: char) -> Option<KeyCode> {
    use KeyCode as K;
    const LETTERS: [KeyCode; 26] = [
        K::A, K::B, K::C, K::D, K::E, K::F, K::G, K::H, K::I, K::J, K::K, K::L, K::M, K::N, K::O, K::P, K::Q, K::R, K::S,
        K::T, K::U, K::V, K::W, K::X, K::Y, K::Z,
    ];
    const DIGITS: [KeyCode; 10] = [K::Key0, K::Key1, K::Key2, K::Key3, K::Key4, K::Key5, K::Key6, K::Key7, K::Key8, K::Key9];
    match c {
        'a'..='z' => Some(LETTERS[(c as u8 - b'a') as usize]),
        'A'..='Z' => Some(LETTERS[(c as u8 - b'A') as usize]),
        '0'..='9' => Some(DIGITS[(c as u8 - b'0') as usize]),
        _ => None,
    }
}

/// Symbol keys that are better matched as typed characters (keyboard-layout independent).
fn sym_char(mods: u8, name: &str) -> Option<char> {
    Some(match (mods & SHIFT != 0, name) {
        (_, "Less") => '<',
        (_, "Greater") => '>',
        (_, "Plus") => '+',
        (_, "Asterisk") => '*',
        (true, ",") | (true, "Comma") => '<',
        (true, ".") | (true, "Period") => '>',
        (false, ",") => ',',
        (false, ".") => '.',
        _ => return None,
    })
}

pub fn parse(text: &str) -> HashMap<String, Vec<Trigger>> {
    let mut map: HashMap<String, Vec<Trigger>> = HashMap::new();
    let mut cur: Option<String> = None;
    let mut toks: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else { break };
        let mut tok = after[..end].to_string();
        let mut consumed = end + 1;
        // `[KEY:]]` binds the `]` key itself.
        if tok == "KEY:" && after[end + 1..].starts_with(']') {
            tok.push(']');
            consumed += 1;
        }
        toks.push(tok);
        rest = &after[consumed..];
    }
    for tok in &toks {
        let parts: Vec<&str> = tok.splitn(3, ':').collect();
        match parts.as_slice() {
            ["BIND", name, ..] => cur = Some(name.to_string()),
            ["SYM", mods, key] => {
                let Some(name) = &cur else { continue };
                let mods: u8 = mods.parse().unwrap_or(0);
                let t = if let Some(c) = sym_char(mods, key) {
                    Trigger::Char(c)
                } else if let Some(k) = sym_key(key) {
                    Trigger::Key { mods, key: k }
                } else {
                    continue;
                };
                map.entry(name.clone()).or_default().push(t);
            }
            ["KEY", rest @ ..] => {
                let Some(name) = &cur else { continue };
                let s = rest.join(":");
                if let Some(c) = s.chars().next() {
                    map.entry(name.clone()).or_default().push(Trigger::Char(c));
                }
            }
            ["MOUSEWHEEL", mods, dir] => {
                let Some(name) = &cur else { continue };
                let mods: u8 = mods.parse().unwrap_or(0);
                map.entry(name.clone()).or_default().push(Trigger::Wheel { mods, up: *dir == "UP" });
            }
            _ => {}
        }
    }
    map
}

impl Bindings {
    pub fn load(df_dir: &Path) -> Bindings {
        for f in interface_files(df_dir) {
            if let Ok(text) = std::fs::read_to_string(&f) {
                let map = parse(&text);
                if map.contains_key("CURSOR_UP_Z") {
                    let mut b = Bindings { map, source: f.display().to_string(), hold_ms: 300.0, repeat_ms: 90.0 };
                    b.add_builtin();
                    return b;
                }
            }
        }
        let mut b = Bindings::defaults();
        b.add_builtin();
        b
    }

    /// Steam DF defaults (used when no interface.txt is found).
    pub fn defaults() -> Bindings {
        let text = "[BIND:CURSOR_UP:REPEAT_FAST][SYM:0:w][BIND:CURSOR_DOWN:REPEAT_FAST][SYM:0:s]\
            [BIND:CURSOR_LEFT:REPEAT_FAST][SYM:0:a][BIND:CURSOR_RIGHT:REPEAT_FAST][SYM:0:d]\
            [BIND:CURSOR_UP_Z:REPEAT_SLOW][SYM:0:q][MOUSEWHEEL:0:DOWN][BIND:CURSOR_DOWN_Z:REPEAT_SLOW][SYM:0:e][MOUSEWHEEL:0:UP]\
            [BIND:CURSOR_UP_Z_AUX:REPEAT_SLOW][SYM:1:,][BIND:CURSOR_DOWN_Z_AUX:REPEAT_SLOW][SYM:1:.]\
            [BIND:CURSOR_UP_Z_FAST:REPEAT_SLOW][SYM:1:e][MOUSEWHEEL:1:DOWN][BIND:CURSOR_DOWN_Z_FAST:REPEAT_SLOW][SYM:1:c][MOUSEWHEEL:1:UP]\
            [BIND:ZOOM_IN:REPEAT_SLOW][SYM:0:Leftbracket][MOUSEWHEEL:2:UP][BIND:ZOOM_OUT:REPEAT_SLOW][SYM:0:Rightbracket][MOUSEWHEEL:2:DOWN]\
            [BIND:D_PAUSE:REPEAT_NOT][SYM:0:Space][BIND:D_ONESTEP:REPEAT_NOT][SYM:0:Period]\
            [BIND:LEAVESCREEN:REPEAT_NOT][SYM:0:ESC]\
            [BIND:D_DESIGNATE_DIG:REPEAT_NOT][SYM:0:m][BIND:D_DESIGNATE_CHOP:REPEAT_NOT][SYM:0:l]\
            [BIND:D_DESIGNATE_GATHER:REPEAT_NOT][SYM:0:g][BIND:D_DESIGNATE_ERASE:REPEAT_NOT][SYM:0:x]\
            [BIND:D_STOCKPILES:REPEAT_NOT][SYM:0:p][BIND:D_UNITLIST:REPEAT_NOT][SYM:0:u]\
            [BIND:DESIGNATE_DIG:REPEAT_NOT][SYM:0:m][BIND:DESIGNATE_STAIR_UPDOWN:REPEAT_NOT][SYM:0:t]\
            [BIND:DESIGNATE_RAMP:REPEAT_NOT][SYM:0:r][BIND:DESIGNATE_CHANNEL:REPEAT_NOT][SYM:0:u]\
            [BIND:DESIGNATE_DIG_REMOVE_STAIRS_RAMPS:REPEAT_NOT][SYM:0:x]";
        Bindings { map: parse(text), source: "built-in defaults".into(), hold_ms: 300.0, repeat_ms: 90.0 }
    }

    /// Rustfort extras that DF binds elsewhere (arrows pan the camera, PageUp/PageDown
    /// change z-levels, speed keys).
    fn add_builtin(&mut self) {
        let mut add = |n: &str, t: Trigger| {
            let v = self.map.entry(n.to_string()).or_default();
            if !v.contains(&t) {
                v.push(t);
            }
        };
        add("CURSOR_UP", Trigger::Key { mods: 0, key: KeyCode::Up });
        add("CURSOR_DOWN", Trigger::Key { mods: 0, key: KeyCode::Down });
        add("CURSOR_LEFT", Trigger::Key { mods: 0, key: KeyCode::Left });
        add("CURSOR_RIGHT", Trigger::Key { mods: 0, key: KeyCode::Right });
        add("CURSOR_UP_Z", Trigger::Key { mods: 0, key: KeyCode::PageUp });
        add("CURSOR_DOWN_Z", Trigger::Key { mods: 0, key: KeyCode::PageDown });
        add("CURSOR_UP_Z_AUX", Trigger::Char('<'));
        add("CURSOR_DOWN_Z_AUX", Trigger::Char('>'));
        add("RF_SPEED_1", Trigger::Key { mods: 0, key: KeyCode::Key1 });
        add("RF_SPEED_2", Trigger::Key { mods: 0, key: KeyCode::Key2 });
        add("RF_SPEED_3", Trigger::Key { mods: 0, key: KeyCode::Key3 });
        add("RF_SPEED_4", Trigger::Key { mods: 0, key: KeyCode::Key4 });
    }

    pub fn triggers(&self, name: &str) -> &[Trigger] {
        self.map.get(name).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Short human label of the first key binding, e.g. `m` or `Shift+e`.
    pub fn label(&self, name: &str) -> String {
        for t in self.triggers(name) {
            match t {
                Trigger::Char(c) => return c.to_string(),
                Trigger::Key { mods, key } => {
                    let k = format!("{key:?}");
                    let k = if k.len() == 1 { k.to_lowercase() } else { k };
                    return match *mods {
                        0 => k,
                        SHIFT => format!("Shift+{k}"),
                        CTRL => format!("Ctrl+{k}"),
                        _ => format!("Alt+{k}"),
                    };
                }
                Trigger::Wheel { .. } => continue,
            }
        }
        String::new()
    }

    /// Keys of a binding that move continuously while held (camera pan).
    pub fn held(&self, name: &str, input: &InputFrame) -> bool {
        self.triggers(name).iter().any(|t| matches!(t, Trigger::Key { mods: 0, key } if input.down.contains(key)))
    }
}

/// Input of one frame, collected once and then matched against bindings.
#[derive(Default)]
pub struct InputFrame {
    pub mods: u8,
    /// Keys that fired this frame (first press plus our own key repeat).
    pub fired: Vec<KeyCode>,
    pub down: Vec<KeyCode>,
    pub chars: Vec<char>,
    /// Mouse wheel steps, + = wheel up.
    pub wheel: i32,
    /// Set by UI code to stop map/global handling of the wheel (e.g. scrolling a list).
    pub wheel_used: bool,
    pub mouse: Vec2,
    pub lpressed: bool,
    pub lreleased: bool,
    pub ldown: bool,
    pub rpressed: bool,
    pub mpressed: bool,
    pub mdown: bool,
}

#[derive(Default)]
pub struct InputState {
    next_repeat: HashMap<KeyCode, f64>,
}

impl InputState {
    pub fn collect(&mut self, b: &Bindings) -> InputFrame {
        let now = get_time() * 1000.0;
        let mut f = InputFrame::default();
        if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
            f.mods |= SHIFT;
        }
        if is_key_down(KeyCode::LeftControl) || is_key_down(KeyCode::RightControl) {
            f.mods |= CTRL;
        }
        if is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt) {
            f.mods |= ALT;
        }
        let down = get_keys_down();
        for k in get_keys_pressed() {
            f.fired.push(k);
            self.next_repeat.insert(k, now + b.hold_ms);
        }
        self.next_repeat.retain(|k, _| down.contains(k));
        for (k, t) in self.next_repeat.iter_mut() {
            if now >= *t && !f.fired.contains(k) {
                f.fired.push(*k);
                *t = now + b.repeat_ms;
            }
        }
        f.down = down.into_iter().collect();
        while let Some(c) = get_char_pressed() {
            f.chars.push(c);
        }
        let (_, wy) = mouse_wheel();
        if wy > 0.0 {
            f.wheel = 1;
        } else if wy < 0.0 {
            f.wheel = -1;
        }
        let (mx, my) = mouse_position();
        f.mouse = vec2(mx, my);
        f.lpressed = is_mouse_button_pressed(MouseButton::Left);
        f.lreleased = is_mouse_button_released(MouseButton::Left);
        f.ldown = is_mouse_button_down(MouseButton::Left);
        f.rpressed = is_mouse_button_pressed(MouseButton::Right);
        f.mpressed = is_mouse_button_pressed(MouseButton::Middle);
        f.mdown = is_mouse_button_down(MouseButton::Middle);
        f
    }
}

impl InputFrame {
    /// How often the binding fired this frame.
    pub fn count(&self, b: &Bindings, name: &str) -> i32 {
        let mut n = 0;
        for t in b.triggers(name) {
            match t {
                Trigger::Key { mods, key } => {
                    if *mods == self.mods && self.fired.contains(key) {
                        n += 1;
                    }
                }
                Trigger::Char(c) => n += self.chars.iter().filter(|x| *x == c).count() as i32,
                Trigger::Wheel { mods, up } => {
                    if !self.wheel_used && *mods == self.mods && self.wheel != 0 && (self.wheel > 0) == *up {
                        n += 1;
                    }
                }
            }
        }
        n
    }

    pub fn hit(&self, b: &Bindings, name: &str) -> bool {
        self.count(b, name) > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_df_bindings() {
        let m = parse("[BIND:CURSOR_UP_Z:REPEAT_SLOW]\n[SYM:0:q]\n[MOUSEWHEEL:0:DOWN]\n[BIND:CURSOR_UP_Z_AUX:REPEAT_SLOW]\n[SYM:1:,]\n[BIND:ZOOM_IN:REPEAT_SLOW]\n[KEY:[]\n[MOUSEWHEEL:2:UP]");
        assert_eq!(
            m["CURSOR_UP_Z"],
            vec![Trigger::Key { mods: 0, key: KeyCode::Q }, Trigger::Wheel { mods: 0, up: false }]
        );
        assert_eq!(m["CURSOR_UP_Z_AUX"], vec![Trigger::Char('<')]);
        assert_eq!(m["ZOOM_IN"], vec![Trigger::Char('['), Trigger::Wheel { mods: 2, up: true }]);
    }

    /// Uses the real interface.txt of the local DF install, if there is one.
    #[test]
    fn loads_the_installed_df_bindings() {
        let b = Bindings::load(&rf_gfx::df_dir());
        assert!(b.triggers("CURSOR_UP_Z").iter().any(|t| matches!(t, Trigger::Wheel { mods: 0, .. })), "{}", b.source);
        assert!(b.triggers("ZOOM_IN").iter().any(|t| matches!(t, Trigger::Wheel { mods: CTRL, .. })), "{}", b.source);
        assert!(!b.triggers("D_PAUSE").is_empty());
        assert!(!b.triggers("CURSOR_UP").is_empty());
    }

    #[test]
    fn defaults_have_the_core_bindings() {
        let b = Bindings::defaults();
        for n in ["CURSOR_UP", "CURSOR_UP_Z", "CURSOR_DOWN_Z", "ZOOM_IN", "D_PAUSE", "D_DESIGNATE_DIG"] {
            assert!(!b.triggers(n).is_empty(), "{n}");
        }
    }
}
