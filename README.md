# Rustfort

A fast re-implementation of the Dwarf Fortress fortress mode in Rust. It is meant for personal use and continues a real DF save.

- Vision and plan: [VISION.md](VISION.md)
- Architecture and rules: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Save export format: [docs/EXPORT_FORMAT.md](docs/EXPORT_FORMAT.md)
- The game window, controls and screens (title screen, import, doors and levers, trade, nobles, settings): [crates/rf-app/README.md](crates/rf-app/README.md)

The repository contains no Dwarf Fortress data. Rustfort reads raws and the Premium tileset at runtime from your own DF installation (the Steam default, the `RUSTFORT_DF_DIR` environment variable, or the folder set in Settings). Fort exports are written to `export/` and save games to `saves/`; both are gitignored.

```
rustfort.cmd                                     # build if needed, then the title screen
cargo run --release -p rf-app -- play --export export/tileshtulon   # straight into an export (no title screen)
cargo run --release -p rf-app -- bench           # headless ticks/s benchmark
```

## How to play (Spielen, für Gordon)

**Starten:** Doppelklick auf `rustfort.cmd` im Rustfort-Ordner (oder in der Konsole `rustfort.cmd`). Beim ersten Mal wird das Spiel gebaut (ein paar Minuten), danach nur noch, wenn sich am Code etwas geändert hat. Dann erscheint der Titelbildschirm mit dem DF-Hintergrund:

- **Continue** lädt deinen neuesten Spielstand (auch Autosaves). Gibt es noch keinen, startet es mit dem neuesten Export aus DF.
- **Load game** zeigt alle Spielstände (Tab *Saves*) und alle bisherigen DF-Exporte (Tab *DF exports*). Anklicken lädt.
- **Import from Dwarf Fortress** holt dein Fort direkt aus dem laufenden DF:
  1. Dwarf Fortress mit DFHack starten und das Fort laden (DF darf weiterlaufen, auch mit deinen eigenen Skripten).
  2. In Rustfort *Import from Dwarf Fortress* wählen. Rustfort fragt DFHack, welches Fort geladen ist, und zeigt es an.
  3. *Import and play* klicken. Ein Balken zeigt den Fortschritt (etwa eine halbe bis eine Minute). Der Exporter **liest nur**: DF wird nie pausiert oder verändert, es läuft währenddessen nur etwas langsamer.
  4. Danach startet das Spiel mit dem frischen Stand. Er landet in `export/<fort>-<datum>/` (ältere Exporte bleiben erhalten) und wird sofort einmal gespeichert, damit *Continue* ihn findet.

  Läuft DF nicht, antwortet DFHack nicht oder ist kein Fort geladen, steht dort, was zu tun ist; *Retry* prüft erneut.
- **New synthetic fort (dev)** ist eine kleine Testwelt für die Entwicklung.
- **Settings**: Größe der Oberfläche, Bildrate, Vsync, Autosave (Standard: jede Jahreszeit, die letzten 3 bleiben), Bevölkerungsgrenze und Invasionen für neue Importe, DF-Ordner, Hinweise wieder einblenden. Gespeichert in `saves/settings.json`.

**Im Spiel** fühlt sich alles wie Steam-DF an, mit deinen eigenen Tastenbelegungen aus DF (`interface.txt`):

- Das Spiel startet pausiert. **Leertaste** läuft/pausiert, **1 2 3 4** = Geschwindigkeit (1x ist DF-Tempo, 4 ist Maximum).
- **Mausrad** = eine Ebene hoch/runter (Shift: zehn), **Strg+Mausrad** = Zoom, **mittlere Maustaste ziehen** oder **WASD** = Karte verschieben.
- **Linksklick** auf Zwerg, Gebäude oder Feld öffnet die Infos; **Rechtsklick** oder **Esc** geht einen Schritt zurück.
- Die Knöpfe unten sind DFs Menüs (Kreaturen, Aufträge, Arbeit, Graben, Bauen, Zonen, Lager, Trupps ...). Mit der Maus darüber siehst du Namen und Taste.
- **F1** oder der **?**-Knopf oben rechts zeigt alle Tasten und Mausbefehle. Beim ersten Start erklären fünf kurze Hinweise das Wichtigste (wegklickbar, *No more hints* schaltet sie ab).
- **F5** speichert sofort. **Esc** öffnet das Menü: Speichern, Laden, Einstellungen, *Save and return to title* (speichert und zurück zum Titelbildschirm), Beenden.
- **Autosave** nach Einstellung (Standard: am Anfang jeder Jahreszeit, wie DFs saisonales Autosave) nach `saves/<fort>-auto-<zeit>.rfsave`; die letzten 3 bleiben, ältere Autosaves werden gelöscht. Eigene Spielstände (F5, Menü) werden nie gelöscht.

