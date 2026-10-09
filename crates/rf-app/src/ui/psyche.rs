//! Unit sheet tabs of the Steam version that show the mind and body of a creature:
//! Thoughts, Needs, Personality, Health and Relations, plus the DF wording they use
//! (stress levels, need focus, facets, beliefs, emotion colors).
//!
//! The wording follows the DF wiki (v50 pages "Stress", "Personality trait", "Need",
//! "Emotion"). Facet and belief sentences are paraphrased (DF has one hand-written sentence per
//! trait and level); the level boundaries are DF's.

use super::{clip, col, pretty, words};
use crate::font::GW;
use crate::game::Game;
use macroquad::prelude::*;
use rf_core::{Unit, World};
use rf_sim::StressLevel;

// ------------------------------------------------------------------ stress

/// Color of a stress level, like the face in DF's unit list (green = happy, red = breaking).
#[allow(unreachable_patterns)]
pub fn stress_color(l: StressLevel) -> Color {
    match l {
        StressLevel::Ecstatic => Color::from_rgba(80, 255, 80, 255),
        StressLevel::Happy => Color::from_rgba(120, 220, 90, 255),
        StressLevel::Content => Color::from_rgba(170, 220, 140, 255),
        StressLevel::Fine => Color::from_rgba(220, 220, 220, 255),
        StressLevel::Stressed => Color::from_rgba(250, 220, 80, 255),
        StressLevel::VeryStressed => Color::from_rgba(245, 140, 50, 255),
        StressLevel::Breaking => Color::from_rgba(255, 70, 60, 255),
        _ => col::TEXT,
    }
}

/// Sort key: higher = more stressed.
#[allow(unreachable_patterns)]
pub fn stress_rank(l: StressLevel) -> i32 {
    match l {
        StressLevel::Ecstatic => 0,
        StressLevel::Happy => 1,
        StressLevel::Content => 2,
        StressLevel::Fine => 3,
        StressLevel::Stressed => 4,
        StressLevel::VeryStressed => 5,
        StressLevel::Breaking => 6,
        _ => 3,
    }
}

/// CP437 face for the stress level (DF uses smiling to frowning faces).
pub fn stress_face(l: StressLevel) -> &'static str {
    if stress_rank(l) >= 4 {
        "☻"
    } else {
        "☺"
    }
}

// ------------------------------------------------------------------ needs

/// DF need focus levels (wiki "Need": "badly distracted" <= -100000 .. "unfettered" >= 400;
/// the positive boundaries 100/200/400 are as DFHack shows them). Returns label, color and
/// a 0..1 bar position.
pub fn focus_label(focus: i32) -> (&'static str, Color, f32) {
    match focus {
        i32::MIN..=-100_000 => ("Badly distracted", Color::from_rgba(255, 70, 60, 255), 0.05),
        -99_999..=-10_000 => ("Distracted", Color::from_rgba(245, 140, 50, 255), 0.2),
        -9_999..=-1_000 => ("Unfocused", Color::from_rgba(250, 220, 80, 255), 0.35),
        -999..=99 => ("Not distracted", Color::from_rgba(210, 210, 210, 255), 0.5),
        100..=199 => ("Untroubled", Color::from_rgba(170, 220, 140, 255), 0.65),
        200..=399 => ("Level-headed", Color::from_rgba(120, 220, 90, 255), 0.82),
        _ => ("Unfettered", Color::from_rgba(80, 255, 80, 255), 1.0),
    }
}

/// What a need is about: (phrase when satisfied, phrase when unmet), as in DF's
/// "level-headed after drinking" / "distracted by being kept from alcohol".
pub fn need_phrases(id: &str) -> (&'static str, &'static str) {
    match id {
        "Socialize" => ("socializing", "being away from people"),
        "DrinkAlcohol" => ("drinking", "being kept from alcohol"),
        "PrayOrMeditate" => ("praying", "being unable to pray"),
        "StayOccupied" => ("staying occupied", "being unoccupied"),
        "BeCreative" => ("doing something creative", "doing nothing creative"),
        "Excitement" => ("doing something exciting", "leading an unexciting life"),
        "LearnSomething" => ("learning something", "not learning anything"),
        "BeWithFamily" | "Family" => ("being with family", "being away from family"),
        "BeWithFriends" | "Friends" => ("being with friends", "being away from friends"),
        "HearEloquence" => ("hearing eloquent speech", "being unable to hear eloquent speech"),
        "UpholdTradition" => ("upholding tradition", "being away from traditions"),
        "SelfExamination" => ("self-examination", "a lack of introspection"),
        "MakeMerry" => ("making merry", "being unable to make merry"),
        "CraftObject" => ("practicing a craft", "being unable to practice a craft"),
        "MartialTraining" => ("practicing a martial art", "being unable to practice a martial art"),
        "PracticeSkill" => ("practicing a skill", "being unable to practice a skill"),
        "TakeItEasy" => ("taking it easy", "being unable to take it easy"),
        "MakeRomance" => ("making romance", "being unable to make romance"),
        "SeeAnimal" => ("seeing animals", "being away from animals"),
        "SeeGreatBeast" => ("seeing a great beast", "being away from great beasts"),
        "AcquireObject" => ("acquiring something", "being unable to acquire something"),
        "EatGoodMeal" => ("eating a good meal", "a lack of decent meals"),
        "Fight" => ("fighting", "being unable to fight"),
        "CauseTrouble" => ("causing trouble", "a lack of trouble-making"),
        "Argue" => ("arguing", "being unable to argue"),
        "BeExtravagant" => ("being extravagant", "being unable to be extravagant"),
        "Wander" => ("wandering", "being unable to wander"),
        "HelpSomebody" => ("helping somebody", "being unable to help anybody"),
        "ThinkAbstractly" => ("thinking abstractly", "a lack of abstract thinking"),
        "AdmireArt" => ("admiring art", "being unable to admire art"),
        _ => ("", ""),
    }
}

