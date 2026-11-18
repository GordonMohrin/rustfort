# [audio14] DF audio: music, ambience and event sounds (mechanics spec for v0.14)

Status: analysis by the mechanics analyst, 2026-11-18. This file is the implementer's only spec.
Everything marked **ESTIMATE** is a guess (no wiki/raws/observation backs the number); keep those
values in named `const`s so they can be tuned later. **MISSING** = DF has it, Rustfort cannot do it
yet (data or sim feature absent).

Sources used:

- w:Audio = https://dwarffortresswiki.org/index.php/Audio (v50 page: MUSIC/SOUND tokens, contexts, events)
- w:Soundtrack = https://dwarffortresswiki.org/index.php/Soundtrack (track list, trigger per track)
- w:Settings = https://dwarffortresswiki.org/index.php/Settings#Audio (volumes 0..255, song gap)
- raws = `<DF>/data/vanilla/vanilla_music/objects/music_standard.txt` and `sound_standard.txt`
  (NUMERIC_VERSION 5306, i.e. 53.06 module; read-only)
- init = `<DF>/data/init/init_default.txt` (SOUND, *_VOLUME, AVERAGE_TIME_BETWEEN_SONGS,
  TITLE_MUSIC) and the player's `<DF>/prefs/init.txt`
- notes = `<DF>/release notes.txt` (audio sections of 50.0x, 51.x, 53.07, 53.17)
- folder = the file listing of `<DF>/data/sound` of the reference install (DF 53.17, 1 729 `.ogg`
  files plus 7 FMOD `.bank` files). All `.ogg` files probed are **Ogg Vorbis**, mono or stereo,
  44.1 kHz or 48 kHz (no Opus).

Clean-room note: the mapping "FILE identifier -> file on disk" is hard-coded in DF. Rustfort must
**not** take it from the executable. It is derived below purely from the folder/file naming,
which is observable data. No DF audio file may ever be copied into the repo or a test fixture;
tests use generated Vorbis-free fixtures (see section 10).

---

## 0. Executive summary

DF has three independent audio layers, each with its own volume:

| Layer | DF volume token (fort / adv) | Default | What plays |
|---|---|---|---|
| Music | `MUSIC_VOLUME` / `MUSIC_VOLUME_ADV` | 255 / 217 | title song, context songs with "card" interludes, event songs |
| Ambience | `AMBIENCE_VOLUME` / `_ADV` | 230 / 240 | looping biome/underground/weather/situation beds + random one-shots |
| Sound effects | `SFX_VOLUME` / `_ADV` | 204 / 230 | announcement stingers (raws `SOUND` objects), mining, savage-area howls |
| User interface | `UI_VOLUME` / `_ADV` | 204 / 230 | button clicks (53.17: FMOD only, see 6) |
| Master | `MASTER_VOLUME` | 255 | multiplies everything |

Plus `[SOUND:YES|NO]` (global off) and `[AVERAGE_TIME_BETWEEN_SONGS:240]` (10..600 s).

Rustfort plan: a new app-only module `crates/rf-app/src/audio/` that (1) scans `<DF>/data/sound`
and parses the `MUSIC`/`SOUND` raws at runtime, (2) runs a pure, testable music director and
ambience mixer driven by facts the sim already exposes (+ one small read-only API file and an
off-by-default cue buffer), (3) plays through `rodio` with Vorbis only, (4) never opens an audio
device in bench/savebench/screenshot/script/test runs or when muted. The sim state hash must stay
**unchanged** (audio is presentation only; the cue buffer is `#[serde(skip)]`, not hashed, uses no
RNG, and is disabled unless the interactive app enables it).

---

## 1. The DF install's sound folder (what exists, what Rustfort uses)

Paths relative to `<DF>/data/sound/`. "Era" tells which DF version introduced the files
(file dates of the reference install): **classic** (pre-Steam), **premium** (50.01, Jan 2023),
**adv** (51.01 adventure mode, 2024/25), **fmod** (53.17, Nov 2026).

| Path | Files | Era | Content | Rustfort use |
|---|---|---|---|---|
| `song_title.ogg` | 1 | classic | Toady's title piece (mono 44.1k) | fallback title music when `tracks/` is absent |
| `song_game.ogg` | 1 | classic | Toady's in-game piece | fallback fort music when `tracks/` is absent |
| `tracks/<song_dir>/<P>_Full.ogg` | 14 dirs | premium | full fortress OST songs | **music** |
| `tracks/<song_dir>/<P>_<n>.ogg` | 4-5 per dir | premium | "cards" = short bits of that song | **music interludes** |
| `tracks/adv/<n>_<Title>.ogg` | 13 | adv | adventure OST, also used in fort mode by raws | **music** |
| `tracks/dwarf_fortress/Dwarf_Fortress.ogg` | 1 | premium | title theme (`TITLE_MUSIC:DWARF_FORTRESS`) | **title music** |
| `cards/Card_1..16.ogg` | 16 | premium | stand-alone interludes not tied to a song | **neutral interludes** (ESTIMATE use, see 3.6) |
| `sounds/*.ogg` | 20 | premium | announcement stingers + savage one-shots | **SFX** via `SOUND` raws |
| `ambience/*.ogg` | 27 | premium | fortress ambience beds (biomes, cavern, magma, river, weather, siege, combat, tavern, workshop, depot, evil/good/terrifying) | **ambience** (primary) |
| `audio/ambiance/regions/<region>/{base,neutral}.ogg` | 26 regions | adv | adventure region beds | ambience fallback / optional |
| `audio/ambiance/layers/{rain,river,volcano,walla/<site>}` | | adv | layered beds (light/heavy rain, river, volcano close/far/low, crowd "walla" low/medium/high) | optional extra layers |
| `audio/ambiance/alignments/{evil,savage}/neutral-00N.ogg` | 6 | adv | alignment beds | optional |
| `audio/one-shots/<region>[/<sub>]/one-shot-NNN.ogg` | ~900 | adv | random one-shots per region | optional ambience one-shots |
| `audio/songs/...`, `audio/combat-songs/or/...` | stems | adv | adventure dynamic-music stems (per-instrument layers) | **not used** (fort mode plays the baked `tracks/adv` versions, w:Soundtrack) |
| `audio/actions/dig.ogg` (+ others) | 21+ | adv | action sounds | **mining SFX** (ESTIMATE substitute for FMOD dig events) |
| `audio/combat/...`, `audio/locomotion/...`, `audio/doors/...` | many | adv | adventure SFX | optional, off by default |
| `audio/notifications/{alerts,map}/*.ogg` | 4 | adv | discovery / key item / battle / hunger pings | not used (adventure) |
| `audio/ui/...` | ~50 | adv | UI clicks/tabs/whooshes | **UI SFX** (ESTIMATE substitute, see 6) |
| `audio/spatialization.txt` | 1 | adv | `maxhearingdistance 10` | read: hearing radius for positional SFX |
| `audio/starting-volume.txt` | 1 | adv | per-category base gain: ambiance 1, ambiancelayer 1, oneshot 1, **song 0.85**, alignmentambiance 1, soundeffects 1, userinterface 1, notification 1 | read: base gain per bus |
| `audio/banks/Desktop/*.bank` | 7 | fmod | FMOD Studio banks (Master, SFX, ambiance, cards, songs, user-interface) | **not read** (proprietary container). See 9 MISSING. |

Format of the two `.txt` files: alternating lines `key` / `value` (value = float). Parse leniently
(trim, skip blanks, ignore unknown keys). If missing: defaults above.

### 1.1 FILE identifier -> path resolution (observed naming rules)

`music_standard.txt` references `FILE:<ID>` and `CARD:<ID>_CARD_<n>`; `sound_standard.txt`
references `FILE:DEFAULT_SOUND_<NAME>`. Resolve without any hard-coded file list:

1. **Premium fort songs.** Normalize every directory name under `tracks/` with
   `norm(s) = uppercase(s)` after replacing `&` by `AND`, dropping `!`, and mapping any run of
   non-alphanumerics to `_` (trim `_`). Examples: `drink_&_industry` -> `DRINK_AND_INDUSTRY`,
   `strike_the_earth!` -> `STRIKE_THE_EARTH`, `koganusan` -> `KOGANUSAN`, `danger_room` ->
   `DANGER_ROOM`, `nabidas` -> `NABIDAS`, `dwarf_fortress` -> `DWARF_FORTRESS`.
   `FILE:<ID>` matches the directory with `norm(dir) == ID`. Inside it the full track is the file
   ending in `_Full.ogg`; if there is none, the only `.ogg` (e.g. `Dwarf_Fortress.ogg`,
   `Nabidas.ogg`); if several and none `_Full`, the largest (ESTIMATE tie-break).
