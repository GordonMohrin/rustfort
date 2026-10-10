//! [validation] Real event counts from DF's `gamelog.txt` (read only).
//!
//! DF appends every announcement to `gamelog.txt` in the install folder, without dates; the
//! only calendar markers are the season lines ("Spring has arrived!", "It is now summer.",
//! "Autumn has come.", "Winter is upon you.") and "** Loading Fortress **" after each load.
//! `scan` counts announcements between two line numbers (1-based, inclusive). The caller finds
//! the lines of two exports by counting season markers back from the live date
//! (docs/VALIDATION.md). The patterns are DF v50's announcement texts as they appear in
//! Gordon's log; conversation lines ("Name, Profession: ...") are skipped.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Default, Debug)]
pub struct GamelogEvents {
    pub seasons: usize,
    pub loads: usize,
    pub counts: BTreeMap<&'static str, usize>,
    /// "line: text" of every citizen death.
    pub citizen_deaths: Vec<String>,
}

const SEASONS: [&str; 4] = ["Spring has arrived!", "It is now summer.", "Autumn has come.", "Winter is upon you."];

const DEATHS: [&str; 16] = [
    " has been found dead",
    " has been struck down",
    " has bled to death",
    " has died of thirst",
    " has starved to death",
    " has drowned",
    " has suffocated",
    " has been crushed",
    " has died after colliding",
    " has been shot and killed",
    " has been murdered",
    " has died of old age",
    " has been impaled",
    " has been killed",
    " has burned to death",
    " has been encased",
];

/// The speaker part of "Name, Profession has ..." (None for "The goblin has ..." etc.).
fn named_subject(line: &str, verb_at: usize) -> Option<&str> {
    let who = &line[..verb_at];
    if who.starts_with("The ") || who.starts_with("A ") || who.starts_with("An ") || !who.contains(", ") || who.contains("(Tame)") {
        return None;
    }
    Some(who)
}

pub fn scan(path: &Path, from: usize, to: usize) -> GamelogEvents {
    let mut ev = GamelogEvents::default();
    for k in [
        "sieges", "ambushes", "migrant_waves", "dwarf_births", "animal_births", "caravans", "citizen_deaths",
        "moods", "artifacts", "masterpieces", "tantrums", "insane", "beasts",
        "migrant_no_wave", "migrant_refusals", // [calib11-econ]
    ] {
        ev.counts.insert(k, 0);
    }
    let Ok(f) = std::fs::File::open(path) else {
        eprintln!("cannot open {}", path.display());
        return ev;
    };
    let mut r = BufReader::with_capacity(1 << 20, f);
    let mut buf = Vec::new();
    let mut n = 0usize;
    loop {
        buf.clear();
        match r.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        n += 1;
        if n < from {
            continue;
        }
        if n > to {
            break;
        }
        // CP437 bytes above 127 only occur in names; replace them, the patterns are ASCII.
        let line = String::from_utf8_lossy(&buf);
        let line = line.trim_end_matches(['\r', '\n']);
        let mut bump = |k: &'static str| *ev.counts.entry(k).or_default() += 1;
        if SEASONS.contains(&line) {
            ev.seasons += 1;
            continue;
        }
        if line.starts_with("** Loading") {
            ev.loads += 1;
            continue;
        }
        // Conversation and thought lines: "Name, Profession: text".
        if line.find(": ").map(|p| p < 80).unwrap_or(false) {
            continue;
        }
        if line.starts_with("A vile force of darkness has arrived") {
            bump("sieges");
        } else if line.starts_with("An ambush!") {
            bump("ambushes");
        } else if line.contains("migrants have arrived") || line.contains("migrant has arrived") || line.contains("to brave this terrifying place") {
            bump("migrant_waves"); // [calib11-econ] incl. the danger variants
        } else if line.contains("attracted no migrants") {
            bump("migrant_no_wave"); // [calib11-econ]
        } else if line.contains("too wary to make the journey") || line.contains("refused to journey") || line.contains("cursed death-trap") {
            bump("migrant_no_wave");
            bump("migrant_refusals");
        } else if let Some(p) = line.find(" has given birth to ") {
            if named_subject(line, p).is_some() {
                bump("dwarf_births");
            } else {
                bump("animal_births");
            }
        } else if line.contains("caravan from") && line.contains("has arrived") {
            bump("caravans");
        } else if line.contains("has begun a mysterious construction") {
            bump("moods");
        } else if line.contains("has created a masterpiece") {
            bump("masterpieces");
        } else if line.contains(" has created ") && line.contains(", a") && line.contains('!') {
            bump("artifacts");
        } else if line.contains("throws a tantrum") || line.contains("is throwing a tantrum") {
            bump("tantrums");
        } else if line.contains("has gone stark raving mad") || line.contains("has gone berserk") || line.contains("has become a Melancholy") {
            bump("insane");
        } else if line.contains(" has come!") && (line.contains("Forgotten Beast") || line.contains("Titan") || line.contains("Demon")) {
            bump("beasts");
        } else if !line.contains("tendon") && !line.contains("ligament") {
            for d in DEATHS {
                if let Some(p) = line.find(d) {
                    if named_subject(line, p).is_some() {
                        bump("citizen_deaths");
                        ev.citizen_deaths.push(format!("{n}: {line}"));
                    }
                    break;
                }
            }
        }
    }
    ev
}