/// "Level-headed after drinking", "Distracted by being away from people", "Not distracted".
pub fn need_sentence(id: &str, focus: i32, deity: Option<&str>) -> String {
    let (label, _, _) = focus_label(focus);
    let (good, bad) = need_phrases(id);
    let god = deity.map(|d| format!(" to {d}")).unwrap_or_default();
    if good.is_empty() {
        return format!("{label} ({})", words(id).to_lowercase());
    }
    if focus >= 100 {
        format!("{label} after {good}{god}")
    } else if focus <= -1_000 {
        format!("{label} by {bad}{god}")
    } else {
        // DF still names the need for neutral ones.
        format!("{label} ({good}{god})")
    }
}

// ------------------------------------------------------------------ emotions

/// Valence of a DF `emotion_type`: 1 = pleasant (lowers stress), -1 = unpleasant, 0 = neutral.
pub fn emotion_valence(e: &str) -> i8 {
    const GOOD: &[&str] = &[
        "ACCEPTANCE", "ADMIRATION", "ADORATION", "AFFECTION", "AMUSEMENT", "ARDOR", "AROUSAL", "BLISS", "CALM",
        "CHEER", "CONTENTMENT", "DELIGHT", "EAGERNESS", "ECSTASY", "ELATION", "ENJOYMENT", "ENTHUSIASM", "EUPHORIA",
        "EXHILARATION", "FONDNESS", "FREEDOM", "GAIETY", "GLEE", "GRATIFICATION", "GRATITUDE", "HAPPINESS", "HOPE",
        "JOVIALITY", "JOY", "JUBILATION", "LIKING", "LOVE", "LUST", "OPTIMISM", "PASSION", "PLEASURE", "PRIDE",
        "RAPTURE", "RELIEF", "RESPECT", "REVERENCE", "SATISFACTION", "SERENITY", "SYMPATHY", "TENDERNESS", "THRILL",
        "TRIUMPH", "VALOR", "WONDER", "ZEAL", "ZEST",
    ];
    const BAD: &[&str] = &[
        "AGITATION", "AGONY", "ALARM", "ALIENATION", "ANGER", "ANGST", "ANGUISH", "ANNOYANCE", "ANXIETY", "APATHY",
        "AVERSION", "BITTERNESS", "BOREDOM", "CONTEMPT", "DEFEAT", "DEJECTION", "DEPRESSION", "DESPAIR",
        "DESPERATION", "DISAPPOINTMENT", "DISCOURAGEMENT", "DISGUST", "DISILLUSIONMENT", "DISLIKE", "DISMAY",
        "DISPLEASURE", "DISQUIET", "DISTRESS", "DREAD", "EMBARRASSMENT", "EXASPERATION", "FEAR", "FEROCITY",
        "FRIGHT", "FRUSTRATION", "FURY", "GLOOM", "GRIEF", "GROUCHINESS", "GRUMPINESS", "GUILT", "HATRED",
        "HOPELESSNESS", "HORROR", "HUMILIATION", "HURT", "INSULT", "IRRITATION", "ISOLATION", "JEALOUSY",
        "LONELINESS", "MELANCHOLY", "MISERY", "MORTIFICATION", "NERVOUSNESS", "OUTRAGE", "PANIC", "PESSIMISM",
        "PITY", "RAGE", "REGRET", "REJECTION", "REMORSE", "REPENTANCE", "RESENTMENT", "SADNESS", "SCORN",
        "SHAKEN", "SHAME", "SHOCK", "SORROW", "SPITE", "SUFFERING", "TERROR", "TORMENT", "UNEASINESS",
        "VENGEFULNESS", "WOE", "WORRY", "WRATH",
    ];
    let e = e.to_ascii_uppercase();
    if GOOD.contains(&e.as_str()) {
        1
    } else if BAD.contains(&e.as_str()) {
        -1
    } else {
        0
    }
}

/// Color of a thought line: green for pleasant, red for unpleasant, brighter when strong.
pub fn emotion_color(e: &str, strength: i32) -> Color {
    let strong = strength.abs() >= 50;
    match emotion_valence(e) {
        1 if strong => Color::from_rgba(90, 255, 90, 255),
        1 => Color::from_rgba(130, 205, 110, 255),
        -1 if strong => Color::from_rgba(255, 80, 70, 255),
        -1 => Color::from_rgba(225, 140, 110, 255),
        _ => Color::from_rgba(200, 200, 205, 255),
    }
}

/// "today", "yesterday", "5 days ago", "2 months ago", "last year".
pub fn ago(w: &World, year: i32, year_tick: i32) -> String {
    let dt = (w.calendar.year - year) as i64 * rf_core::TICKS_PER_YEAR as i64 + (w.calendar.year_tick - year_tick) as i64;
    let day = rf_core::TICKS_PER_DAY as i64;
    let month = rf_core::TICKS_PER_MONTH as i64;
    match dt {
        i64::MIN..=-1 => "recently".into(),
        d if d < day => "today".into(),
        d if d < 2 * day => "yesterday".into(),
        d if d < month => format!("{} days ago", d / day),
        d if d < 2 * month => "last month".into(),
        d if d < 12 * month => format!("{} months ago", d / month),
        d if d < 24 * month => "last year".into(),
        d => format!("{} years ago", d / (12 * month)),
    }
}

/// DF keeps placeholder memories (`ANYTHING`, thought `None`) that its UI never lists.
pub fn shown_emotion(e: &rf_core::Emotion) -> bool {
    e.emotion != "ANYTHING" && !e.emotion.is_empty() && e.thought != "None"
}

pub fn pronoun(u: &Unit) -> &'static str {
    match u.sex {
        0 => "She",
        1 => "He",
        _ => "It",
    }
}

// ------------------------------------------------------------------ personality