2. **Cards.** `CARD:<ID>_CARD_<n>` -> same directory as `<ID>`, the file whose stem ends in
   `_<n>` (e.g. `EXPANSIVE_CAVERN_CARD_3` -> `tracks/expansive_cavern/EC_3.ogg`).
3. **Adventure songs.** Files `tracks/adv/<n>_<Title>.ogg`: drop the `<n>_` prefix, drop any
   parenthesised part (`Bruising The Fat (Shar Ikal)` -> `Bruising The Fat`), then `norm` ->
   `STRIKE_IT_AGAIN`, `THE_CAPITAL`, `BRUISING_THE_FAT`, `LIFE_OF_ADVENTURE`,
   `BEYOND_THE_GATES`, `HAMLET_MEMOIRS`, `NIGHT_CREATURE`, `INTO_THE_PITS`, `SISTERS_OF_WAR`,
   `CANNIBAL`, `PONY_RIDER`, `TWO_COINS`, `LOSING_IS_FUN` (all 13 match the raws).
4. **Sounds.** `DEFAULT_SOUND_<NAME>` -> `sounds/<lowercase(NAME)>.ogg` (all 20 vanilla
   identifiers resolve: ADAMANTINE, AMBUSH, ARTIFACT_CREATED, BABY_BORN, CAVERN_BREAK,
   DEMON_ATTACK, MEGABEAST, SIEGE, STRANGE_MOOD, WEDDING, HOWL_1..5, GIANT_STEP_1..3 ->
   `Giant_Step_1.ogg`, so compare **case-insensitively**). `sounds/alert.ogg` and
   `sounds/demon_attack_add.ogg` have no raw object (see 4.3).
5. **Mod identifiers** (w:Audio "Identifiers", since 50.09): a mod's `sound/` folder may hold
   files named `sound_file_*.txt` (`[OBJECT:SOUND_FILE]`, `[SOUND_FILE:<id>] [FILE:<path>]`) and
   `music_file_*.txt` (`[OBJECT:MUSIC_FILE]`, `[MUSIC_FILE:<id>] [FILE:<path>] [AUTHOR:..]
   [TITLE:..] [LOOPS]`). Paths are relative to the mod folder (usually `sound/...ogg`).
   Mod identifiers win over the built-in naming rules for the same id.
6. Unresolvable id -> log once (`eprintln!` at catalog build, not per frame) and skip the entry.

Song titles for a "now playing" line: mod `TITLE`, else the adventure file title, else
title-cased directory name (`Drink & Industry`, `Strike The Earth!`). Short titles are facts,
not copied text.

### 1.2 Which raws are active

- Vanilla: `<DF>/data/vanilla/vanilla_music/objects/{music_*.txt,sound_*.txt}`.
- Mods: only the mods **active in the loaded world** (`World::meta.mods`, ids like
  `audible_alerts:1004`), looked up in `<DF>/data/installed_mods/<id> (<version>)/objects/` and
  `<DF>/mods/...` like the existing graphics/raws code does. On the title screen (no world) use
  vanilla only. Object ids defined twice: later mod in load order replaces earlier (DF's general
  raw rule, ESTIMATE for audio).
- Parse with `rf_raws::parse_file` (bracketed tokens only). Text outside brackets is a comment:
  e.g. the reference install's `audible_alerts` mod has four objects written without brackets;
  DF ignores them and so must Rustfort (test this, section 10).

---

## 2. Settings, mute and device policy

### 2.1 DF rules (init / w:Settings)

| Token | Range | Default (`data/init`) | Reference player prefs |
|---|---|---|---|
| `SOUND` | YES/NO | YES | YES |
| `MASTER_VOLUME` | 0..255 (255 = 100 %) | 255 | **0** (player has DF muted) |
| `MUSIC_VOLUME` | 0..255 | 255 | 113 |
| `AMBIENCE_VOLUME` | 0..255 | 230 | 103 |
| `SFX_VOLUME` | 0..255 | 204 | 108 |
| `UI_VOLUME` | 0..255 | 204 | 204 |
| `AVERAGE_TIME_BETWEEN_SONGS` | 10..600 s | 240 | 250 |
| `TITLE_MUSIC` | music file id | `DWARF_FORTRESS` | - |

Files are read in order `data/init/init_default.txt`, then `prefs/init.txt` (and the
`%APPDATA%/Bay 12 Games/Dwarf Fortress/prefs/init.txt` copy, same priority list as
`ui::round9::df_files`); last value wins (w:Settings).

Gap rule (init comment + w:Settings): "default of 240 gives a range of 180 to 300 seconds".
Rustfort: gap = uniform in `[avg * 3/4, avg * 5/4]` seconds. The ±25 % is **OK at 240**, the
proportional scaling for other values is **ESTIMATE**.

Gain: DF volume `v` is linear 0..255 -> `v / 255.0` (ESTIMATE that it is linear amplitude, FMOD
volumes are linear). Final gain of a voice:
`master/255 * bus/255 * starting_volume[category] * duck * positional`.

### 2.2 Rustfort design

`crates/rf-app/src/settings.rs`, new field (all lines `// [audio14]`):

```rust
#[serde(default)]
pub audio: AudioSettings,           // [audio14]

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AudioSettings {          // lives in audio/settings.rs, re-exported
    /// None = take DF's values (prefs, then data/init); Some = changed in Rustfort.
    pub from_df: bool,              // true until the player moves a slider
    pub muted: bool,                // global mute (Settings checkbox + key)
    pub master: u8, pub music: u8, pub ambience: u8, pub sfx: u8, pub ui: u8,
    pub avg_song_secs: u16,         // clamp 10..=600
    pub extra_sfx: bool,            // optional adventure-folder SFX (combat blows), default false
}
```

Defaults: `from_df: true`, values = DF `data/init` defaults (255/255/230/204/204/240). When
`from_df` is true, the values are re-read from the DF files at start (so the reference install
starts **silent** because its `MASTER_VOLUME` is 0; the Audio page shows the source line
"from your DF prefs" like the Announcements page does). `[SOUND:NO]` in DF => start muted with the
note "DF sound is off (init.txt SOUND:NO)"; the player can unmute in Rustfort.

UI: `crates/rf-app/src/ui/audio14.rs`, a new `menu::Page::Audio` (`// [audio14]` in `menu.rs`),
reached from a new "Audio" row on the Settings page (title screen and in-game Esc menu share
`Menu`). Rows: Mute checkbox, five sliders shown as 0-100 % (`round(v*100/255)`), "Average time
between songs" (10..600 s, step 10), "Extra sound effects (adventure files)" checkbox,
"Now playing: <title>" line, "Sound folder: <path> (<n> files)" or the reason audio is off.
`--menu-page audio` opens it (for screenshot tests). Rustfort-only key: none required; optional
`Ctrl+M` toggle mute (ESTIMATE, not DF).

### 2.3 Device policy (hard requirement)

The audio output device is opened **only** when all hold:

1. command is `play`, and
2. not `--screenshot`, not `--script`, not `--shots-at` (all headless/test driven), and
3. env `RUSTFORT_MUTE` is unset or `0`, and no `--mute` flag, and
4. `settings.audio.muted == false` and `master > 0`, and
5. a DF install with `data/sound` was found.

`bench`, `savebench`, `cargo test`, examples: the audio module is never constructed with a real
backend. Unmuting later in the session may open the device lazily then. `--mute` is added to
`parse_args` and the help text (`// [audio14]`). `RUSTFORT_MUTE=1` must also be honoured by the
in-game unmute (env wins: show "muted by RUSTFORT_MUTE"). Device open failure -> audio disabled
for the session, status line on the Audio page, no panic.

Rodio's `OutputStream` must stay on the main thread (not `Send` on all platforms): keep it in
`Game`/`Menu` owner struct (`audio: Option<AudioSystem>`), never move it to the sim thread.

---

## 3. Music

### 3.1 Raw format (w:Audio, raws)

```
[MUSIC:<id>]
  [FILE:<music file id>]             required
  [CARD:<music file id>]...          optional: "shuffled deck of short bits"
  [EVENT:<event>]...                 plays immediately when the event happens, overrides current song
  [CONTEXT:<context>]...             may be picked at random while ANY listed context holds
  [FREQUENCY:UNCOMMON|RARE]          UNCOMMON = half as often, RARE = 1/5 as often as other candidates
```

