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

Gefährlich oder wichtig wie in DF: Belagerungen und Monster pausieren das Spiel mit einem Hinweisfenster (*Center on them* springt hin). Der Trupp-Bildschirm (`c`) stellt die Soldaten auf Bereitschaft oder zurück an die Arbeit.