/// Facet: DF token, sentence for high values, sentence for low values. `{}` takes the
/// intensity adverb.
const FACETS: &[(&str, &str, &str)] = &[
    ("LOVE_PROPENSITY", "{}easily falls in love and develops positive feelings", "{}rarely falls in love or develops positive feelings"),
    ("HATE_PROPENSITY", "{}easily develops negative feelings", "{}rarely develops negative feelings"),
    ("ENVY_PROPENSITY", "is {}prone to envy", "is {}free of envy"),
    ("CHEER_PROPENSITY", "is {}cheerful", "is {}rarely happy or enthusiastic"),
    ("DEPRESSION_PROPENSITY", "is {}prone to sadness and despair", "{}rarely feels discouraged"),
    ("ANGER_PROPENSITY", "is {}quick to anger", "is {}slow to anger"),
    ("ANXIETY_PROPENSITY", "is {}nervous and anxious", "is {}calm and collected"),
    ("LUST_PROPENSITY", "is {}lustful", "{}rarely feels lustful"),
    ("STRESS_VULNERABILITY", "is {}vulnerable to stress", "is {}resilient under pressure"),
    ("GREED", "is {}greedy", "cares {}little for material goods"),
    ("IMMODERATION", "is {}ruled by cravings and urges", "{}rarely gives in to cravings"),
    ("VIOLENT", "is {}given to violence", "is {}averse to violence"),
    ("PERSEVERANCE", "is {}stubborn and persistent", "is {}quick to give up"),
    ("WASTEFULNESS", "is {}wasteful with resources", "is {}frugal"),
    ("DISCORD", "is {}fond of discord", "is {}keen to keep the peace"),
    ("FRIENDLINESS", "is {}friendly", "is {}quarrelsome"),
    ("POLITENESS", "is {}polite", "is {}rude"),
    ("DISDAIN_ADVICE", "is {}dismissive of advice", "is {}reliant on the advice of others"),
    ("BRAVERY", "is {}brave in the face of danger", "is {}fearful in the face of danger"),
    ("CONFIDENCE", "is {}confident", "is {}unsure of own abilities"),
    ("VANITY", "is {}vain", "is {}unconcerned with appearance"),
    ("AMBITION", "is {}ambitious", "is {}content with own station"),
    ("GRATITUDE", "is {}grateful for help", "is {}ungrateful"),
    ("IMMODESTY", "is {}immodest", "is {}modest"),
    ("HUMOR", "is {}fond of jokes and humor", "is {}humorless"),
    ("VENGEFUL", "is {}vengeful", "is {}forgiving"),
    ("PRIDE", "is {}proud", "is {}humble"),
    ("CRUELTY", "is {}cruel", "is {}merciful"),
    ("SINGLEMINDED", "is {}single-minded", "is {}easily distracted from goals"),
    ("HOPEFUL", "is {}hopeful about the future", "is {}pessimistic"),
    ("CURIOUS", "is {}curious", "is {}incurious"),
    ("BASHFUL", "is {}bashful", "is {}socially bold"),
    ("PRIVACY", "is {}private", "is {}open about personal matters"),
    ("PERFECTIONIST", "is {}particular about details", "is {}careless with details"),
    ("CLOSEMINDED", "is {}closed-minded", "is {}open-minded"),
    ("TOLERANT", "is {}tolerant of others", "is {}intolerant of others"),
    ("EMOTIONALLY_OBSESSIVE", "is {}obsessive about relationships", "is {}detached from relationships"),
    ("SWAYED_BY_EMOTIONS", "is {}swayed by emotions", "is {}unswayed by emotions"),
    ("ALTRUISM", "is {}altruistic", "is {}unconcerned with helping others"),
    ("DUTIFULNESS", "is {}dutiful", "is {}disdainful of obligations"),
    ("THOUGHTLESSNESS", "is {}rash and thoughtless", "is {}careful and deliberate"),
    ("ORDERLINESS", "is {}orderly", "is {}disorganized"),
    ("TRUST", "is {}trusting", "is {}distrustful"),
    ("GREGARIOUSNESS", "is {}fond of company", "is {}solitary"),
    ("ASSERTIVENESS", "is {}assertive", "is {}passive"),
    ("ACTIVITY_LEVEL", "is {}energetic", "is {}leisurely"),
    ("EXCITEMENT_SEEKING", "is {}thrill-seeking", "is {}averse to excitement"),
    ("IMAGINATION", "is {}imaginative", "is {}unimaginative"),
    ("ABSTRACT_INCLINED", "is {}drawn to abstract ideas", "is {}practical-minded"),
    ("ART_INCLINED", "is {}moved by art", "is {}unmoved by art"),
];

/// DF facet levels: 0-9, 10-24, 25-39 (low), 40-60 (average, not described), 61-75, 76-90,
/// 91-100 (high). Returns the sentence (without subject), or None for average values.
pub fn facet_sentence(token: &str, v: i32) -> Option<String> {
    let (high, adv) = match v {
        91..=i32::MAX => (true, "very "),
        76..=90 => (true, ""),
        61..=75 => (true, "somewhat "),
        40..=60 => return None,
        25..=39 => (false, "somewhat "),
        10..=24 => (false, ""),
        _ => (false, "very "),
    };
    let s = match FACETS.iter().find(|f| f.0 == token) {
        Some(f) => if high { f.1 } else { f.2 },
        None => return Some(format!("{} {}: {v}", if high { "high" } else { "low" }, words(&pretty(token)).to_lowercase())),
    };
    Some(s.replacen("{}", adv, 1))
}

/// DF value (belief) nouns.
fn value_noun(id: &str) -> String {
    match id {
        "LAW" => "the law".into(),
        "TRUTH" => "the truth".into(),
        "ARTWORK" => "art".into(),
        "SELF_CONTROL" => "self-control".into(),
        "MARTIAL_PROWESS" => "martial prowess".into(),
        "HARD_WORK" => "hard work".into(),
        "LEISURE_TIME" => "leisure time".into(),
        other => other.replace('_', " ").to_lowercase(),
    }
}