A MUSIC object with only EVENTs never plays at random (raws comment on TRACK_01).

### 3.2 Contexts (w:Audio) and how Rustfort evaluates them

| Context | DF meaning | Rustfort fact | Class |
|---|---|---|---|
| `ANY` | any time, also menus | always true (title screen too) | OK |
| `MAIN` | game mode is fortress mode | true while a fort is loaded (`Game`), false on title | OK |
| `FIRST_YEAR` | fort less than one year old | `fort_age_ticks < TICKS_PER_YEAR` (403 200) | OK rule / ESTIMATE data (3.2.1) |
| `SECOND_YEAR_PLUS` | fort more than one year old | `fort_age_ticks >= TICKS_PER_YEAR` | same |
| `CAVERNS_OPENED` | fort has access to the caverns | any non-magma `CavernLayer::discovered()` | ESTIMATE ("access" may mean breached; discovered = revealed tiles > 0; pick discovered, see note) |
| `SPRING`/`SUMMER`/`AUTUMN`/`WINTER` | current season | `world.calendar.month() / 3` = 0/1/2/3 | OK; wiki says SPRING..AUTUMN also hold in legends/main menu: on the title screen treat SPRING, SUMMER, AUTUMN as true, WINTER false (wiki, verify-tagged) |

Note CAVERNS_OPENED: DF's FIRST_CAVERN_OPENED event pairs with the FEATURE_DISCOVERY
announcement (sound CAVERN_BREAK), which fires when the cavern is first seen, so "discovered"
is the consistent choice.

#### 3.2.1 Fort age

DF counts from the fort's founding. Rustfort has no founding date (`rf_core::Meta` has none).
Design:

- rf-core (additive, `#[serde(default)]`, `// [audio14]`): `Meta::founded_year: i32` and
  `Meta::founded_tick: i32` (`-1` = unknown; serde default must be -1 -> use a
  `#[serde(default = "neg1")]` helper).
- Exporter (`tools/dfhack/rustfort-export.lua`, additive, read-only, documented in
  `docs/EXPORT_FORMAT.md` under `meta`): the fort site's creation year/tick. ESTIMATE field
  source: `df.global.world.world_data.sites` entry of `df.global.plotinfo.site_id`, fields
  `created_year`/`created_tick` (verify names in df-structures; wrap in `pcall`, write -1 when
  absent).
- Synthetic worlds and new embarks: `founded_* = calendar at sim tick 0`.
- `fort_age_ticks = (year - founded_year) * TICKS_PER_YEAR + (year_tick - founded_tick)`.
  Unknown founding on an imported fort: assume SECOND_YEAR_PLUS (ESTIMATE: imports are usually
  older forts; the reference fort Tileshtulon is several years old).

### 3.3 Events (w:Audio) and Rustfort detection

All detection runs in the app (`audio/director.rs`), fed once per frame after
`drain_announcements` in `Game::update` (`game.rs` near line 1118, one call
`crate::audio::on_frame(self, &fresh)` marked `// [audio14]`). The sim stays untouched except the
read-only API of section 7.

| Event | DF rule (w:Audio) | Rustfort trigger | Class |
|---|---|---|---|
| `JUST_EMBARKED` | founding a new fortress | first frame of a world whose `sim.ticks == 0` and `fort_age_ticks < TICKS_PER_DAY` (synthetic/new embark). Imported or loaded forts: no event | OK / ESTIMATE threshold |
| `SIEGE` | a siege is announced | an announcement classified `SIEGE` by `ui::round9::classify` (goblin "vile force" line, "An army of ... has arrived"); also an `UNDEAD_ATTACK` (necromancer siege, [undead13] text, match ESTIMATE "undead"/"the dead") | OK |
| `MEGABEAST_ATTACK` | megabeast arrival announced | announcement class `MEGABEAST` whose text does **not** contain "forgotten beast" (megabeasts/titans from [mega13]; semi-megabeasts: unknown per wiki -> no) | OK / ESTIMATE for titans |
| `FORGOTTEN_BEAST_ATTACK` | forgotten beast arrival announced | class `MEGABEAST` with "forgotten beast" (`caverns.rs` line "The Forgotten Beast X has come!") | OK |
| `FIRST_CAVERN_OPENED` | a new cave layer is discovered (wiki: verify) | caverns-discovered bitmask goes from 0 to non-zero during play (section 7 `AudioFacts::caverns_mask`). Initialise the previous mask from the first snapshot after load so loading a fort never fires it | ESTIMATE (first only) |
| `DEATH_SPIRAL` | many citizen deaths in short succession | ≥ `DEATH_SPIRAL_DEATHS = 5` announcements classified `DEATH` (citizen, via `round9::death_type`) within `DEATH_SPIRAL_WINDOW = 2 * TICKS_PER_MONTH` (67 200 ticks) of sim time; then re-arm only after `DEATH_SPIRAL_COOLDOWN = TICKS_PER_YEAR/4` (one season) | ESTIMATE (all three numbers) |
| `TAVERN_MUSIC_PRESENT` | many units gathered to perform/watch music | a `Perform` cue (section 7) of kind Music in a tavern/temple with audience ≥ `TAVERN_CROWD = 5`, at most once per `TAVERN_COOLDOWN = TICKS_PER_YEAR/4`; only while no other event song plays | ESTIMATE (threshold, cooldown) |
| `TAVERN_DANCE_PRESENT` | many units gathered for a dance | same with kind Dance. Rustfort's `perform` does not distinguish music from dance (MakeMerry vs HearEloquence only): treat every non-eloquent performance as Music, Dance never fires | MISSING (dance) |
| `LOST_FORT` | fortress abandoned/retired, adventurer died | living citizens drop from > 0 to 0 (`AudioFacts::citizens_alive`); also when the player chooses "Abandon" if Rustfort ever adds it. Plays KOGANUSAN / LOSING_IS_FUN | ESTIMATE (Rustfort has no abandon) |
| `FORT_LEVEL` | (51.01) may relate to higher fort ranks (wiki verify) | `Sim::land_title()` ([diplo13]) changes to a higher rank during play (compare against value at load) | ESTIMATE |
| `FIRST_GHOST` | (51.01) first time a ghost attacks (wiki verify) | `AudioFacts::ghosts_risen` goes from 0 to ≥ 1 during play (`pop.rep.ghosts_risen.len()`, saved state, so loads do not re-fire) | ESTIMATE |

Event handling rules:

- "When the chosen event occurs, this song automatically plays and overrides the current song.
  If multiple songs match the event, a random song will be played from among them." (w:Audio,
  paraphrased). Uniform choice among the event's MUSIC objects (FREQUENCY applies only to random
  context picks: ESTIMATE). Skip a candidate whose file did not resolve.
- Override: fade the current music out over `FADE_OUT_S = 1.5` s (ESTIMATE), start the event
  song from its beginning, no cards before an event song (ESTIMATE).
- If an event song is already playing and a new event comes: a different event overrides only if
  it has higher priority. Priority (ESTIMATE): LOST_FORT > SIEGE > FORGOTTEN_BEAST_ATTACK =
  MEGABEAST_ATTACK > DEATH_SPIRAL > FIRST_CAVERN_OPENED > FIRST_GHOST > FORT_LEVEL >
  TAVERN_* > JUST_EMBARKED. The same event while its song plays: ignored.
- After an event song ends, the normal gap timer restarts (3.5).
- Several events in one frame: highest priority wins, the others are dropped (ESTIMATE).

### 3.4 Vanilla music objects (raws 53.06) as Rustfort sees them

