# Rustfort fidelity audit (round 6)

The DF v50 wiki and observed behavior are the spec. Each row is classified:

- **OK**: matches the wiki.
- **WRONG**: contradicts the wiki.
- **MISSING**: the wiki describes it, but Rustfort does not do it.
- **ESTIMATE**: the wiki gives no number, so Rustfort uses its own value (listed in the module docs).
- **FIXED r6**: was WRONG or MISSING and was fixed in round 6, with a test.

## Economy, production & society

(This half is written by the economy audit and merged in.)

## Combat, defense, health & moods

Modules: `combat.rs`, `military.rs`, `invasion.rs`, `ranged.rs`, `defense.rs`, `health.rs`, `moods.rs`, `caverns.rs` and the new `syndrome.rs`.

Wiki pages checked: Combat, Material science, Attack, Armor, Wound, Blood, Pain, Dodging, Military, Squad, Schedule, Siege, Ambush, Invasion, Crossbow, Ammunition, Trap, Cage trap, Weapon trap, Door, Bridge, Lever, Burrow, Health care, Hospital, Infection, Strange mood, Artifact, Insanity, Cavern, Forgotten beast and Cave-in.

### Round 6 fixes

| Fix | Wiki | Before | After | Evidence |
|---|---|---|---|---|
| Soldiers see lurking hostiles | Military: on-duty soldiers "will attack any hostiles they encounter" | A dormant hostile (no prey in its own sight) was skipped by the threat scan. Soldiers within 20 tiles ignored it until it woke up by itself 400-500 ticks later, and never at all if their z differed by 3-4. | Soldiers on duty spot it within 100 ticks; civilians still react only to active hostiles. | `combat::tests::soldiers_spot_a_lurking_hostile` fails without the fix (target stays `NONE`) and passes with it. Counter `CombatStats::lurkers_engaged`. |
| Wounded soldiers rest | Health care: injured dwarves "go (or are brought) to recuperate" | `combat_decide` always made a soldier on duty a `Soldier`, so soldiers with broken bones trained or stood guard until they healed (bones heal only at rest, so never). | With no able hostile within 3 tiles, a wounded soldier becomes a patient (hospital bed first, care by doctors) and returns to duty when healed. The threat scan no longer pulls a resting soldier out of bed. | `combat::tests::wounded_soldier_rests_instead_of_training`. On the real fort over 2 years, `soldiers_rested` = 2. |
| Weapon traps jam | Weapon trap: "a 50% chance that a kill will result in the victim getting stuck in the mechanism and cause the trap to jam"; "a mechanic will automatically attempt to clean it" | Traps never jammed. | 50 % of kills by the trap itself jam it: it does nothing until a mechanic does a **Clean Trap** job (no job while enemies are within 20 tiles). The state is kept as a `JAMMED` building flag, so it survives a rebuild or a save. | `defense::tests::weapon_trap_jams_on_kills_and_a_mechanic_cleans_it`. On the real fort over 2 years, `traps_jammed` = 1. |
| Forgotten beast special attacks | Forgotten beast: poisonous bites and stings, deadly dust and toxic gas, webs; syndromes "from mild pain to complete and instantaneous necrosis or paralysis" | Only claws and teeth. | `syndrome.rs` rolls one special attack per beast: none 30 %, venom 35 %, breath 25 %, webs 10 %.<br>Effects: paralysis, unconsciousness, bleeding, pain, necrosis, and respiratory failure (venom only).<br>The arrival text ends with DF's "Beware its ...!". | 4 tests in `syndrome::tests` (breath paralyzes at 6 tiles, a venom bite takes hold only on a wound, respiratory failure kills, the odds). On the real fort 2 of 5 beasts had stings. |

### Combat (wounds, materials, defense rolls)