/// DF belief levels (wiki "Personality trait", beliefs -50..=50): -10..=10 is neutral.
pub fn value_sentence(id: &str, strength: i32) -> Option<String> {
    let n = value_noun(id);
    Some(match strength {
        41..=i32::MAX => format!("holds {n} to be one of the highest ideals"),
        26..=40 => format!("greatly values {n}"),
        11..=25 => format!("values {n}"),
        -10..=10 => return None,
        -25..=-11 => format!("does not care much for {n}"),
        -40..=-26 => format!("sees {n} as a waste"),
        _ => format!("finds {n} abhorrent"),
    })
}

// ------------------------------------------------------------------ rows

/// One entry of a scrollable sheet tab.
pub enum Row {
    Head(String),
    /// Wrapped text.
    Text(String, Color),
    /// Text with a dim note right-aligned on its (first) line.
    Note(String, Color, String),
    /// Label, bar fill 0..1, bar color, caption on the bar.
    Bar(String, f32, Color, String),
    /// Clickable unit link (selects the unit).
    Link(String, Color, String, i32),
    /// Small level gauge (0..1) before wrapped text, with a dim note on the right.
    Gauge(String, Color, f32, String),
    Gap,
}

enum VLine {
    Head(String),
    Text(String, Color, String),
    Bar(String, f32, Color, String),
    Link(String, Color, String, i32),
    Gauge(String, Color, Option<f32>, String),
    Gap,
}

/// Width of the gauge in characters (plus one blank).
const GAUGE_C: usize = 5;

