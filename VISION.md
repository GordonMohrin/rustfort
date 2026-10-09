# Rustfort: Makro-Vision (Entwurf zur Freigabe)

Arbeitstitel „Rustfort“: ein eigener Nachbau des DF-Festungsmodus in Rust. Er bleibt auch bei 200+ Zwergen schnell, verhält sich wie das Original und spielt deinen echten Save weiter.
Stand 09.10.2026. Es gibt noch keine Zeile Code; dieser Plan muss erst freigegeben werden.

## 1. Nordstern: drei messbare Ziele

| Ziel | Messung | Heute in echtem DF |
|---|---|---|
| **Schnell** | Dein importiertes Fort mit 200 Zwergen und 500 weiteren Einheiten läuft headless mit **mindestens 1000 Ticks/s**, mit Grafik mit mindestens 500 (ein Spielmonat etwa 35–70 s). | Razordrums: 229 aktive Einheiten, 12 889 Items, **etwa 220 Ticks/s**. Der Hauptthread ist am Limit, der Prozess nutzt nur etwa 1,4 Kerne deines 5600X (`dwarf-fortress/SCHNELLER-run3.md`). |
| **Originalgetreu** | Lädt deine Raws aus `E:\…\Dwarf Fortress\data\vanilla` 1:1: 981 Kreaturen, 265 Inorganics, 225 Pflanzen, 163 Reaktionen. Zeitbasis, Bedürfnis-Timer und Gangarten stimmen exakt. | – |
| **Kontinuität** | Dein aktueller Save wird importiert und läuft weiter. Ein Import-Bericht zeigt, was übernommen, neu erzeugt oder verworfen wurde. | – |

## 2. Warum DF langsam wird, und unsere Antwort

Kernbefund: Das Problem sind **Einheiten insgesamt**, nicht die Zwerge. Razordrums hatte 5 Bürger, aber 229 aktive Einheiten. Davon waren 98 Dämonen in der Hölle (z0–6) und 24 Tiere in der Kaverne, die alle in jedem Tick mitgerechnet wurden.

| Ursache (Wiki / Profiling) | Anteil | Antwort in Rustfort | Schätzung |
|---|---|---|---|
| Einheiten-Züge | über 60 % der Rechenzeit | Eine Einheit rechnet nur, wenn sie etwas zu tun hat („Think-Wheel“). Teure Prüfungen werden gestaffelt. Die Entscheidungsphase läuft parallel. Unerreichbare oder schlafende Kreaturen (Hölle, Kaverne) werden ruhend gestellt. | 5–20x |
| Sichtlinien Einheit↔Einheit | O(n²) im 26-Kachel-Radius, „slowest part by a wide margin“ | Spatial Hash, Neuberechnung nur bei Bewegung, parallel | 10–50x bei Menschenmengen |
| Temperatur und Items | Temperatur aus ergibt etwa +100 % FPS | Temperatur wird nur bei aktiven Wärmequellen ausgewertet (lazy). Items liegen in Indizes nach Kategorie, Material und Ort statt in langen Listen. | etwa 10x |
| Wegfindung | unter 10 % der Einheitenzeit, außer bei eingeschlossenen Einheiten | Zusammenhangskomponenten beantworten „erreichbar?“ in O(1). Dazu kommen Chunk-Portale (HPA*) und Flow-Fields für Lager und Speisesaal. | 5–30x |
| Flüssigkeiten | im v50-Profiling genannt | Ruhende Chunks schlafen; nur aktive Zellen werden gerechnet. | 3–10x |
| Jobsuche | jeder Zwerg durchsucht alles | zentraler Matcher mit räumlichem Index | 3–10x |
| **Nur ein Kern** | Hauptthread am Limit | alle 12 Threads des 5600X, deterministisch | Multiplikator |

Die Faktoren pro Ursache sind Schätzungen. Sie werden ab Meilenstein M0 gemessen.

## 3. Prinzipien

1. **Clean-room.** Als Spezifikation dienen das Wiki und beobachtetes Verhalten; DF wird nicht dekompiliert. Raws und Grafiken werden zur Laufzeit aus deiner Installation gelesen (wie OpenMW mit den Morrowind-Daten) und kommen nie ins Repo. Das Repo bleibt privat.
2. **Datenmodell DF-kompatibel**, wegen des Save-Imports. Raw-IDs, Tiletypes, Material-Indizes, Einheiten-Felder (Attribute, Skills, Persönlichkeit, Bedürfnisse, Wunden) und die IDs historischer Figuren entsprechen DF. Das Verhalten darf schneller implementiert sein, die Daten müssen passen.
3. **Deterministisch.** Gleicher Seed und gleiche Befehle ergeben das gleiche Ergebnis, auch bei paralleler Ausführung. Das ermöglicht Replays, Hash-Checks und reproduzierbare Bugs.
4. **Headless zuerst, mit Agent-API.** Die Simulation ist eine Bibliothek ohne Bildschirm; Darstellung und API docken an. Die API läuft nur auf localhost (keine externen Dienste) und nutzt das dfpilot-Vokabular, sodass Claude-Agenten mit 1000+ Ticks/s mitspielen.
5. **Erst messen, dann bewerten.** Es gibt von Tag 1 an ein Benchmark-Harness mit der Kurve Ticks/s gegen Population. Dein echter Save ist der Referenz-Benchmark.