| MUSIC id | FILE (dir / file) | Cards | EVENT | CONTEXT | FREQ |
|---|---|---|---|---|---|
| TRACK_01 | KOGANUSAN (`koganusan/KG_Full`) | - | LOST_FORT | - | |
| TRACK_02 | EXPANSIVE_CAVERN (`EC_*`) | 4 | FIRST_CAVERN_OPENED | CAVERNS_OPENED | |
| TRACK_03 | DEATH_SPIRAL (`DS_Full`) | - | DEATH_SPIRAL | - | |
| TRACK_04 | HILL_DWARF (`HD_*`) | 5 | - | MAIN | |
| TRACK_05 | FORGOTTEN_BEAST (`FB_*`) | 5 | MEGABEAST_ATTACK, FORGOTTEN_BEAST_ATTACK | - | |
| TRACK_06 | DRINK_AND_INDUSTRY (`DI_Full`) | - | TAVERN_MUSIC_PRESENT, TAVERN_DANCE_PRESENT | - | |
| TRACK_07 | VILE_FORCE_OF_DARKNESS (`VFOD_*`) | 5 | SIEGE | - | |
| TRACK_08 | FIRST_YEAR (`FY_*`) | 4 | - | FIRST_YEAR | |
| TRACK_09 | ANOTHER_YEAR (`AY_*`) | 5 | - | SECOND_YEAR_PLUS | |
| TRACK_10 | STRIKE_THE_EARTH (`STE_*`) | 4 | JUST_EMBARKED | MAIN | UNCOMMON |
| TRACK_11 | STRANGE_MOODS (`SM_Full`) | - | - | MAIN | |
| TRACK_12 | WINTER_ENTOMBS_YOU (`WEY_*`) | 4 | - | WINTER | |
| TRACK_13 | CRAFTSDWARFSHIP (`CS_*`) | 5 | - | ANY | |
| TRACK_14 | MOUNTAINHOME (`MH_*`) | 5 | - | ANY | |
| ADV_OST_TRACK_01 | STRIKE_IT_AGAIN | - | JUST_EMBARKED | MAIN | UNCOMMON |
| ADV_OST_TRACK_02 | THE_CAPITAL | - | - | SECOND_YEAR_PLUS | |
| ADV_OST_TRACK_03 | BRUISING_THE_FAT | - | SIEGE | - | |
| ADV_OST_TRACK_04 | LIFE_OF_ADVENTURE | - | - | ANY | |
| ADV_OST_TRACK_05 | BEYOND_THE_GATES | - | SIEGE, MEGABEAST_ATTACK, FORGOTTEN_BEAST_ATTACK | - | |
| ADV_OST_TRACK_06 | HAMLET_MEMOIRS | - | FORT_LEVEL | - | |
| ADV_OST_TRACK_07 | NIGHT_CREATURE | - | FIRST_GHOST | - | |
| ADV_OST_TRACK_08 | INTO_THE_PITS | - | - | ANY | |
| ADV_OST_TRACK_09 | SISTERS_OF_WAR | - | TAVERN_MUSIC_PRESENT, TAVERN_DANCE_PRESENT | - | |
| ADV_OST_TRACK_10 | CANNIBAL | - | FIRST_CAVERN_OPENED | CAVERNS_OPENED | |
| ADV_OST_TRACK_11 | PONY_RIDER | - | MEGABEAST_ATTACK, FORGOTTEN_BEAST_ATTACK | - | |
| ADV_OST_TRACK_12 | TWO_COINS | - | - | ANY | |
| ADV_OST_TRACK_13 | LOSING_IS_FUN | - | DEATH_SPIRAL | - | |
| DANGER_ROOM | DANGER_ROOM (`DR_Full`) | - | TAVERN_MUSIC_PRESENT, TAVERN_DANCE_PRESENT | - | |
| NABIDAS | NABIDAS (`Nabidas`) | - | - | ANY | |

These come from the raws at runtime; the table is only for test expectations. Note:
`music_standard.txt` puts free text after tokens on the same line (`[CARD:..] for the shuffled
deck ...`); the tokenizer ignores it.

Resulting random pools (vanilla), in a fort:
- always: CRAFTSDWARFSHIP, MOUNTAINHOME, LIFE_OF_ADVENTURE, INTO_THE_PITS, TWO_COINS, NABIDAS (ANY);
  HILL_DWARF, STRANGE_MOODS, STRIKE_THE_EARTH (U), STRIKE_IT_AGAIN (U) (MAIN);
- first year: FIRST_YEAR; later: ANOTHER_YEAR, THE_CAPITAL;
- winter: WINTER_ENTOMBS_YOU; caverns: EXPANSIVE_CAVERN, CANNIBAL.
On the title screen: the ANY pool only (no MAIN/year/caverns/winter).

### 3.5 Random (context) selection: exact algorithm

Weights (w:Audio "half as often" / "1/5 as often"): normal 10, UNCOMMON 5, RARE 2. OK (ratios
from the wiki; a Steam thread doubts FREQUENCY works at all, keep the wiki rule).

State machine (`audio/director.rs`, pure, time injected as `f64` seconds of wall time):

```
Silent { until: t }        -> when now >= until: pick slot
PlayingCards { song, queue: SmallVec<[u8; 8]>, idx }  -> card ended: next card or start song
PlayingSong { music_id, event: Option<Event> }        -> ended: Silent { until: now + gap() }
PlayingInterlude { card }                              -> ended: Silent { until: now + gap() }
FadingOut { next }                                     -> after FADE_OUT_S: start `next`
```

Picking a slot when the gap elapsed:
1. Candidates = MUSIC objects with ≥ 1 CONTEXT true now and a resolved FILE.
2. Remove the song that played last (no immediate repeat) if ≥ 2 candidates (ESTIMATE).
3. With probability `NEUTRAL_INTERLUDE_P = 1/4` (ESTIMATE) and if `cards/Card_*.ogg` exist and
   context is MAIN: play one random neutral card as an interlude (PlayingInterlude).
4. Else weighted pick of a song S. If S has cards:
   - first time S is picked in this fort (session set keyed by MUSIC id; optional persistence in
     settings.json keyed by fort name, ESTIMATE): play **all** its cards in order 1..n, then S;
   - later: play `1 + rand(3)` (1..=3, ESTIMATE "a few") distinct random cards drawn from S's
     shuffled deck (deck = permutation, refilled and reshuffled when empty), then S.
   Source: notes 50.0x "Now only plays a few random cards for a song on subsequent playthroughs
   rather than playing them in order each time"; w:Audio CARD: "Before the song proper starts,
   the game will play a few of these in random order" (verify-tagged). The cards-then-song chain
   is **ESTIMATE**; the gap between cards and between last card and song is 0 s (ESTIMATE).
5. If no candidate: stay Silent and retry after 5 s (ESTIMATE).

Gap `gap() = uniform(avg*0.75, avg*1.25)` seconds (2.1). Start of a session in a fort (load or
import, no JUST_EMBARKED): first slot after `uniform(5, 15)` s (ESTIMATE). Music keeps playing
while the game is paused (ESTIMATE; DF observation suggests yes).

Title screen: play `TITLE_MUSIC` (init; default `DWARF_FORTRESS`) once from the start, then the
normal ANY pool with gaps. Entering a fort: fade the title music out (1.5 s) and start the fort
logic (JUST_EMBARKED or the 5-15 s first slot). Returning to the title: fade out, title music
again (ESTIMATE).

Classic fallback (no `tracks/` folder, e.g. classic DF): title -> `song_title.ogg`, fort ->
`song_game.ogg` with the same gap logic, no events.

Random source: the app-side director owns a tiny splitmix64 seeded from wall time (this is UI,
not sim, so wall-clock seeding is allowed); tests seed it explicitly. Never touch `rf_sim::rng`.

### 3.6 Neutral cards

`cards/Card_1..16.ogg` are not referenced by any raw. DF 53.17's FMOD cards bank has a
"Neutral" cards event next to one per song (name fragments in `Master.strings.bank`), which
suggests stand-alone interludes. w:Soundtrack also says interludes play intermittently.
Use as in 3.5 step 3. ESTIMATE in full.

### 3.7 Ducking