/// Greedy word wrap to `n` characters per line; continuation lines get `indent` spaces.
pub fn wrap(s: &str, n: usize, indent: usize) -> Vec<String> {
    let n = n.max(8);
    let mut out = Vec::new();
    let mut cur = String::new();
    for w in s.split(' ') {
        let lim = if out.is_empty() { n } else { n - indent };
        if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > lim {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    for l in out.iter_mut().skip(1) {
        *l = format!("{}{l}", " ".repeat(indent));
    }
    out
}

/// Draw rows into `area` with the sheet's scroll offset (`g.unit_scroll`, wheel scrolls).
pub fn draw_rows(g: &mut Game, rows: Vec<Row>, area: Rect) {
    let s = g.ui.scale;
    let lh = g.ui.line_h();
    let maxc = ((area.w / (GW * s)) as usize).max(10);
    let mut v: Vec<VLine> = Vec::with_capacity(rows.len() + 8);
    for r in rows {
        match r {
            Row::Head(t) => v.push(VLine::Head(t)),
            Row::Text(t, c) => v.extend(wrap(&t, maxc, 2).into_iter().map(|l| VLine::Text(l, c, String::new()))),
            Row::Note(t, c, note) => {
                let room = maxc.saturating_sub(note.chars().count() + 1);
                let mut ls = wrap(&t, room, 2);
                let first = ls.remove(0);
                v.push(VLine::Text(first, c, note));
                v.extend(ls.into_iter().map(|l| VLine::Text(l, c, String::new())));
            }
            Row::Bar(l, f, c, cap) => v.push(VLine::Bar(l, f, c, cap)),
            Row::Link(t, c, note, id) => v.push(VLine::Link(t, c, note, id)),
            Row::Gauge(t, c, f, note) => {
                let room = maxc.saturating_sub(note.chars().count() + 1 + GAUGE_C + 1);
                for (k, l) in wrap(&t, room, 1).into_iter().enumerate() {
                    if k == 0 {
                        v.push(VLine::Gauge(l, c, Some(f), note.clone()));
                    } else {
                        v.push(VLine::Gauge(l, c, None, String::new()));
                    }
                }
            }
            Row::Gap => v.push(VLine::Gap),
        }
    }
    let h_of = |l: &VLine| if matches!(l, VLine::Gap) { lh * 0.5 } else { lh };
    // Scroll: how many lines fit from the offset.
    let total = v.len();
    let fits = |from: usize| {
        let mut h = 0.0;
        let mut n = 0;
        for l in &v[from..] {
            h += h_of(l);
            if h > area.h {
                break;
            }
            n += 1;
        }
        n
    };
    let step = super::wheel_over(&g.ui, &mut g.input, area);
    let mut max_scroll = 0;
    while max_scroll < total && fits(max_scroll) + max_scroll < total {
        max_scroll += 1;
    }
    g.unit_scroll = (g.unit_scroll as i32 + step * 3).clamp(0, max_scroll as i32) as usize;
    let start = g.unit_scroll;
    let n = fits(start);
    let mut y = area.y;
    let mut select = None;
    for l in v.iter().skip(start).take(n) {
        match l {
            VLine::Head(t) => {
                g.font.draw(&clip(t, maxc), area.x, y, s, col::BORDER_HI);
            }
            VLine::Text(t, c, note) => {
                g.font.draw(t, area.x, y, s, *c);
                if !note.is_empty() {
                    let w = g.font.width(note, s);
                    g.font.draw(note, area.x + area.w - w, y, s, col::TEXT_DIM);
                }
            }
            VLine::Bar(label, f, c, cap) => {
                let lw = (maxc as f32 * 0.42).floor() as usize;
                g.font.draw(&clip(label, lw), area.x, y, s, col::TEXT);
                let bx = area.x + (lw + 1) as f32 * GW * s;
                let bw = area.w - (lw + 1) as f32 * GW * s;
                let bh = lh - 4.0 * s;
                draw_rectangle(bx, y - 1.0 * s, bw, bh, Color::new(0.12, 0.12, 0.14, 1.0));
                draw_rectangle(bx, y - 1.0 * s, bw * f.clamp(0.0, 1.0), bh, Color::new(c.r * 0.55, c.g * 0.55, c.b * 0.55, 1.0));
                draw_rectangle_lines(bx, y - 1.0 * s, bw, bh, 1.0, Color::new(0.35, 0.35, 0.38, 1.0));
                let cc = clip(cap, ((bw / (GW * s)) as usize).saturating_sub(1));
                g.font.draw(&cc, bx + 4.0 * s, y, s, *c);
            }
            VLine::Link(t, c, note, id) => {
                let rr = Rect::new(area.x - 2.0 * s, y - 2.0 * s, area.w + 4.0 * s, lh);
                let hov = g.ui.hovered(rr);
                if hov {
                    draw_rectangle(rr.x, rr.y, rr.w, rr.h, col::BUTTON_HOVER);
                }
                let room = maxc.saturating_sub(note.chars().count() + 1);
                g.font.draw(&clip(t, room), area.x, y, s, if hov { col::TEXT_HI } else { *c });
                if !note.is_empty() {
                    let w = g.font.width(note, s);
                    g.font.draw(note, area.x + area.w - w, y, s, col::TEXT_DIM);
                }
                if *id >= 0 && g.ui.clicked(rr) {
                    select = Some(*id);
                }
            }
            VLine::Gauge(t, c, f, note) => {
                if let Some(f) = f {
                    let gw = (GAUGE_C as f32 - 0.5) * GW * s;
                    let gh = lh - 7.0 * s;
                    let gy = y + 1.0 * s;
                    draw_rectangle(area.x, gy, gw, gh, Color::new(0.12, 0.12, 0.14, 1.0));
                    draw_rectangle(area.x, gy, gw * f.clamp(0.04, 1.0), gh, *c);
                    draw_rectangle_lines(area.x, gy, gw, gh, 1.0, Color::new(0.38, 0.38, 0.42, 1.0));
                }
                g.font.draw(t, area.x + (GAUGE_C + 1) as f32 * GW * s, y, s, *c);
                if !note.is_empty() {
                    let w = g.font.width(note, s);
                    g.font.draw(note, area.x + area.w - w, y, s, col::TEXT_DIM);
                }
            }
            VLine::Gap => {}
        }
        y += h_of(l);
    }
    if start > 0 {
        g.font.draw("▲", area.x + area.w - GW * s, area.y - lh, s, col::TEXT_DIM);
    }
    if start + n < total {
        g.font.draw("▼ more (wheel)", area.x + area.w - 14.0 * GW * s, area.y + area.h + 2.0 * s, s, col::TEXT_DIM);
    }
    if let Some(id) = select {
        if g.sim.world.unit(id).is_some() {
            g.select_unit(id);
            g.unit_tab = 0;
        }
    }
}

// ------------------------------------------------------------------ tabs

pub fn thoughts_rows(g: &Game, u: &Unit) -> Vec<Row> {
    let mut rows = Vec::new();
    let lvl = g.sim.stress_level(u.id);
    rows.push(Row::Note(format!("{} is {}.", pronoun(u), lvl.label().to_lowercase()), stress_color(lvl), format!("stress {}", u.stress)));
    if !u.mood.is_empty() {
        rows.push(Row::Text(format!("{} is in a {} mood.", pronoun(u), words(&u.mood).to_lowercase()), col::TEXT_HI));
    }
    rows.push(Row::Gap);
    rows.push(Row::Head("Recent thoughts".into()));
    if !u.emotions.iter().any(shown_emotion) {
        let t = if u.emotions.is_empty() { "No thoughts recorded (this export has none)." } else { "No recent thoughts." };
        rows.push(Row::Text(t.into(), col::TEXT_DIM));
        return rows;
    }
    // Newest first, like DF.
    let mut idx: Vec<usize> = (0..u.emotions.len()).filter(|&i| shown_emotion(&u.emotions[i])).collect();
    idx.sort_by_key(|&i| std::cmp::Reverse((u.emotions[i].year, u.emotions[i].year_tick, i)));
    for i in idx {
        let e = &u.emotions[i];
        let mut t = g.sim.describe_emotion(e);
        if let Some(f) = t.chars().next() {
            if f.is_lowercase() {
                t = format!("{} {t}", pronoun(u));
            }
        }
        if !t.ends_with('.') {
            t.push('.');
        }
        rows.push(Row::Note(t, emotion_color(&e.emotion, e.strength), ago(&g.sim.world, e.year, e.year_tick)));
    }
    rows
}

pub fn needs_rows(g: &Game, u: &Unit) -> Vec<Row> {
    let mut rows = Vec::new();
    if u.needs.is_empty() {
        rows.push(Row::Text("No needs recorded (this export has none).".into(), col::TEXT_DIM));
        return rows;
    }
    // Overall focus like DF's header: the share of needs that are met.
    let unmet = u.needs.iter().filter(|n| n.focus_level <= -1_000).count();
    let n = u.needs.len();
    let (t, c) = match unmet * 100 / n.max(1) {
        0 => ("is focused", col::TEXT_GOOD),
        1..=24 => ("is slightly distracted", col::TEXT),
        25..=49 => ("is distracted", col::TEXT_HI),
        _ => ("is badly distracted", col::TEXT_BAD),
    };
    rows.push(Row::Note(format!("{} {t}.", pronoun(u)), c, format!("{unmet} of {n} unmet")));
    rows.push(Row::Gap);
    let mut needs: Vec<&rf_core::UnitNeed> = u.needs.iter().collect();
    // Strongest needs first, the most distracting first among equals.
    needs.sort_by(|a, b| b.need_level.cmp(&a.need_level).then(a.focus_level.cmp(&b.focus_level)).then(a.id.cmp(&b.id)));
    for nd in needs {
        let (_, c, f) = focus_label(nd.focus_level);
        let deity = (nd.deity_id >= 0).then(|| deity_name(&g.sim.world, nd.deity_id));
        let t = need_sentence(&nd.id, nd.focus_level, deity.as_deref());
        rows.push(Row::Gauge(t, c, f, need_short(nd.need_level).to_string()));
    }
    rows.push(Row::Gap);
    rows.push(Row::Text("Right: how strong the need is (DF need level 1-10).".into(), col::TEXT_DIM));
    rows
}

/// Short need strength for the needs list.
pub fn need_short(level: i32) -> &'static str {
    match level {
        i32::MIN..=1 => "slight",
        2..=4 => "",
        5..=9 => "strong",
        _ => "overwhelming",
    }
}

/// A deity's display name: the hist. figure's unit on the map if any, else its id.
fn deity_name(w: &World, hf: i32) -> String {
    w.units.iter().find(|u| u.hist_fig_id == hf).map(super::panels::unit_name).unwrap_or_else(|| format!("deity #{hf}"))
}

pub fn personality_rows(_g: &Game, u: &Unit) -> Vec<Row> {
    let mut rows = Vec::new();
    rows.push(Row::Head("Personality".into()));
    let mut avg = Vec::new();
    let mut any = false;
    for (f, v) in &u.facets {
        match facet_sentence(f, *v) {
            Some(t) => {
                any = true;
                let strong = *v >= 91 || *v <= 9;
                rows.push(Row::Note(format!("{} {t}.", pronoun(u)), if strong { col::TEXT_HI } else { col::TEXT }, v.to_string()));
            }
            None => avg.push(words(&pretty(f)).to_lowercase().replace(" propensity", "")),
        }
    }
    if u.facets.is_empty() {
        rows.push(Row::Text("No personality recorded.".into(), col::TEXT_DIM));
    } else if !any {
        rows.push(Row::Text("Utterly average in every way.".into(), col::TEXT_DIM));
    }
    // Like DF, average facets (40-60) are not described.
    if !avg.is_empty() && any {
        rows.push(Row::Text(format!("({} more traits are average.)", avg.len()), col::TEXT_DIM));
    }
    rows.push(Row::Gap);
    rows.push(Row::Head("Beliefs".into()));
    let mut vals: Vec<&rf_core::ValueStrength> = u.values.iter().collect();
    vals.sort_by(|a, b| b.strength.cmp(&a.strength).then(a.id.cmp(&b.id)));
    let mut shown = 0;
    for v in vals {
        if let Some(t) = value_sentence(&v.id, v.strength) {
            let c = if v.strength > 0 { Color::from_rgba(150, 210, 240, 255) } else { Color::from_rgba(220, 160, 140, 255) };
            rows.push(Row::Note(format!("{} {t}.", pronoun(u)), c, format!("{:+}", v.strength)));
            shown += 1;
        }
    }
    if u.values.is_empty() {
        rows.push(Row::Text("No beliefs recorded (this export has none).".into(), col::TEXT_DIM));
    } else if shown == 0 {
        rows.push(Row::Text("Holds no strong beliefs.".into(), col::TEXT_DIM));
    }
    rows
}

/// Name of a body part token from the creature raws of the unit's race/caste.
pub fn body_part_name(w: &World, u: &Unit, token: &str) -> String {
    let part = w.creature_raws.iter().find(|c| c.token == u.race).and_then(|c| {
        c.castes.iter().find(|k| k.token == u.caste).or(c.castes.first()).and_then(|k| k.body_parts.iter().find(|p| p.token == token))
    });
    match part {
        Some(p) if !p.name.is_empty() => p.name.clone(),
        _ => pretty(token).to_lowercase(),
    }
}

pub fn wound_color(w: &rf_core::Wound) -> Color {
    let has = |f: &str| w.flags.iter().any(|x| x == f);
    if has("SEVERED") || has("MISSING") {
        Color::from_rgba(150, 150, 150, 255)
    } else if has("BROKEN") || has("FRACTURED") || w.severity >= 6000 {
        Color::from_rgba(255, 70, 60, 255)
    } else if w.severity >= 2000 || w.bleeding > 0 || has("BLEEDING") {
        Color::from_rgba(245, 150, 60, 255)
    } else {
        Color::from_rgba(240, 220, 110, 255)
    }
}

/// A healed wound (DF keeps scars as wounds with `SCAR_*` flags).
pub fn is_scar(w: &rf_core::Wound) -> bool {
    w.bleeding == 0
        && w.pain == 0
        && w.flags.iter().any(|f| f.starts_with("SCAR_"))
        && !w.flags.iter().any(|f| matches!(f.as_str(), "INFECTED" | "INFECTION" | "BROKEN" | "FRACTURED" | "SEVERED"))
}

/// A wound that still matters (not a scar).
pub fn is_active_wound(w: &rf_core::Wound) -> bool {
    !is_scar(w)
}

pub fn is_bleeding(u: &Unit) -> bool {
    u.wounds.iter().any(|w| w.bleeding > 0 || w.flags.iter().any(|f| f == "BLEEDING"))
}

/// Readable words of DF wound flags (administrative flags are dropped).
pub fn wound_words(w: &rf_core::Wound) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        if !out.iter().any(|o| o == s) {
            out.push(s.to_string());
        }
    };
    for f in &w.flags {
        match f.as_str() {
            "DIAGNOSED" | "ENTIRE_SURFACE" => {}
            "EDGED_DAMAGE" | "CUT" => push("cut"),
            "BLUNT_DAMAGE" => push("bruised"),
            "SMASHED" => push("smashed"),
            "SMASHED_APART" => push("smashed apart"),
            "BROKEN" | "FRACTURED" => push("broken"),
            "SEVERED" => push("severed"),
            "INFECTED" | "INFECTION" => push("infected"),
            "BLEEDING" => push("bleeding"),
            "SUTURED" => push("sutured"),
            "BANDAGED" => push("bandaged"),
            "SET" => push("set"),
            x if x.starts_with("SCAR_") => push("scar"),
            x if x.contains("SHAKE") => push("trembling"),
            x if x.contains("JOINT_BEND") => push("stiff joint"),
            x => push(&x.replace('_', " ").to_lowercase()),
        }
    }
    if w.bleeding > 0 {
        push("bleeding");
    }
    out
}