## 4. Treue-Stufen

| Stufe | Bedeutung | Inhalt |
|---|---|---|
| **A: exakt** | wie DF, Zahl für Zahl | Zeitbasis (1200 Ticks/Tag, 403 200/Jahr); Hunger, Durst und Schlaf; Gangarten (900 = 9 Ticks/Kachel); Raws (Materialien, Kreaturen, Körper, Items); Designations und Kachel-Semantik; Labors, Skills und XP; Lager und Hauling; Werkstatt-Ausgaben; Qualitätsstufen; Flüssigkeitsstufen 0–7 |
| **B: gleiches Verhalten** | Statistik stimmt, nicht jeder Tick | Gedanken, Stress und Bedürfnisse; Kampf über Gewebeschichten; Migranten- und Karawanen-Rhythmus; Handelswerte; Stimmungen und Artefakte; Belagerungen; Forgotten Beasts; Temperatur; Fluss-Algorithmus |
| **C: vereinfacht** | spielt sich ähnlich | Politik der Adligen, Justiz, Religion, Auftritte, Wetter, Wildtier-Wanderungen, Details im Hospital |
| **D: nicht geplant** | – | Adventure-Modus, volle Weltgeschichte und Legends, Nekromantie-Ketten, Modding-Oberfläche |

Wichtiger Befund: Fast alle Werkstätten und Öfen (Schreiner, Steinmetz, Schmelze, Brennerei, Küche) sind in DF **fest einprogrammiert und stehen nicht in den Raws**. Die Raws enthalten nur 2 Werkstätten. Diese Werkstätten und Jobs bauen wir nach dem Wiki nach.

## 5. Save-Import

**Weg:** Ein DFHack-Exporter liest den laufenden DF-Speicher und schreibt ein Export-Paket. `rf-import` lädt dieses Paket in Rustfort. Die `.sav`-Datei direkt zu lesen ist **nicht realistisch**: Sie ist ein undokumentierter Dump der internen Strukturen (autosave 3: 22 MB gepackt, 233 MB entpackt, 11 672 zlib-Chunks), und das Format ändert sich mit jeder DF-Version.

