# Third-party notices

Rustfort's own code and documentation are licensed under MIT OR Apache-2.0
([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)). This file lists the material in
this repository that comes from others and is not covered by that license, and what a binary
distribution of Rustfort has to include.

Rustfort contains no Dwarf Fortress code. DF's graphics, fonts and raws are read from the
player's own installation at runtime and are never copied into the repository.

## 1. DF thought captions (`crates/rf-sim/src/social/captions.rs`)

The table `CAPTIONS` in `crates/rf-sim/src/social/captions.rs` (about 240 entries, for example
`("WitnessDeath", "after seeing somebody die")`) holds the `caption` attribute of the
`unit_thought_type` enum from DFHack's df-structures, <https://github.com/DFHack/df-structures>.
Rustfort uses these captions to word thoughts the way DF does.

- **Origin:** the strings originate from Dwarf Fortress (Bay 12 Games). They are DF's own
  wording of thought circumstances, recorded in df-structures by the DFHack contributors.
- **License:** df-structures has no license file of its own. It is part of DFHack, whose
  `LICENSE.rst` states that DFHack's code uses the zlib license unless noted otherwise, with
  contributors releasing their work under that license, and gives DFHack's copyright line as
  "(c) 2009-2012, Petr Mrázek".
- **Changes:** the captions were converted from df-structures' XML into a Rust array. This
  notice marks that altered form, as clause 2 of the license asks.

The license text below is the zlib license as DFHack's `LICENSE.rst` reproduces it (the
standard zlib license), headed by DFHack's copyright line and its contributors, who release
their work under the same license:

```text
DFHack, including df-structures
Copyright (c) 2009-2012, Petr Mrázek, and the DFHack contributors

This software is provided 'as-is', without any express or implied
warranty. In no event will the authors be held liable for any
damages arising from the use of this software.

Permission is granted to anyone to use this software for any
purpose, including commercial applications, and to alter it and
redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must
   not claim that you wrote the original software. If you use this
   software in a product, an acknowledgment in the product
   documentation would be appreciated but is not required.

2. Altered source versions must be plainly marked as such, and
   must not be misrepresented as being the original software.

3. This notice may not be removed or altered from any source
   distribution.
```

## 2. README screenshots (`docs/screenshots/*.png`)

The README screenshots in `docs/screenshots/` (`fort.png`, `unit-sheet.png`, `military.png`,
`siege.png`, `ballista.png`) show Rustfort drawing Dwarf Fortress's Premium graphics: sprites,
portraits, UI icons and the font, (c) Bay 12 Games / Kitfox Games, read at runtime from a DF
installation. They are shown for illustration only. They are not licensed by this project and
are not covered by MIT or Apache-2.0. They are the only DF imagery in the repository.

## 3. Game facts in the code

A few small compiled-in tables hold game facts: token names and numbers, for example for
reactions, default materials, armor and ores. They come from the Dwarf Fortress Wiki, the
raws and observing the game, and they contain no DF files or text. They are used where DF hard-codes a
rule (such as the kiln, glass and ashery jobs) and as fallbacks where a fort export or the
built-in test fort has no raws for something.

## 4. Binary distributions

A **source distribution** needs nothing beyond this repository: Cargo downloads every
dependency, with its license information, when it builds.

A **binary distribution** of `rustfort.exe` (or `rustfort` on Linux) is different. The
executable statically links Rust crates, and through `zstd-sys` the zstd C library, under the
MIT, Apache-2.0, BSD-3-Clause (zstd) and Zlib licenses. Many crates offer a choice, such as
MIT OR Apache-2.0. These licenses require their copyright notices and license texts to
accompany the binary. Ship them next to `rustfort.exe` together with `LICENSE-MIT`,
`LICENSE-APACHE` and this file. Crates that run only while building (procedural macros, build
scripts) are not linked into the binary. The current list follows from `Cargo.lock`, for
example with `cargo metadata` or a tool such as cargo-about. Do not put DF files, fort exports
or saves into a binary distribution: they contain DF data.