pub fn health_rows(g: &Game, u: &Unit) -> Vec<Row> {
    let mut rows = Vec::new();
    let w = &g.sim.world;
    let active: Vec<&rf_core::Wound> = u.wounds.iter().filter(|x| is_active_wound(x)).collect();
    let scars: Vec<&rf_core::Wound> = u.wounds.iter().filter(|x| is_scar(x)).collect();
    let pain: i32 = u.wounds.iter().map(|w| w.pain).sum();
    // Status flags like the DF health screen.
    let mut status = Vec::new();
    if u.flags.dead {
        status.push(("Dead".to_string(), col::TEXT_BAD));
    }
    if is_bleeding(u) {
        status.push(("Bleeding".to_string(), col::TEXT_BAD));
    }
    match pain {
        0 => {}
        1..=49 => status.push(("In pain".to_string(), col::TEXT_HI)),
        50..=149 => status.push(("In great pain".to_string(), Color::from_rgba(245, 150, 60, 255))),
        _ => status.push(("In extreme pain".to_string(), col::TEXT_BAD)),
    }
    if u.wounds.iter().any(|x| x.flags.iter().any(|f| f == "INFECTED" || f == "INFECTION")) {
        status.push(("Infection".to_string(), Color::from_rgba(200, 220, 80, 255)));
    }
    if u.blood_max > 0 && u.blood < u.blood_max {
        let pct = u.blood * 100 / u.blood_max;
        let c = if pct < 50 { col::TEXT_BAD } else if pct < 80 { col::TEXT_HI } else { col::TEXT };
        status.push((if pct < 50 { "Severe blood loss".to_string() } else { "Blood loss".to_string() }, c));
    }
    rows.push(Row::Head("Status".into()));
    if status.is_empty() {
        rows.push(Row::Text(if active.is_empty() { "Healthy.".into() } else { "Wounded.".into() }, if active.is_empty() { col::TEXT_GOOD } else { col::TEXT_HI }));
    }
    for (t, c) in status {
        rows.push(Row::Text(t, c));
    }
    rows.push(Row::Gap);
    if u.blood_max > 0 {
        let f = u.blood as f32 / u.blood_max as f32;
        let c = if f < 0.5 { col::TEXT_BAD } else if f < 0.8 { col::TEXT_HI } else { col::TEXT_GOOD };
        rows.push(Row::Bar("Blood".into(), f, c, format!("{} / {}", u.blood, u.blood_max)));
    }
    if pain > 0 {
        rows.push(Row::Bar("Pain".into(), (pain as f32 / 200.0).min(1.0), col::TEXT_HI, pain.to_string()));
    }
    rows.push(Row::Gap);
    rows.push(Row::Head(format!("Wounds ({})", active.len())));
    if active.is_empty() {
        rows.push(Row::Text("No open wounds.".into(), col::TEXT_DIM));
    }
    // Grouped by body part, in the order they appear.
    let parts = |ws: &[&rf_core::Wound]| {
        let mut p: Vec<String> = Vec::new();
        for x in ws {
            if !p.contains(&x.body_part) {
                p.push(x.body_part.clone());
            }
        }
        p
    };
    for p in parts(&active) {
        let ws: Vec<&rf_core::Wound> = active.iter().copied().filter(|x| x.body_part == p).collect();
        let worst = ws.iter().max_by_key(|x| x.severity).copied().unwrap();
        rows.push(Row::Text(cap_first(&body_part_name(w, u, &p)), wound_color(worst)));
        for wd in ws {
            let layer = if wd.layer.is_empty() { String::new() } else { format!("{}: ", wd.layer.replace('_', " ").to_lowercase()) };
            let ww = wound_words(wd);
            let what = if ww.is_empty() { "damaged".to_string() } else { ww.join(", ") };
            let note = if wd.pain > 0 { format!("pain {}", wd.pain) } else { String::new() };
            rows.push(Row::Note(format!("  {layer}{what}"), wound_color(wd), note));
        }
    }
    if !scars.is_empty() {
        rows.push(Row::Gap);
        rows.push(Row::Head(format!("Scars ({})", scars.len())));
        for p in parts(&scars) {
            let ws: Vec<&rf_core::Wound> = scars.iter().copied().filter(|x| x.body_part == p).collect();
            let mut extra: Vec<String> = Vec::new();
            for x in &ws {
                for t in wound_words(x) {
                    if (t == "trembling" || t == "stiff joint") && !extra.contains(&t) {
                        extra.push(t);
                    }
                }
            }
            let n = ws.len();
            let what = if n == 1 { "a scar".to_string() } else { format!("{n} scars") };
            let tail = if extra.is_empty() { String::new() } else { format!(" ({})", extra.join(", ")) };
            rows.push(Row::Text(format!("{}: {what}{tail}", cap_first(&body_part_name(w, u, &p))), Color::from_rgba(170, 150, 140, 255)));
        }
    }
    rows
}