**Neu in Runde 8** (wie in Steam-DF):

- **Arbeit (`y`)** hat drei Reiter: *Work details* (DFs Arbeitsgruppen mit ihren Symbolen; rechts die gewählte Gruppe mit "Alle / Nur ausgewählte / Niemand", die Zwerge zum An- und Abhaken, eigene Gruppen mit "New work detail" und "Edit labors"), *Kitchen* (Kochen/Brauen je Zutat; Plump Helmets werden nur gebraut, deine Regel) und *Advanced* (das alte Raster aller Arbeiten).
- **Lager-Einstellungen:** im Lager-Fenster "Settings, links...": Kategorien, Gruppen und einzelne Sorten wie in DF, Vorlagen, Fässer/Kisten/Schubkarren, Verknüpfungen ("Give to..." / "Take from..." und dann ein anderes Lager oder eine Werkstatt auf der Karte anklicken).
- **Tiere:** Kreaturen > Haustiere > "Animals: manage..." (oder "Pick animals..." im Pferch): Schlachten, Kriegs-/Jagdausbildung, Weide zuweisen.
- **Orte (`P`):** Tempel, Tavernen, Bibliotheken mit ihren Ämtern (Wirt, Darsteller, Gelehrte ...); aus einem Versammlungsbereich einen Ort machen, beim Tempel mit Gottheit.
- **Justiz (`j`)**, **Objekte (`O`, Artefakte)** und **Welt (`Y`, Zivilisationen, Karawanen)** sind jetzt in der unteren Leiste.
- **Minikarte:** Knopf `▓` oben rechts; Klick auf die Karte springt dorthin.

Gefährlich oder wichtig wie in DF: Belagerungen und Monster pausieren das Spiel mit einem Hinweisfenster (*Center on them* springt hin). Der Trupp-Bildschirm (`c`) stellt die Soldaten auf Bereitschaft oder zurück an die Arbeit.

**Neu in Runde 9:**

- **Meldungen wie DFs `announcements.txt`:** Esc > Settings > *Announcements...* zeigt alle Ereignisarten mit *Box* (Fenster, pausiert), *Pause*, *Center* (Kamera springt hin) und *Log* (Meldungszeile). Die Vorgaben kommen aus deiner DF-Datei `prefs/announcements.txt`; deshalb öffnen wie bei dir in DF z. B. Tode von Bürgern, Migranten, Diebe, Kindesentführer, Wutanfälle und die Karawane ein Fenster. Was du hier änderst, steht in `saves/settings.json`. Mehrere Fenster stehen in einer Schlange ("1 of 5"); *Enter* schließt eins, *Close all* alle. Tote Feinde öffnen kein Fenster.
- **Überfälle:** Wird ein Hinterhalt, Dieb oder Kindesentführer entdeckt, kommt DFs Fenster ("Thief! Protect the hoard from skulking filth!", "Snatcher! Protect the children!", "An ambush! Curse them!") mit Pause, solange das in den Meldungen so eingestellt ist. Unentdeckte Angreifer bleiben unsichtbar.
- **Bittschriften (Petitions):** Bittet ein Besucher um Aufenthalt oder Bürgerrecht (oder eine Gilde/Religion um einen Ort), erscheint ein Fenster mit *Accept* / *Deny* / *Later*, und oben links blinkt "Petitions (n)". Der Knopf öffnet die Liste mit allen Bittschriften, Details und den verbleibenden Tagen (unbeantwortet = nach 28 Tagen abgelehnt).
- **Trupps (`c`)** haben Reiter: *Members*, *Equipment* (Uniform je Position: links die Positionen mit fehlenden Teilen in Rot, rechts je Körperteil Gegenstand `<` `>` und Material; *Use squad uniform*, *Copy to all positions*; darunter, was der Soldat trägt), *Ammunition* (Bolzen usw. mit Menge, Material, Kampf/Training) und *Patrol and barracks* (*New route*: Wegpunkte auf der Karte anklicken, Enter fertig, Rechtsklick nimmt den letzten zurück; Kasernen an- und abhaken).
- **Berichte (`Shift+n`) > Combat reports:** Kämpfe zusammengefasst wie in DF ("Kûbuk Beardedwhips and 9 more vs Nguslu Spiderword", 110 Zeilen, 1 tot); rechts alle Zeilen des Kampfes, *Go there* springt hin. Das rohe Kampfprotokoll gibt es weiter unter *Combat log*.
- **Fertigkeiten** im Zwergenblatt mit DFs Namen ("Legendary +5 Observer", "High Master Axe Dwarf") und einem Balken bis zur nächsten Stufe (Maus darüber: Erfahrung x / y). Alle Knöpfe unten und Pause/Tempo oben zeigen beim Darüberfahren ihre Taste.