| Item | Class | Notes |
|---|---|---|
| Attack momentum (Material science formula 3), weapon vs body-part attacks | OK | Unit test against the wiki's battle axe example. |
| Edged cut / blunt fracture thresholds over armor and tissue layers, quality factors | OK | Wiki formulas. Some minimum contact areas are estimates (A >= 1 blunt, A >= 2 edged on armor). |
| "Copper cannot cut iron": no cut when the armor's shear yield is at least 2x the weapon's | ESTIMATE | Wiki lore only, no formula. |
| Contact area = volume^(2/3) x 50 | ESTIMATE | |
| Hit, dodge, block and parry chances; attack intervals; momentum roll | ESTIMATE | DF uses opposed skill and attribute rolls; the wiki gives no numbers. |
| Dodging moves the defender to an adjacent tile | MISSING | Rustfort's dodge leaves the defender in place. |
| Wrestling moves (grab, joint lock, break, throw, choke) | MISSING | Unarmed attacks are body-part strikes only. |
| Stunned, winded, nauseous and dizzy states | MISSING | Combat page status effects. |
| Fatigue and exhaustion in long fights | MISSING | |
| Prone and crawling with broken legs | MISSING | Units with a broken stance limb still walk; see Recover wounded below. |
| Fainting below 50 % blood, death at 0 blood, brain/heart/head death | OK | Wiki Blood / Wound. |
| Bleeding volumes, clotting, pain threshold 250 + WILLPOWER/10, healing rates | ESTIMATE | |
| Severing (all layers cut and the contact area is at least half the part) | ESTIMATE | |
| Fire, burns, temperature | MISSING | The save has temperature off. Dragonfire and fire breath are not modelled. |

### Military

| Item | Class | Notes |
|---|---|---|
| Duty from orders and schedule (off duty, staggered, constant, Ready) | OK | Staggered is split by position parity per month (ESTIMATE). |
| Kill orders: pursue the unit wherever it is reachable; done when it dies | OK | If the target is unreachable the soldier waits (DF soldiers stand idle the same way). |
| Soldiers engage hostiles in sight (20 tiles, 2 z-levels) | ESTIMATE + FIXED r6 | The range is an estimate; dormant hostiles are now included. |
| Patrol routes | MISSING | Treated as Train. |
| Training by sparring and demonstrations (sparring injuries, teachers) | MISSING | Training is flat experience every 100 ticks (ESTIMATE). |
| Barracks: sleeping, training spot, equipment storage | MISSING | |
| Squad ammunition settings (ammo per squad, combat/training flags) | MISSING | Not exported. Ammo is chosen by launcher class: bolts for crossbows, metal ammo first for combat. |
| Uniforms: pick up missing pieces, `KIND:SUBTYPE@MATERIAL` entries | OK | "Replace clothing", "exact matches" and partial matches are not distinguished. |
| Wounded soldiers rest | FIXED r6 | |
| Soldiers leave a fight at the job-cancel thirst/hunger thresholds | ESTIMATE | Wiki: fights are "do-or-die". The rule exists so an unkillable foe cannot starve the whole squad. |
| **Civilian alert** | WRONG | Wiki Burrow: the civilian alert is "missing" in v50.07+; citizens assigned to a burrow "will only accept jobs from within their assigned burrow(s)" and still work. Rustfort's alert makes civilians mill about in the burrow with no work at all.<br>Not fixed: it needs burrow filtering in the job, haul and production searches (economy modules). |
| Civilians flee hostiles within 12 tiles; cornered civilians fight back | ESTIMATE | |

### Invasions

| Item | Class | Notes |
|---|---|---|
| Sieges from a population of 80 | OK | Wiki Siege: "80 members". |
| Checked at season changes, chance 1/3 | ESTIMATE | |
| Siege size | WRONG | Wiki: set by fort wealth (from 10 regulars at Town to 120 regulars + 40 monsters at Metropolis). Rustfort: 10 + population/8, at most 60. |
| Sieges vs ambushes | MISSING | Wiki: 25 % of invasions are sieges on normal difficulty (50 % on hard); the rest are smaller ambushes. Rustfort only has sieges. |
| Thieves and snatchers (kobold thieves, goblin snatchers) | MISSING | |
| Goblins charge at once; humans sometimes camp at the map edge | OK for goblins / MISSING camps | Rustfort sieges are goblin sieges, which charge as on the wiki. |
| Monsters and mounts (trolls, beak dogs) in sieges | MISSING | 25 % of a siege are bowmen (ESTIMATE). |
| Retreat when "sufficiently" beaten or waited out | ESTIMATE | Half dead or 2 months. Leader death has no effect. |
| Invader equipment shares and skills | ESTIMATE | |

### Ranged combat

| Item | Class | Notes |
|---|---|---|
| Launcher + matching ammo, quivers, fire range about 20 tiles, misses fly on | OK | Wiki Marksdwarf. |
| Projectile momentum and armor resolution | OK | Wiki Material science. |
| Fortifications: below Accomplished, fire only from the adjacent tile | OK | Wiki Fortification. |
| Hit, dodge and block chances; rate of fire; ammo breakage | ESTIMATE | |
| Friendly fire (units standing in the line of fire) | MISSING | |
| Squad ammo settings | MISSING | See Military. |