**Deine Saves** liegen in `%APPDATA%\Bay 12 Games\Dwarf Fortress\save\`:

| Ordner | Fort | Welt | Größe (entpackt) | Stand |
|---|---|---|---|---|
| autosave 3 | **Trussroads** (Run 6) | Zilirróth | 233 MB | heute 22:45 |
| region3 | **Inchcraft** (dein eigenes) | Angsturstrasp | 335 MB | 06.10. |

Die beiden Forts liegen in verschiedenen Welten. Das sind also zwei getrennte Migrationen.

**Ablauf eines Exports:**
1. DF pausieren und den Tick merken. Ändert sich der Tick während des Exports, bricht der Export ab.
2. Die Karte in Häppchen exportieren, etwa 300 Blöcke pro Frame. Die Lehre aus BUG-415: Ein Lua-Scan der ganzen Karte fror DF 9–28 s ein. Alternativ zieht ein Rust-Client die Karte über RemoteFortressReader (localhost:5000). Für eine 4x4-Karte dauert das etwa 1–3 Minuten ohne langes Einfrieren.
3. Die Weltgeschichte (`exportlegends`) wird **nur aus einer Kopie** des Saves gelesen, nie aus dem laufenden Spiel, weil sonst der Save beschädigt werden kann.
4. Ergebnis ist ein Paket aus `manifest.json` (Versionen, Tick, Zählungen, sha256) und zstd-Dateien: Raws des Saves, Karte pro z-Ebene, Einheiten, historische Figuren, Items, Gebäude, Zonen, Lager, Trupps, Pflanzen, Artefakte, Gravuren. Geschätzt 30–80 MB gepackt.

**Die Raws kommen aus dem Save selbst**, nicht nur aus `vanilla`. Dein Save enthält generierte Materialien: göttliche Metalle, Forgotten-Beast-Stoffe, Syndrome. Die Material-Indizes werden exakt wie in DF übernommen.

| Kategorie | Inhalt |
|---|---|
| **Muss übernommen werden** | Alle DF-IDs und Next-ID-Zähler; Karte mit Materialien, Adern, Flüssigkeiten, Konstruktionen und Entdeckt-Status; Einheiten mit Körper, Wunden, Skills, Attributen, Persönlichkeit, Bedürfnissen, Stress, Erinnerungen und Beziehungen; Items mit Containern, Verzierungen und Artefakten; Gebäude, Räume und Besitz; Zonen; Lager-Einstellungen; Burrows; Adlige, Trupps, Uniformen; Kalender; die Weltgeschichte in schlanker Form (Figuren mit Bezug zum Fort, Zivilisation) |
| **Wird neu erzeugt** | Laufende Jobs, Pfade, Erreichbarkeitsdaten, Temperatur, Caches, kurzfristige Emotionen |
| **Wird verworfen** | Tiefe Weltgeschichte, Meldungen, Kampf-Logs, Grafik-Zustand |

**Prüfung des Imports:**
1. Zählungen vergleichen: Einheiten nach Rasse, Items nach Typ und Material, Kacheln nach Material pro z-Ebene.
2. Block-Hashes vergleichen, die Lua-Exporter und Rust-Importer identisch berechnen.
3. Prüfen, dass jede Referenz auflösbar ist.
4. Stichproben: 50 Kacheln und 20 Zwerge Feld für Feld vergleichen.

**Nur in eine Richtung:** DF → Rustfort. Ein Rückweg nach DF ist nicht realistisch, weil ein einziger Fehler den Save still beschädigt. Dein letzter DF-Save bleibt als eingefrorenes Archiv bestehen.

**Wiederverwendbar:**
- `dwarf-fortress/lua/claude/kartendump.lua` (620 Zeilen, Karten-JSON) als Gerüst. Er ist aber verlustbehaftet, weil er Materialklassen statt Material-Indizes schreibt.
- Die Frame-Verteilung aus `schau.lua` und `wachen.lua`.

## 6. Architektur

```
 DF 53.16 + DFHack ──export_fort.lua──▶ fort-export (NDJSON+zstd) ──▶ rf-import
 E:\…\data\vanilla ─────────────────────────────────────────────────▶ rf-raws
                                                                        │
          ┌──────────── rf-sim (deterministischer Tick, bevy_ecs + rayon) ◀┘
          │  perceive ∥ → decide ∥ → resolve (seriell, nach ID) → commit
          │  rf-world (16³-Chunks, Fluide, Temperatur)  rf-path (Komponenten, HPA*, Flow-Fields)
          │  rf-items (Slotmap + räumliche Indizes)     rf-jobs (Matcher, Labors, Werkstätten)
          ▼
   Snapshot (Triple-Buffer) ──▶ rf-render-gpu (Tileset/CP437, 60 fps)
                            ──▶ rf-render-tui (ASCII im Terminal, Debug)
                            ──▶ rf-api (localhost HTTP/JSON, Agenten)
   rf-save (serde+postcard+zstd, versioniert)    rf-bench (Szenarien, Ticks/s-Kurven)