DF 53.17 has an FMOD snapshot `Music_Active` (mixes ambience/SFX "based on music volumes and
states", notes 53.17). Rustfort: while a song or card plays, ambience gain x `AMBIENCE_DUCK =
0.6` (ESTIMATE), ramped over 1 s.

---

## 4. Sound effects (announcement stingers and one-shots)

### 4.1 Raw format (w:Audio)

```
[SOUND:<id>]
  [FILE:<sound file id>]          required
  [ANNOUNCEMENT:<announcement>]... trigger: plays when any listed announcement type fires
  [SAVAGE_AREA]                   played randomly in a savage area
```

### 4.2 Vanilla sound objects and Rustfort triggers

Rustfort announcements carry text + `AnnouncementKind`, no DF token. The app maps them with
`ui::round9::classify` (+ `death_type`). Add a table in `audio/sfx.rs`: Rustfort class ->
DF announcement tokens it stands for. Then the generic rule "play every SOUND whose
ANNOUNCEMENT list contains one of those tokens" also works for mods (e.g. `audible_alerts`).

| SOUND (raws) | FILE | DF announcements | Rustfort source | Class |
|---|---|---|---|---|
| ADAMANTINE | sounds/adamantine | STRUCK_DEEP_METAL | not announced by the sim. App: dig cue (7) whose material is the deep metal ore (`MaterialInfo` token `ADAMANTINE` / inorganic flag `DEEP_SPECIAL`), first one per session (ESTIMATE; DF announces it once) | ESTIMATE |
| AMBUSH | sounds/ambush | AMBUSH_THIEF_SUPPORT_SKULKING, AMBUSH_THIEF_SUPPORT_NATURE, AMBUSH_THIEF_SUPPORT, AMBUSH_SNATCHER_SUPPORT, AMBUSH_AMBUSHER_NATURE, AMBUSH_AMBUSHER | class `AMBUSH` -> AMBUSH_AMBUSHER. Lone THIEF / SNATCHER map to AMBUSH_THIEF / AMBUSH_SNATCHER: **no sound** (raws bind only the *_SUPPORT variants). Rustfort cannot tell supported thieves: MISSING | OK / MISSING |
| ARTIFACT_CREATED | sounds/artifact_created | MADE_ARTIFACT | class `ARTIFACT` | OK |
| BABY_BORN | sounds/baby_born | BIRTH_CITIZEN | class `BIRTH` only for "has given birth" (not "has grown up", which is GROWN_UP_CITIZEN or similar: no sound) | OK |
| CAVERN_BREAK | sounds/cavern_break | FEATURE_DISCOVERY | sim does not announce it. App: each new bit in `caverns_mask` (every newly discovered non-magma layer; the magma sea is also a "feature" but ESTIMATE: no) | ESTIMATE |
| DEMON_ATTACK | sounds/demon_attack | ENDGAME_EVENT_2 | Rustfort has no underworld breach | MISSING |
| MEGABEAST | sounds/megabeast | MEGABEAST_ARRIVAL, WEREBEAST_ARRIVAL | class `MEGABEAST` (forgotten beasts included, ESTIMATE that FBs use MEGABEAST_ARRIVAL); werebeast arrival from [vampire13]: add `classify` keyword "werebeast" only if their text is not already a class (coordinate via `round13_api` text, ESTIMATE) | OK / ESTIMATE |
| SIEGE | sounds/siege | UNDEAD_ATTACK | necromancer siege from [undead13] only. A goblin siege has **no** raws sound (its stinger in DF is music event SIEGE + Siege ambience) | OK |
| STRANGE_MOOD | sounds/strange_mood | STRANGE_MOOD | class `MOOD` (all mood starts: fey, secretive, possessed, fell, macabre) | OK |
| WEDDING | sounds/wedding | MARRIAGE | sim has no marriage announcement (social pairs exist, no line) | MISSING |
| HOWL_1..5, GIANT_STEP_1..3 | sounds/Howl_N, Giant_Step_N | - | `SAVAGE_AREA`: 4.4 | ESTIMATE |

Mapping table Rustfort class -> DF tokens (complete; only these classes can trigger SOUNDs;
also allows mods like audible_alerts to work):

```
SIEGE        -> (none; music event only)  + UNDEAD_ATTACK if text matches undead
AMBUSH       -> AMBUSH_AMBUSHER
THIEF        -> AMBUSH_THIEF
SNATCHER     -> AMBUSH_SNATCHER
ABDUCTED     -> CITIZEN_SNATCHED
MEGABEAST    -> MEGABEAST_ARRIVAL (+ WEREBEAST_ARRIVAL if "werebeast")
DEATH        -> CITIZEN_DEATH        PET_DEATH -> PET_DEATH
MIGRANTS     -> MIGRANT_ARRIVAL, D_MIGRANTS_ARRIVAL
CARAVAN      -> CARAVAN_ARRIVAL      VISITOR -> GUEST_ARRIVAL
MOOD         -> STRANGE_MOOD         MOOD_CLAIM -> MOOD_BUILDING_CLAIMED, ARTIFACT_BEGUN
ARTIFACT     -> MADE_ARTIFACT        MASTERPIECE -> MASTERPIECE_CRAFTED
BIRTH        -> BIRTH_CITIZEN (birth lines only)
TANTRUM      -> CITIZEN_TANTRUM, BERSERK_CITIZEN
NOBLES       -> NEW_MANDATE          GHOST -> GHOST_ATTACK
CAVE_IN      -> CAVE_COLLAPSE        TRAPS -> ANIMAL_TRAP_CATCH   JOB -> CANCEL_JOB
(app-detected) FEATURE_DISCOVERY, STRUCK_DEEP_METAL
```

(Reuse `ANN_TYPES[t].df` where it already lists the tokens; extend with the extra ones in
`audio/sfx.rs`, not in round9.rs.)

Rules:
- One stinger per (SOUND id) per `SFX_DEDUP_S = 2.0` s of wall time (ESTIMATE): a siege or a
  death wave produces many lines in one frame; DF plays the sound once.
- Stingers are not positional (DF alerts are global; ESTIMATE). Gain category `soundeffects`.
- Respect the announcement's own settings? DF plays the sound regardless of display flags
  (ESTIMATE). Rustfort: play it even when the line is hidden (`show == false`).
- Announcements drained while the sim ran at "max" speed for a long frame can be many: process
  at most 8 stingers per frame (ESTIMATE, dedup makes this rarely matter).

### 4.3 `alert.ogg` and `demon_attack_add.ogg`

Both exist without raws. FMOD names show an "Alerts/Alert" event. ESTIMATE use: play
`sounds/alert.ogg` when the hostile alert box (`ui::alerts`, title "Siege!", "Ambush!", "A
monster has come!", "Hostile creatures") opens and no SOUND object fired for that alert in the
same frame. `demon_attack_add.ogg` is byte-identical in size to `demon_attack.ogg`: ignore.

### 4.4 Savage-area one-shots

"Played randomly in a savage area" (w:Audio); notes 50.0x: "Made the wild ambience use some
randomly spaced howls and giant footsteps". Rustfort:
- Condition: `World::climate.savagery >= SAVAGE_MIN = 66` (wiki Surroundings: savage = top third,
  ESTIMATE cut-off) and the camera z is at/above the surface at the view centre (3.8 depth = 0).
  Climate is MISSING in exports today ([contract13] FIDELITY row) -> feature dormant until the
  exporter fills it.
- Interval: uniform `SAVAGE_GAP = 45..150` s wall time (ESTIMATE). Pick uniformly among all SOUND
  objects with `SAVAGE_AREA`. Random stereo pan in [-0.6, 0.6] (ESTIMATE).

### 4.5 Mining

Notes 53.17: "spatialized mining sounds come from where your miners are actually working,
muffled by the stone around them"; "every mining breakthrough sounds different depending on
what was uncovered: stone, gems, ice, water, magma, caverns". The FMOD names list
`SFX/Digging/Tiles/` events (Adamantine, Cave, Dig Loop, Lava, RampStair, Stone,
Tile_Reveal_Empty, Water, ...) and parameters "Digging/Occlusion", "Tile Type". Those sounds
exist **only inside SFX.bank**: MISSING as files. Rustfort substitute (ESTIMATE):
- On each `AudioCue::Dig { pos, class }` (section 7) play `audio/actions/dig.ogg` positional.
- Breakthrough: if the dug tile opened into a cavern -> already covered by CAVERN_BREAK; into
  water/magma -> no file (MISSING).
- Rate limit: ≤ `DIG_MAX_VOICES = 3` concurrent, min `DIG_MIN_GAP_S = 0.12` s between starts.

### 4.6 Positional model (for dig and optional extra SFX)

`audio/spatialization.txt`: `maxhearingdistance 10` (adventure, unit = tiles). Fortress camera
version (ESTIMATE):
- `dz = |src.z - view.z|`; inaudible if `dz > 2`; z gain `[1.0, 0.45, 0.2][dz]`
  ("muffled by stone": additionally `low_pass(1200 Hz)` when `dz > 0`; rodio `Source::low_pass`).
- Horizontal: `d` = distance in tiles from the view rectangle (0 inside the visible rect);
  gain `max(0, 1 - d / 10)` using maxhearingdistance (OK value, ESTIMATE use).
- Pan = `clamp((src.x - view_center.x) / (view_half_width), -1, 1) * 0.8`.

### 4.7 Optional extra SFX (off by default)

`extra_sfx` setting: combat blows from `audio/combat/melee/<mat>` (bone/flesh/glass/metal/
whip/wood), blocks/parries, projectile fire/hit. DF fortress mode does not play these (they are
adventure assets): **Rustfort extension**, default off. Needs `AudioCue::Strike` (7). Limit
`STRIKE_MAX_PER_S = 6`.

---

## 5. Ambience

### 5.1 DF rules (as far as documented)

- w:Audio: "dynamic ambient sounds depending on the setting, biome and layer of the player's view";
  weather sounds, wild ambience, building sounds, river.
- notes 50.0x: ambiences continue where they left off (not restarted); outdoor ambiences alternate
  between more active beds and a few neutral wind beds; neutral cavern ambience; biome-specific
  wild ambience; old wild ambience moved to "terrifying" areas.
- notes 53.17: biome ambience mixes itself as conditions change; "as you dig down, the surface
  sounds fade away and the underground sounds take over".
- 53.17 FMOD parameters (names only, from `Master.strings.bank`): Biome, Depth, Camera Depth (UI),
  Alignment_Evil/Good/Savage, Neutrality, Rain Intensity, Snow Intensity, River Intensity,
  Magma Intensity, Combat Intensity, Siege Intensity, Walla, Occlusion. These name the inputs;
  their curves are unknown (ESTIMATE all numbers below).

### 5.2 Rustfort layers (files from `ambience/`, the fortress-mode set)

Each layer = one looping sink with a target gain; gain ramps at `AMB_RAMP_PER_S = 0.5`
(full fade in 2 s, ESTIMATE). A layer at gain 0 is **paused** (rodio `Sink::pause`) so it resumes
where it left off (notes 50.0x, OK). At most `AMB_MAX_LAYERS = 4` audible at once (highest target
gains win).

Inputs computed every `AMB_PERIOD_S = 0.5` s from the camera (view centre `c`, view z `vz`):

- `depth` = number of z-levels between `vz` and the surface at `c` (first z from the top whose
  tile at (c.x, c.y) is not open air/`OUTSIDE`); 0 = at/above surface. `outside` = tile flag
  `OUTSIDE` at (c.x, c.y, vz).
- `cavern` = `vz` within `[z_min, z_max]` of a discovered non-magma `CavernLayer`.
- `magma_d` = Chebyshev distance (x, y, and z*3) from `c@vz` to the nearest magma tile, searched in
  a 31x31x3 box (`liquid & MAGMA`), using a cached per-frame scan; none = inf.
- `river` = count of water tiles with `OUTSIDE` and not `STAGNANT`, depth ≥ 1, inside the view
  rect at `vz` and `vz-1` (sampled every 2nd tile).
- `weather` = `Sim::weather_summary().kind` ([weather13]; Clear until they implement it).
- `siege` = any hostile wave active (app `update_hostiles` list non-empty with ≥ 6 people, or
  `threats()` of kind siege) ; `combat` = ≥ 1 `AudioCue::Strike` within view in the last 3 s, or
  combat log grew by ≥ 3 lines in the last 3 s (cheap counter, section 7).
- `room` = the location/building under the view centre: tavern (`location_at`), trade depot,
  workshop within 6 tiles (ESTIMATE radius).
- `alignment` from `World::climate` (evil, good, savagery).

Layer targets (gain 0..1 before bus volume):

| Layer file (`ambience/`) | Target | Class |
|---|---|---|
| biome bed: `Forest`, `Grasslands`, `Desert`, `Glacier`, `Swamp`, `Rainforest` by `climate.biome` (FOREST_TAIGA/TEMPERATE_* -> Forest, FOREST_TROPICAL_* -> Rainforest, GRASSLAND_*/SAVANNA_*/SHRUBLAND_* -> Grasslands, DESERT_* -> Desert, GLACIER/TUNDRA -> Glacier, SWAMP_*/MARSH_* -> Swamp, unknown/MOUNTAIN/other -> `Outside`) | `surface_w` = 1 at depth 0, `max(0, 1 - depth/4)` below (fades over 4 levels, ESTIMATE). Alternates with `Neutral_Winds` / `Neutral_Winds_2`: every `uniform(60,120)` s swap active <-> neutral (notes 50.0x, interval ESTIMATE) | OK idea / ESTIMATE numbers |
| `Evil` / `Good` / `Terrifying` | replaces the biome bed when climate evil (`Terrifying` if evil and savagery ≥ 66) / good | ESTIMATE (climate MISSING in exports) |
| `Blizzard` | weather Snow and `surface_w > 0` -> `surface_w` | ESTIMATE |
| `Thunderstorm` | weather Rain -> `surface_w * 0.8` (no rain intensity in sim) | ESTIMATE |
| `Cavern` / `Neutral_Cavern` | `cavern` -> 1 (alternate like surface beds); underground non-cavern -> `Neutral_Cavern` at 0.35 | ESTIMATE |
| `Magma_Close` / `Magma_Far` / `Magma_Low` | `magma_d <= 3` -> Close 1; `<= 10` -> Far `1 - (d-3)/7`; view inside the magma sea layer -> Low 1 | ESTIMATE |
| `River_Low` / `River_Medium` / `River_High` | river tiles 1-15 Low, 16-60 Medium, > 60 High, gain `surface_w` | ESTIMATE |
| `Siege` | siege active -> 0.8 | ESTIMATE |
| `Combat` | combat in view -> 1, ramp down over 3 s | ESTIMATE |
| `Tavern` | view centre in a tavern location -> 0.9 | ESTIMATE |
| `Trade_Depot` | within 6 tiles of a depot at `vz` -> 0.8 | ESTIMATE |
| `Workshop` | ≥ 1 workshop with an active job within 6 tiles -> 0.6 | ESTIMATE |
| `Outside` | `outside` and no biome resolved -> `surface_w` | ESTIMATE |

Fallback when `ambience/` is missing but `audio/ambiance/regions/` exists: map the same
decisions to `regions/<region>/base.ogg` (active) and `neutral.ogg` (neutral), regions
`forest`/`grassland`/`desert`/`glacier`/`tundra`/`wetland`/`mountains`/`cavern`/`fortress`/`tavern`,
rain to `layers/rain/{light,heavy}-rain.ogg`, magma to `layers/volcano/{close,far,low}.ogg`,
river to `layers/river/river.ogg`. One-shots from `audio/one-shots/<region>/` every
`uniform(8, 25)` s at gain 0.5 (category `oneshot`, ESTIMATE) - only in this fallback mode, so the
default mode stays close to fortress-mode DF.

Ambience on the title screen: none (DF's main menu has an `Outside_MainMenu` FMOD event; the
loose `audio/ambiance/layers/character-creation/outside.ogg` is the nearest file: ESTIMATE play it
at 0.4 on the title screen only if `ambience/` is absent). Keep simple: off in the first version.

### 5.3 Walla (crowd)

FMOD `Walla_Fort/Fortress/...` and loose `layers/walla/fortress/walla-{low,medium,high}.ogg`:
crowd murmur. Rustfort optional (priority 6): citizens within the view at `vz`: 3-9 low,
10-24 medium, ≥ 25 high, gain 0.5. ESTIMATE.

---

## 6. User interface sounds

53.17 added "unique sounds for every button along the edge of the screen" with their own volume
(`UI_VOLUME`) - FMOD only (`user-interface.bank`; names: OpenMenu, CloseMenu, Confirm, Pause,
Orders, Labor, Nobles, Justice, Settings, ...). MISSING as files. Rustfort substitute
(ESTIMATE): `audio/ui/clicks/generic/click-00{1,2}.ogg` for buttons, `generic-small` for list rows
and checkboxes, `audio/ui/tab/tab-001.ogg` for tab switches, `audio/ui/whoosh` for opening/closing
full-screen menus, `clicks/confirm` for confirm dialogs. Hook: one call in the shared UI button
helper (`ui::UiCtx` click detection, `// [audio14]`), pushing into a per-frame `ui_sounds` queue
that the audio system plays (dedup: one UI sound per frame). Priority 5.

---

## 7. Sim side: read-only facts and the cue buffer

New file `crates/rf-sim/src/audio14_api.rs` (`pub mod audio14_api;` in `lib.rs` with
`// [audio14]`). No new saved state, no RNG, no hash change.

```rust
/// [audio14] Cheap facts for the app's music/ambience logic (one call per frame, no allocation).
#[derive(Clone, Copy, Debug, Default)]
pub struct AudioFacts {
    pub ticks: u64,
    /// Bit `id` set = cavern layer `id` (1..=31, non-magma) discovered.
    pub caverns_mask: u32,
    pub magma_sea_discovered: bool,
    pub citizens_alive: u32,
    pub ghosts_risen: u32,              // pop.rep.ghosts_risen.len()
    pub combat_lines: u64,              // monotonic count of combat log lines ever written
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioCue {
    /// A tile was dug (`dig_tile`): position and tiletype material class of the dug tile,
    /// plus the material index (for adamantine detection).
    Dig { pos: Pos, mat: u16, class: DigSound },
    /// A performance happened (`locations::perform`): location id, music (true) or poetry/story.
    Perform { loc: i32, music: bool, audience: u16 },
    /// A blow landed (`combat::strike`), for the optional extra SFX and the combat layer.
    Strike { pos: Pos, kind: StrikeSound },
}
pub enum DigSound { Stone, Ore, Gem, Soil, Ice, Wood, Other }   // from dig_class()
pub enum StrikeSound { Metal, Wood, Bone, Flesh, Glass, Whip, Wrestle }

impl Sim {
    pub fn audio_facts(&self) -> AudioFacts;
    /// Enable/disable cue recording (default off; the buffer is not saved).
    pub fn set_audio_cues(&mut self, on: bool);
    /// Move pending cues into `out` (clears the buffer, keeps capacity).
    pub fn drain_audio_cues(&mut self, out: &mut Vec<AudioCue>);
}
```

Sim field (in `lib.rs` `struct Sim`, `// [audio14]`):

```rust
#[serde(skip)]
pub(crate) audio_cues: crate::audio14_api::CueBuf,   // { on: bool, buf: Vec<AudioCue> }
```

`CueBuf::push(&mut self, c)`: `if !self.on || self.buf.len() >= 1024 { return; } self.buf.push(c)`.
Capacity reserved once when enabled (1024), so pushes never allocate. `after_exact_load`
needs nothing (skip -> `Default` -> off; the app re-enables after a load).

Hook lines (each one line, `// [audio14]`):
- `lib.rs::dig_tile` after the tile changes: `self.audio_cues.push(AudioCue::Dig{..})`
  (dig completion rate is low: no perf concern).
- `locations.rs::perform` once per performer after the audience loop (needs the audience count of
  that performer: count in the existing loop with a local `u16`).
- `combat.rs::strike` at the hit-confirmed point: `if self.audio_cues.on { ... }` guard first so
  the disabled path is one predictable branch (strike is hot in big fights; the
  `perf::prof::STRIKE` guard shows the budget).

Per-phase/period: there is **no new phase in `tick_inner`** and no periodic sim work. Facts are
computed on demand by the app thread while it owns the `Sim` (between worker chunks; see
`simthread.rs`), so no locking.

`audio_facts` cost: caverns loop over ≤ ~4 layers, `citizens_alive` - **do not** iterate units
every frame: reuse an existing counter if present (`stats.active_units` is not citizens);
otherwise compute only every 1 s in the app (the facts struct is cheap enough at 1 Hz; ≤ 300
units). `combat_lines`: add a `u64` counter to the combat log push site **only if** it already
exists as saved state; otherwise derive from cues (`Strike` count) - do not add saved fields.

Determinism checks: the bench never enables cues -> identical code path except a `bool` test;
`state_hash` does not include `audio_cues`; `saveload` example IDENTICAL (field skipped).
Expect the bench hash **unchanged**. Speed: ticks/s within noise (one branch per dig/strike).

Thread note: if the app enables cues and the sim runs on the worker, cues accumulate there and
are drained when the UI takes the sim back each frame (bounded 1024; overflow drops newest,
acceptable for audio).

---

## 8. App architecture (files, structs, hooks)

```
crates/rf-app/src/audio/mod.rs        AudioSystem facade, on_frame(), on_menu_frame(), mute policy
crates/rf-app/src/audio/catalog.rs    scan data/sound + parse MUSIC/SOUND/MUSIC_FILE/SOUND_FILE raws,
                                      init.txt volumes, starting-volume.txt, spatialization.txt (pure)
crates/rf-app/src/audio/director.rs   music state machine, contexts, events, weights, cards (pure)
crates/rf-app/src/audio/ambience.rs   layer targets from AmbInputs (pure) + input gathering from World
crates/rf-app/src/audio/sfx.rs        class->DF token table, SOUND triggers, dedup, savage timer,
                                      positional gain (pure)
crates/rf-app/src/audio/out.rs        Backend trait; RodioBackend (cfg feature "audio"); NullBackend
crates/rf-app/src/audio/settings.rs   AudioSettings, DF init import
crates/rf-app/src/ui/audio14.rs       Settings > Audio page
```

`mod audio; // [audio14]` in `main.rs`; `pub mod audio14; // [audio14]` in `ui/mod.rs`.

Backend trait (only `out.rs` touches rodio):

```rust
pub trait Backend {
    fn play(&mut self, bus: Bus, path: &Path, opts: PlayOpts) -> Option<VoiceId>; // streams from disk
    fn set_gain(&mut self, v: VoiceId, gain: f32);
    fn pause(&mut self, v: VoiceId, paused: bool);
    fn stop(&mut self, v: VoiceId);
    fn finished(&self, v: VoiceId) -> bool;
}
pub enum Bus { Music, Ambience, Sfx, Ui }
pub struct PlayOpts { pub looped: bool, pub gain: f32, pub pan: f32, pub low_pass: Option<u32> }
```

`RodioBackend`: one `OutputStream` + `OutputStreamHandle`; one `Sink` per voice (`Sink::try_new`),
music/ambience **streamed** (`Decoder::new(BufReader::new(File::open(p)?))`, `repeat_infinite()`
for loops - ambience files are 0.5-7 MB, streaming avoids decoding 60 MB up front); SFX decoded once
into a small LRU cache of `Buffered<Decoder<..>>` (≤ 32 entries) keyed by path. Pan via
`rodio::source::Spatial` or channel volume (ESTIMATE, simplest: `ChannelVolume`). Voices capped:
music 2 (for fades), ambience 6, SFX 8, UI 2. `NullBackend`: records calls (for tests).

Per-frame flow (`Game::update` after announcements; `Menu` frame for the title):
1. `facts = sim.audio_facts()` (1 Hz for the citizen count), `sim.drain_audio_cues(&mut buf)`.
2. Director: detect events (3.3) from `fresh` announcements + facts transitions; advance state
   machine with `now = get_time()` (macroquad wall time); issue backend calls.
3. SFX: stingers from `fresh` (4.2) with dedup; dig/strike cues (4.5-4.7); savage timer (4.4);
   alert box sound (4.3).
4. Ambience: every 0.5 s recompute targets (5.2); every frame ramp gains.
All of it no-ops when muted (but the director keeps its timers so unmuting resumes sensibly).

Catalog build: on a background `std::thread` at app start (walk ~1 750 files + 2 raw files:
< 50 ms), result via channel; until ready, audio is silent. Rebuild when the DF folder setting
changes or a world with different active mods loads.

Dependency (`crates/rf-app/Cargo.toml`, `// [audio14]` comments allowed in TOML as `# [audio14]`):

```toml
[features]
default = ["audio"]
audio = ["dep:rodio"]

[dependencies]
rodio = { version = "0.20", default-features = false, features = ["vorbis"], optional = true } # [audio14]
```

`vorbis` uses `lewton` (pure Rust Vorbis) + `ogg`; output via `cpal` (WASAPI on Windows; ALSA on
Linux needs `libasound2-dev` at build time - the `audio` feature can be disabled with
`--no-default-features` for headless Linux builds). Avoid symphonia features (MPL-2.0). Licenses
to list in THIRD-PARTY-NOTICES.md section 4 (verify with `cargo metadata` after adding):
rodio MIT OR Apache-2.0; cpal Apache-2.0; lewton MIT OR Apache-2.0; ogg BSD-3-Clause;
dasp_sample MIT OR Apache-2.0; plus whatever `cpal` pulls on Windows (`windows`/`windows-core`
MIT OR Apache-2.0). Section 4 sentence to add: the binary also links these crates, and DF's sound
files are read from the user's install at runtime, never shipped. Note: crates are not in the local
cargo cache yet; the first build needs network.

Also add to CONTRIBUTING/INSTALL (short): `RUSTFORT_MUTE=1` / `--mute`; tests and screenshot
runs are always silent.

---

## 9. Gaps summary (FIDELITY rows for the implementer's `## [audio14]` section)

| Item | Class | Notes |
|---|---|---|
| MUSIC raws: FILE, CARD, EVENT, CONTEXT, FREQUENCY | OK | w:Audio; parsed at runtime incl. active mods |
| Contexts ANY/MAIN/seasons | OK | w:Audio |
| FIRST_YEAR / SECOND_YEAR_PLUS | OK rule, ESTIMATE data | needs founding date in export (3.2.1) |
| CAVERNS_OPENED = any cavern discovered | ESTIMATE | |
| Event triggers SIEGE, FORGOTTEN_BEAST_ATTACK, MEGABEAST_ATTACK | OK | from announcements |
| JUST_EMBARKED, FIRST_CAVERN_OPENED, FIRST_GHOST, FORT_LEVEL, LOST_FORT | ESTIMATE | transition detection in the app |
| DEATH_SPIRAL 5 deaths / 2 months, 1 season cooldown | ESTIMATE | |
| TAVERN_MUSIC_PRESENT audience ≥ 5 | ESTIMATE; TAVERN_DANCE MISSING | sim has no dance |
| FREQUENCY weights 10/5/2 | OK | w:Audio |
| Song gap avg ±25 % | OK at 240, ESTIMATE elsewhere | init comment |
| Cards before songs (all in order first time, then 1-3 random) | ESTIMATE | notes 50.0x + w:Audio verify |
| Neutral cards as interludes p = 1/4 | ESTIMATE | |
| Volumes 0..255 linear, starting-volume song 0.85 | OK values, ESTIMATE curve | init, starting-volume.txt |
| SOUND raws via announcements | OK where the sim announces | |
| STRUCK_DEEP_METAL, FEATURE_DISCOVERY from app detection | ESTIMATE | sim does not announce |
| DEMON_ATTACK, WEDDING, supported-thief AMBUSH | MISSING | no sim source |
| SAVAGE_AREA howls | ESTIMATE, dormant | climate MISSING in exports |
| Ambience layers and all their thresholds | ESTIMATE | w:Audio prose, notes, FMOD parameter names |
| 53.17 FMOD-only sounds (mining tile types, UI buttons, breakthroughs) | MISSING | not in loose files; substitutes from `audio/` are ESTIMATE |
| Extra adventure SFX (combat blows) | Rustfort extension, default off | |

---

## 10. Tests (no playback, no DF files in the repo)

Unit tests in the audio modules (`#[cfg(test)]`, run under `cargo test --release --workspace`):

1. **Raw parsing** (`catalog.rs`): inline raw strings (written in the test, not copied files)
   with 3 MUSIC objects (cards, events, contexts, FREQUENCY:UNCOMMON/RARE) and 2 SOUND objects
   (ANNOUNCEMENT list, SAVAGE_AREA); check fields. Include an unbracketed `SOUND:X` line that must
   be ignored, and trailing comment text after tokens.
2. **Path resolution** against a temp dir built in the test (`std::env::temp_dir()/rf-audio14-<pid>`)
   with empty placeholder files named like the DF layout (`tracks/drink_&_industry/DI_Full.ogg`,
   `tracks/strike_the_earth!/STE_3.ogg`, `tracks/adv/3_Bruising The Fat (Shar Ikal).ogg`,
   `sounds/Giant_Step_1.ogg`): ids resolve as in 1.1; unknown id -> None. Mod
   `sound_file_*.txt` identifier overrides.
3. **init.txt import**: data/init then prefs override; SOUND:NO -> muted; clamp avg 10..600.
   `starting-volume.txt` / `spatialization.txt` parsing incl. missing file defaults.
4. **Context selection**: given facts (month 10 = winter, age < 1 year, caverns) the candidate set
   equals the expected ids; title screen = ANY only; weights 10/5/2 -> with a seeded RNG over
   100 000 picks the UNCOMMON/normal ratio is 0.5 ± 0.02 and RARE 0.2 ± 0.02.
5. **No immediate repeat**; deck: first pick plays all cards in order; later picks 1-3 distinct
   cards; the deck reshuffles after exhaustion (every card heard once per cycle).
6. **Events**: feed synthetic announcement texts ("A vile force of darkness has arrived!",
   "The Forgotten Beast X has come! ...") and fact transitions (caverns 0 -> 1, ghosts 0 -> 1,
   citizens n -> 0) to the director with a `NullBackend`; assert the started MUSIC id is among
   the event's songs, priority overrides, same-event ignore, and that a first snapshot after
   "load" with caverns already discovered fires nothing.
7. **Death spiral window/cooldown** with tick-stamped deaths.
8. **Gap**: 10 000 samples of `gap(240)` lie in [180, 300] and the mean is 240 ± 2.
9. **SFX mapping**: each Rustfort class maps to the expected SOUND ids with vanilla-like inline
   raws; dedup within 2 s; lone THIEF -> no sound.
10. **Ambience targets** (`ambience.rs` pure fn `targets(&AmbInputs) -> [f32; N]`): surface
    forest -> Forest 1; depth 2 -> 0.5; cavern -> Cavern; magma distance 2/7/inf; river counts.
11. **Positional gain**: dz 0/1/2/3 and distance 0/5/10/11.
12. **Decode smoke test** (only when `RUSTFORT_DF_DIR`/`rf_raws::df_dir()` has `data/sound`, else
    skip with a message): decode the first 4 096 samples of `sounds/alert.ogg` with
    `rodio::Decoder` (no output device) -> sample rate 48 000, 2 channels. Behind
    `#[cfg(feature = "audio")]`.
13. **Mute policy**: `should_open_device(args, env, settings)` truth table (screenshot, script,
    bench, RUSTFORT_MUTE=1, --mute, muted setting, master 0) -> false in all those cases.
14. rf-sim: `cargo test -p rf-sim --release --test saves11` still passes (skip field);
    a test that `drain_audio_cues` is empty when cues are off and receives a Dig cue after
    `dig_tile_for_test` when on; `state_hash` equal with cues on vs off over 2 000 ticks of the
    synthetic fort.

Manual check (by the implementer, muted policy respected): `rustfort play --screenshot
target/audio14.png --frames 30 --menu --menu-page audio` shows the Audio page and must not open
a device (log line "audio: off (screenshot)").

---

## 11. Performance notes

- Sim: one `bool` branch in `dig_tile`, `perform`, `strike` when cues are off. Bench must stay
  within ~5 % (expect identical).
- App: per frame O(#fresh announcements + #cues); ambience scan at 2 Hz over ≤ 31x31x3 magma box
  and a strided view rect (≤ ~5 000 tile reads); no per-frame allocation (reuse `Vec`s in
  `AudioSystem`).
- Decoding happens on rodio's mixer thread (streaming); the 117 MB of `tracks/` is never loaded
  whole. SFX cache ≤ 32 buffered sounds (~20 MB worst case, typically < 5 MB).

---

## 12. Prioritized implementation list

1. `audio/catalog.rs`: scan + raws + id resolution + init/starting-volume parsing, with tests 1-3.
2. `audio/out.rs` + mute/device policy (`--mute`, `RUSTFORT_MUTE`, headless never opens), test 13;
   rodio dependency + THIRD-PARTY-NOTICES section 4.
3. `audio/director.rs`: title music, context pools, weights, gaps, cards, tests 4-5, 8.
4. Settings: `AudioSettings`, DF import, `ui/audio14.rs` Audio page.
5. Events from announcements (SIEGE, FB/megabeast) + SOUND stingers (`sfx.rs`) + alert ping,
   tests 6, 9.
6. `rf-sim/src/audio14_api.rs`: `AudioFacts`, cue buffer, hooks in `dig_tile`/`perform`/`strike`;
   transition events (FIRST_CAVERN_OPENED + CAVERN_BREAK, FIRST_GHOST, LOST_FORT, DEATH_SPIRAL,
   TAVERN_MUSIC_PRESENT, FORT_LEVEL, JUST_EMBARKED); test 7, 14; bench/hash/saveload checks.
7. Ambience mixer (`ambience.rs`) with legacy `ambience/` beds, ducking, resume-on-pause; test 10.
8. Mining SFX positional (`dig.ogg`), test 11; adamantine stinger.
9. Fort age: rf-core `Meta::founded_*` + exporter field + EXPORT_FORMAT.md.
10. Optional: UI clicks, savage howls (dormant until climate export), walla, adventure-folder
    fallback ambience and one-shots, extra combat SFX.
11. `docs/FIDELITY.md` `## [audio14]` section from section 9.