fn cap_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Label of a DF relationship kind (`Spouse`, `Mother`, `Friend`, `Grudge`, ...).
pub fn relation_label(kind: &str) -> String {
    match kind {
        "Grudge" => "Grudge".into(),
        "KindredSpirit" => "Kindred spirit".into(),
        "CloseFriend" => "Close friend".into(),
        "FormerSpouse" => "Former spouse".into(),
        "Pet" | "PetOwner" => "Pet".into(),
        k => words(k),
    }
}

fn relation_color(kind: &str) -> Color {
    match kind {
        "Spouse" | "Lover" | "Betrothed" => Color::from_rgba(240, 130, 190, 255),
        "Mother" | "Father" | "Child" | "Sibling" | "Brother" | "Sister" | "Grandparent" | "Grandchild" => {
            Color::from_rgba(150, 210, 240, 255)
        }
        "Grudge" | "Enemy" | "Rival" => Color::from_rgba(255, 90, 80, 255),
        "Friend" | "CloseFriend" | "KindredSpirit" => Color::from_rgba(130, 220, 110, 255),
        _ => col::TEXT,
    }
}

/// Relationship groups in the order DF lists them.
const REL_GROUPS: &[(&str, &[&str])] = &[
    ("Family", &["Spouse", "Lover", "Betrothed", "DeceasedSpouse", "FormerSpouse", "Mother", "Father", "Child", "Sibling", "Brother", "Sister", "Grandparent", "Grandchild"]),
    ("Worship", &["Deity"]),
    ("Friends", &["CloseFriend", "Friend"]),
    ("Kindred spirits", &["KindredSpirit"]),
    ("Grudges", &["Grudge", "Enemy", "Rival", "BitterRival"]),
    ("Pets", &["Pet", "PetOwner"]),
    ("Acquaintances", &["PassingAcquaintance", "Acquaintance"]),
];