### Defense (doors, bridges, levers, traps)

| Item | Class | Notes |
|---|---|---|
| Locked or lever-closed doors block everyone; building destroyers smash them | OK | |
| Pet-passable doors: pets path through | MISSING | Pets only wander through. |
| Bridges: raised wall edge, flinging, atom smasher, retracting | OK | |
| Mechanism delay 100 ticks; pulling a lever 10 ticks | ESTIMATE | |
| Linking needs mechanisms and a mechanic job | MISSING | Linking is instant. |
| Traps: hostiles and wild animals trigger them; citizens, pets, merchants and `TRAPAVOID` creatures do not | OK | |
| Unconscious creatures trigger traps, including your own dwarves | MISSING | Wiki Weapon trap. |
| Cage trap: the creature is caught; a mechanic re-arms it with a free cage | OK | |
| Occupied cages hauled to an animal stockpile (standing order) | MISSING | Wiki Cage trap. The cage stays on the trap tile; hauling belongs to the storage module. |
| Weapon trap: all weapons strike at once (trap components `HITS` times) | OK | |
| Weapon trap jamming and Clean Trap | FIXED r6 | |
| Weapon trap reset delay ("after an unknown period") | ESTIMATE | Rustfort: no delay. |
| Stone-fall trap: single use, re-armed with a stone | OK | |

### Health

| Item | Class | Notes |
|---|---|---|
| Diagnosis first, then suturing, bone setting, surgery and dressing with supplies | OK | Durations are estimates. |
| Hospital beds first, then the patient's own bed, then any free bed | OK | Wiki Health care. |
| **Recover wounded**: immobile or injured dwarves are carried to a hospital zone or the nearest free bed | MISSING | Doctors treat unconscious patients where they fell. Patients with a broken leg walk to bed themselves (WRONG). Next item to do. |
| Feeding bedridden patients (water, food) | OK | |
| Traction benches, casts, crutches | MISSING | |
| Hospital chests stocked with thread, cloth and splints | MISSING | Supplies are taken from anywhere. |
| Infections from uncleaned wounds; soap | ESTIMATE | Rates. |

### Strange moods and insanity

| Item | Class | Notes |
|---|---|---|
| Counter, 20 eligible dwarves, artifact cap `min(items/100.1, revealed tiles/2304)` | OK | |
| Profession lottery, workshop by highest moodable skill, materials, 50 000-tick insanity timer | OK | |
| Fell and macabre moods only for highly stressed dwarves | OK | The odds are estimates. |
| Artifact value, names | ESTIMATE | Names come from random syllables, not DF's language files. |
| Insanity kinds and odds | ESTIMATE | Melancholy dwarves stop eating; they do not seek water to drown in. |

### Caverns and forgotten beasts

| Item | Class | Notes |
|---|---|---|
| Cavern layers, discovery, breach, magma sea | ESTIMATE | Flood-fill detection; DF stores the layers directly. |
| Cave wildlife from the save's subterranean creature raws | OK | Group sizes and rates are estimates. |
| Forgotten beast arrival rate | ESTIMATE / likely too high | Rustfort: 1/4 per season, one at a time, once a cavern is discovered (5 in 2 years on the real fort). Wiki: "exactly one beast is spawned per each underground region", attracted by wealth. |
| Beasts never leave; `TRAPAVOID`; building destroyers | OK | |
| Venom, breath, webs and syndromes | FIXED r6 (simplified) | No resistance or size dilution, no cure; the durations are estimates. |
| Fire breath, dragonfire, secretions, contact syndromes on touch | MISSING | No temperature. |
| **Cave-ins** | MISSING | Wiki Cave-in: tiles not supported along the X, Y and Z axes fall (diagonals, bridges and ramps below give no support). Creatures directly underneath die; the dust knocks out and throws creatures nearby. Rustfort never checks support after digging, channeling or deconstruction. |

### Performance and determinism (round 6, real fort, 2 years with `--standdown`)

- **State hash:** stays the same across repeated runs (`9cfb23265fa36f6b`, twice).
- **Survival:** 88 -> 90 citizens, the same as master.
- **Speed:** about 59.6k ticks/s, against master's 61.4k measured back to back on the same busy machine (4 agents building at once). That is within the noise.
- **Combat cost:** the combat phase stays at about 0.11 us/tick.