**Neu in Runde 10:**

- **Aufgaben (`t`, Knopf *Tasks* unten links):** alle laufenden Arbeiten wie DFs Aufgabenliste: Graben/Fällen/Sammeln, Glätten/Gravieren, Bauen, Werkstattaufträge, Schleppen, Pflege, Soldaten und Essen/Trinken/Schlafen, jeweils mit Zwerg ("Inactive" = noch niemand) und Ort. Reiter mit Anzahl, Häkchen *Only without worker*. Klick auf eine Zeile springt hin und wählt den Zwerg. Werkstattaufträge: *Suspend*/*Resume* und *Do now* (an den Anfang der Warteschlange); *Cancel* löscht Bezeichnungen, Glätten/Gravieren, geplante Bauten, Werkstattaufträge und Müll-Markierungen. Oben steht, wie viele Zwerge gerade nichts zu tun haben.
- **Transportrouten (`Shift+h`):** die Routen aus deinem DF-Fort mit Haltestellen (*Go* springt hin), Abfahrtsbedingungen und Lagern. Nur zum Ansehen: Loren fahren in Rustfort noch nicht (dein Fort hat ohnehin keine Routen).
- **Bau-Tastenfolgen wie in DF** (aus deiner `interface.txt`): `b`, dann die Gruppe, dann das Gebäude. `b o t` = Steinmetz, `b o p` = Schreiner, `b o f l` = Brennerei (Still), `b o f k` = Küche, `b o u l` = Schmelze, `b o l` = Leder/Kleidung, `b f b` = Bett, `b n l` = Wand, `b p` = Feld. Oben in der Mitte zeigt eine Leiste, wo du bist und welche Taste was macht; die Knöpfe zeigen ihre Taste ("Still (l)"). *Esc* geht eine Stufe zurück. Graben bleibt wie gehabt (`m`, dann `m`/`u`/`t`/`r`/`x`).
- **Rechtsklick-Menü auf der Karte** (ohne aktives Werkzeug): Zwerg/Kreatur auswählen, Gebäude oder Zone öffnen, Grab-Bezeichnung, Glätten/Gravieren oder geplanten Bau abbrechen, Feld ansehen. Auf leeren Feldern hebt Rechtsklick wie bisher die Auswahl auf.
- **Feldbeschreibung wie in DF:** "A rough floor of rock salt, never smoothed.", "A smoothed wall of ...", "A constructed wall of ...". Gravuren mit Qualität: "The floor is engraved with a superior image of the fortress. *", dazu Graveur (mit Stufe) und Wert. Das Bildmotiv exportiert DF nicht; Rustfort nimmt ein allgemeines Motiv (steht so dabei). Gegenstände auf einem Feld werden zusammengefasst ("6 x +iron bucket+"), F/D gelten dann für die ganze Gruppe.
- **Absturzschutz:** Stürzt die Simulation oder die Oberfläche ab, bleibt das Fenster offen. Ein Fenster zeigt den Fehler und bietet *Save last good state* (Rustfort hält alle 60 Sekunden eine Kopie des Forts im Speicher), *Save it and continue from there* (speichert die Kopie und spielt von dort weiter), *Save current state* bzw. *Save now*, *Return to title* und *Quit*. Alle Fehler mit Ort und Backtrace stehen in `saves/crash.log` (bitte mitschicken, wenn etwas passiert).
- **Titelbildschirm > Settings > Announcements...:** die Meldungs-Einstellungen gibt es jetzt auch dort, nicht nur im Spiel.

**Neu in Runde 12** (Loren, Maschinen, Belagerungswaffen):

- **Transportrouten (`Shift+h`) sind jetzt ein Editor wie in Steam-DF:** *New route* legt eine Route an, *Delete route* löscht sie. *Add stop (click on the map)* schließt das Fenster; dann einen Gleisstopp oder ein Gleisfeld anklicken, das Fenster kommt mit der neuen Haltestelle zurück (Rechtsklick: zurück ohne). Je Haltestelle: *Go* (hinspringen), *x* (löschen), die Abfahrtsbedingung als Knöpfe (*right away* / *when full* / *when empty* / *any time*, *no wait* / *wait 1 day* / *wait 7 days*, *Push* / *Ride* / *Guide* und die Richtung N/S/E/W; daneben steht sie in Worten, z. B. "push north when full") und die Lager: *+ Take from pile* / *+ Give to pile*, dann ein Lager auf der Karte anklicken (*x* löst die Verbindung). *Assign minecart* zeigt die freien Loren und Schubkarren des Forts zum Zuweisen. Oben steht live, was die Route macht (Fahrten, wo jede Lore ist, wie viel geladen ist). Auf der Karte sind die Haltestellen nummeriert zu sehen, solange das Fenster oder eines seiner Werkzeuge offen ist; Loren einer Route haben einen blauen Rahmen mit "R1", "R2" ...
- **Bauen > Machines** (`b m`, wie in DF: `p` Schraubenpumpe, `h` Wasserrad, `m` Windmühle, `g` Getriebe, `a` waagrechte Achse, `Shift+a` senkrechte Achse, `r` Rolle) und der **Gleisstopp** (`b n Shift+k`); **Bauen > Siege engines** (`b y`: `b` Balliste, `p` Katapult, dazu der Bolzenwerfer). Wo es eine Richtung gibt, stehen oben im Baumenü Knöpfe dafür, **Tab** dreht weiter (Shift+Tab zurück): Pumpe "pump from north/east/south/west" (sie wird dabei 1x2 oder 2x1), Wasserrad Nord-Süd oder Ost-West, Rolle Schubrichtung, Gleisstopp Kipprichtung, Balliste/Katapult Blickrichtung. Ein blauer Pfeil im Bauvorschau-Bild zeigt sie (bei der Pumpe den Weg des Wassers). Achsen und Rollen zieht man als gerade Linie (höchstens 10 Felder). Pumpen, Wasserräder, Windmühlen, Getriebe und Achsen dürfen über offenem Raum hängen; Rollen, Gleisstopps und Belagerungswaffen brauchen glatten Boden.
- **Maschinen-Fenster** (eine Maschine anklicken): läuft sie oder nicht und warum (Wasserrad braucht fließendes Wasser darunter, Windmühle Wind, alles andere eine Verbindung über Achsen und Getriebe), wie viel Kraft sie macht oder braucht, bei der Pumpe die Richtung in Worten und *Pump by hand* (ein Zwerg mit "Pumpe bedienen" pumpt von Hand). *Power overlay on the map* färbt alle Maschinen: grün = läuft, rot = keine Kraft, grau = noch nicht gebaut. Rollen und Gleisstopps haben Richtungsknöpfe.
- **Belagerungswaffen-Fenster:** Blickrichtung, Munition (Ballistenpfeile, beim Katapult Steine, beim Bolzenwerfer Bolzen: geladen und im Fort), das Ziel, *Aim at a tile...* (Zielfeld anklicken; Linie und Fadenkreuz auf der Karte) und *Fire at will* (der Bediener sucht sich selbst Ziele).
- **Grafik:** Pumpen, Wasserräder, Windmühlen, Getriebe, Achsen, Rollen, Gleisstopps, Ballisten, Katapulte und Bolzenwerfer mit DFs Premium-Bildern in ihrer Richtung; **Gleise** (geschnitzt mit Glätten > *Carve track* oder gebaut mit Bauen > Constructions > Track) zeigen ihre Richtungen, Kurven, Kreuzungen und Enden wie in DF.
- Ob Loren wirklich fahren, Pumpen pumpen und Ballisten schießen, macht die Simulation (die anderen Teile von Runde 12); die Fenster hier sind die Bedienung dafür.