```

- **Kacheln, Items und Jobs** liegen in eigenen Struct-of-Arrays-Speichern. In der ECS stehen nur Einheiten.
- **Zufall** ist pro Einheit und Tick geseedet, damit die Thread-Reihenfolge keine Rolle spielt.
- **Rendering blockiert die Simulation nie**, weil es nur Snapshots liest.

## 7. Meilensteine (jeder mit einem Prüfpunkt für dich)

| # | Inhalt | Prüfpunkt für dich |
|---|---|---|
| **M0** Fundament | Workspace, Raw-Parser (Templates, Creature-Variations, COPY_TAGS_FROM, Castes), Benchmark-Harness, Stresstest mit 200/400/800 Einheiten auf einer 4x4-Karte | `rustfort raws --stats` zeigt 981/265/225/163. Der Stresstest zeigt die Ticks/s-Kurve. |
| **M1** Deine Karte | DFHack-Exporter (Karte, Gebäude, Lager) und Viewer (GPU + ASCII) | Du scrollst durch **deine eigene Festung** in Rustfort, und die Kachelzählung pro Material stimmt mit DF überein. |
| **M2** Leben | Einheiten importieren; Bewegung, Wegfindung, Sichtlinien; essen, trinken, schlafen | Dein Fort läuft 1 Spieljahr headless ohne Massensterben bei mindestens 1000 Ticks/s. Differenztest: Die Bedürfnis-Kurven stimmen mit echtem DF überein. |
| **M3** Arbeit | Designations, Labors, Hauling, Lager, die Kern-Werkstätten, Landwirtschaft, Brauen, Kochen, Arbeitsaufträge | **Ab hier spielbar:** Du gräbst, baust und produzierst, und das Fort trägt sich selbst. |
| **M4** Gesellschaft | Gedanken, Stress, Persönlichkeit, Migranten, Adlige und Mandate, Bestattung und Geister, Karawanen und Handel | Stimmung und Stress deiner Zwerge entwickeln sich plausibel; die erste Karawane kommt. |
| **M5** Kampf | Körper- und Gewebekampf aus den Raws, Wunden, Medizin, Trupps, Belagerungen, Forgotten Beasts | Eine Goblin-Belagerung mit 50 Angreifern läuft bei voller Geschwindigkeit. |
| **M6** Physik | Flüssigkeiten, Temperatur, Magma, Kavernen, Aquifer | Stresstest geflutete Kaverne und Magma-Schmiede |
| **M7** Feinschliff | Stimmungen und Artefakte; Premium-Tileset aus deiner Installation; UI-Parität | „Fühlt sich an wie DF“: Das entscheidest du. |

**Fortschrittsmaß über alle Meilensteine: die Import-Abdeckung.** Sie gibt an, welcher Anteil der Einheiten, Items, Gebäude und Jobs deines Saves tatsächlich simuliert wird. Der Wert ist eine Zahl, die bei jedem Meilenstein steigt.

**Ehrliche Einschätzung:**
- M0–M2 dauern Tage bis wenige Wochen mit Agenten.
- M3–M5 sind Monate.
- 100 % Parität erreichen wir nie, weil DF 20 Jahre Entwicklung enthält.
- Mit Lücken spielbar wird dein Save ab M3. Wie DF fühlt er sich ab M5 an.
- Den Export kannst du jederzeit wiederholen. Du spielst in DF also weiter, bis Rustfort bereit ist, und importierst dann den neuesten Stand.

## 8. Risiken

1. **Die Breite ist schwieriger als die Geschwindigkeit.** Das größte Risiko sind Bedürfnisse, Stimmungen, Kampf und Materialien.
2. **Import-Lücken.** Was Rustfort noch nicht kennt, geht beim Import verloren. Der Import-Bericht macht das sichtbar.
3. **DF-Updates ändern die Datenstrukturen.** Deshalb ist der Exporter auf DF 53.16 und DFHack 53.16-r2 festgelegt.
4. **Determinismus und Parallelität** verlangen Disziplin ab Tag 1. Replay-Hash-Tests laufen in CI.
5. **Die gestaffelte Ausführung verschiebt Timings minimal** gegenüber DF. Das wird dokumentiert und per Differenztest begrenzt.
6. **Rechtliches:** nur privat; keine DF-Dateien im Repo; eigener Name.

## 9. Sofort-Hilfe im echten DF (parallel zum Projekt)

- In den Einstellungen unter Spiel die **experimentelle Multithreading-Option** einschalten. Sie parallelisiert die Sichtlinien.
- **Eigene Lua-Dauerjobs abspecken:** Sie kosteten 28–39 % der Echtzeit, und ein Karten-Scan verursachte 9–11 s Stalls.
- **gfps auf 25–30** senken und `timestream` nutzen; das ist gemessen etwa x2.
- **Einheiten und Items reduzieren:** Tiere schlachten, Kavernen abriegeln, `clean` ausführen. Optional Temperatur aus (etwa +100 %, dann aber ohne Magma- und Feuer-Mechanik).

## 10. Deine Entscheidungen

1. **Welcher Save zuerst?** Trussroads (Run 6, aktuell) oder Inchcraft (dein eigenes)? Ich empfehle das Fort, das du wirklich weiterspielen willst. Die Engine ist für beide dieselbe, nur der Export läuft zweimal.
2. **Treue-Stufen A–D:** Passt der Schnitt? Vor allem: Adventure-Modus und volle Weltgeschichte bleiben draußen.
3. **Darstellung:** Zuerst ein GPU-Fenster mit CP437-Schrift und ein ASCII-Debug-Terminal, das Premium-Tileset aus deiner Installation kommt in M7. Oder soll das Tileset schon früher kommen?
4. **Agent-API als Kernfeature:** ja, damit Claude-Agenten headless mitspielen und testen.
5. **Repo:** privat als `GordonMohrin/rustfort`, Commits als du mit Claude als Co-Autor, immer pushen.
6. **Start:** nach Freigabe M0 und M1 parallel mit Agenten (Raw-Parser, Benchmark-Harness, Exporter). Der erste Prüfpunkt für dich ist „deine Festung im Rustfort-Viewer“.
