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