pub fn relations_rows(g: &Game, u: &Unit) -> Vec<Row> {
    let mut rows = Vec::new();
    let w = &g.sim.world;
    if u.relationships.is_empty() {
        rows.push(Row::Text("No relationships recorded (this export has none).".into(), col::TEXT_DIM));
        return rows;
    }
    let group_of = |k: &str| REL_GROUPS.iter().position(|(_, ks)| ks.contains(&k)).unwrap_or(REL_GROUPS.len());
    for gi in 0..=REL_GROUPS.len() {
        let mut entries: Vec<(bool, String, Row)> = Vec::new();
        for r in u.relationships.iter().filter(|r| group_of(&r.kind) == gi) {
            let other = if r.unit_id >= 0 { w.unit(r.unit_id) } else { None }
                .or_else(|| (r.hist_fig_id >= 0).then(|| w.units.iter().find(|o| o.hist_fig_id == r.hist_fig_id)).flatten());
            let label = relation_label(&r.kind);
            let rank = REL_GROUPS.get(gi).map(|(_, ks)| ks.iter().position(|k| *k == r.kind).unwrap_or(99)).unwrap_or(99);
            match other {
                Some(o) => {
                    let dead = if o.flags.dead { " (dead)" } else { "" };
                    let name = super::panels::unit_name(o);
                    let t = format!("{label}: {name}{dead}");
                    entries.push((false, format!("{rank:03}{name}"), Row::Link(t, relation_color(&r.kind), super::panels::unit_profession(o), o.id)));
                }
                None => {
                    let who = if r.kind == "Deity" { format!("deity #{}", r.hist_fig_id) } else { format!("figure #{}", r.hist_fig_id) };
                    entries.push((true, format!("{rank:03}{:09}", r.hist_fig_id), Row::Link(format!("{label}: {who}"), relation_color(&r.kind), "not here".into(), -1)));
                }
            }
        }
        if entries.is_empty() {
            continue;
        }
        // On the map first, then by kind and name.
        entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let title = REL_GROUPS.get(gi).map(|g| g.0).unwrap_or("Other");
        rows.push(Row::Head(format!("{title} ({})", entries.len())));
        rows.extend(entries.into_iter().map(|e| e.2));
        rows.push(Row::Gap);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn need_focus_wording_like_df() {
        assert_eq!(focus_label(-200_000).0, "Badly distracted");
        assert_eq!(focus_label(-10_000).0, "Distracted");
        assert_eq!(focus_label(-5_000).0, "Unfocused");
        assert_eq!(focus_label(0).0, "Not distracted");
        assert_eq!(focus_label(150).0, "Untroubled");
        assert_eq!(focus_label(250).0, "Level-headed");
        assert_eq!(focus_label(400).0, "Unfettered");
        assert_eq!(need_sentence("DrinkAlcohol", 300, None), "Level-headed after drinking");
        assert_eq!(need_sentence("Socialize", -20_000, None), "Distracted by being away from people");
        assert_eq!(need_sentence("PrayOrMeditate", -150_000, Some("Armok")), "Badly distracted by being unable to pray to Armok");
        assert_eq!(need_sentence("SomethingNew", 0, None), "Not distracted (something new)");
    }

    #[test]
    fn facets_and_beliefs() {
        assert_eq!(facet_sentence("ANXIETY_PROPENSITY", 95).unwrap(), "is very nervous and anxious");
        assert_eq!(facet_sentence("ANXIETY_PROPENSITY", 80).unwrap(), "is nervous and anxious");
        assert_eq!(facet_sentence("BRAVERY", 30).unwrap(), "is somewhat fearful in the face of danger");
        assert!(facet_sentence("BRAVERY", 50).is_none());
        assert_eq!(facet_sentence("LOVE_PROPENSITY", 5).unwrap(), "very rarely falls in love or develops positive feelings");
        assert_eq!(FACETS.len(), 50);
        assert!(FACETS.iter().all(|f| f.1.contains("{}") && f.2.contains("{}")));
        assert_eq!(value_sentence("CRAFTSMANSHIP", 45).unwrap(), "holds craftsmanship to be one of the highest ideals");
        assert_eq!(value_sentence("LAW", -30).unwrap(), "sees the law as a waste");
        assert!(value_sentence("FAMILY", 5).is_none());
    }

    #[test]
    fn emotions_have_valence() {
        assert_eq!(emotion_valence("SATISFACTION"), 1);
        assert_eq!(emotion_valence("annoyance"), -1);
        assert_eq!(emotion_valence("ASTONISHMENT"), 0);
        let mut w = World::synthetic(16, 16, 10);
        w.calendar = rf_core::Calendar { year: 250, year_tick: 5 * rf_core::TICKS_PER_DAY };
        assert_eq!(ago(&w, 250, 5 * rf_core::TICKS_PER_DAY), "today");
        assert_eq!(ago(&w, 250, 4 * rf_core::TICKS_PER_DAY), "yesterday");
        assert_eq!(ago(&w, 250, 0), "5 days ago");
        assert_eq!(ago(&w, 249, 0), "last year");
    }

    #[test]
    fn wraps_words() {
        assert_eq!(wrap("aaa bbb ccc ddd", 8, 2), vec!["aaa bbb", "  ccc", "  ddd"]);
        assert_eq!(wrap("short", 20, 2), vec!["short"]);
        assert_eq!(wrap("", 20, 2), vec![""]);
    }

    #[test]
    fn stress_ranks_are_ordered() {
        assert!(stress_rank(StressLevel::from_stress(200_000)) > stress_rank(StressLevel::from_stress(0)));
        assert!(stress_rank(StressLevel::from_stress(-200_000)) < stress_rank(StressLevel::from_stress(0)));
    }
}
