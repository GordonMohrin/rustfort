# Installing Rustfort

This guide takes you from a Windows PC with Dwarf Fortress installed to a running Rustfort,
and then to playing your own fort in it. It is written for people and for AI coding agents.
Each step gives the command, how to check it worked (a command and the output to expect), and
what to do if it failed. Do the steps in order and skip the ones that are already done.

Most of the time goes into downloads and the first build, so plan for about 20-40 minutes.

**You need:**

- Windows 10 or 11 (64-bit). Linux is untested, see [Linux (best effort)](#linux-best-effort).
- **Dwarf Fortress v50 or newer, the Steam edition** (it includes the Premium graphics that
  Rustfort draws with). Rustfort ships none of DF's files and reads them from your install.
- About 5 GB of free disk space: up to 3 GB for the C++ build tools, 2-4 GB for Rustfort's
  build folder, and about 70 MB per fort export.
- An internet connection for the downloads.

All commands are for **PowerShell** (Windows Terminal or "Windows PowerShell"). Commands
after step 5 are run **in the Rustfort folder**.

## Windows

### Step 1: Check that Dwarf Fortress is installed

Rustfort needs the DF folder with the v50 layout, which means it contains `data\vanilla`.

**Do:** find the folder. In Steam, right-click *Dwarf Fortress* > *Manage* > *Browse local
files*. The folder that opens is the DF folder. With the default Steam library it is
`C:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress`.

**Check:**

```powershell
Test-Path "C:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress\data\vanilla"
```

Expected: `True`. If your Steam library is somewhere else, use that path instead. To list
every Steam library folder:

```powershell
$steam = (Get-ItemProperty HKCU:\Software\Valve\Steam).SteamPath
Select-String -Path "$steam\steamapps\libraryfolders.vdf" -Pattern '"path"'
```

DF is then in `<library>\steamapps\common\Dwarf Fortress`.

**If it fails:** install Dwarf Fortress from Steam and start it once. If the folder has no
`data\vanilla`, the version is older than v50. Update it in Steam.

**How Rustfort finds the DF folder:** it checks these places in order and uses the first
folder that contains `data\vanilla`:

1. the environment variable `RUSTFORT_DF_DIR`;
2. the folder set in the title screen under **Settings** (stored in `saves\settings.json`);
3. the Steam install, found automatically. It reads Steam's location from the registry
   (`HKCU\Software\Valve\Steam` value `SteamPath`, then `HKLM\SOFTWARE\WOW6432Node\Valve\Steam`
   value `InstallPath`), then looks in every library listed in
   `<Steam>\steamapps\libraryfolders.vdf` for `steamapps\common\Dwarf Fortress`;
4. common default paths: `C:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress`,
   `C:\Program Files\Steam\steamapps\common\Dwarf Fortress`, and
   `<drive>:\SteamLibrary\steamapps\common\Dwarf Fortress` for drives C to H.

So with Steam you normally have nothing to set. If DF is somewhere else (another store, a copy
of the folder), set it in **Settings** after step 7, or for one terminal session:

```powershell
$env:RUSTFORT_DF_DIR = "D:\Games\Dwarf Fortress"
```

### Step 2: Install Git

**Check first:**

```powershell
git --version
```

Expected: `git version 2.` followed by more numbers. If that works, go to step 3.

**Do:**

```powershell
winget install --id Git.Git -e
```

Then **close the terminal and open a new one** so that `git` is on the PATH, and run the check
again.

**If it fails:** if `winget` is not found, install or update *App Installer* from the
Microsoft Store, or download Git from <https://git-scm.com/download/win> and install it with
the default options.

### Step 3: Install the Visual Studio 2022 Build Tools (C++)

Rust on Windows uses Microsoft's linker (`link.exe`), and one dependency (the zstd compression
library) is compiled from C. The linker and the C compiler both come with the free Build Tools. A full Visual Studio 2022 with the *Desktop development
with C++* workload works too.

**Check first:**

```powershell
& "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property displayName
```

Expected: a line such as `Visual Studio Build Tools 2022`, or the name of your Visual Studio
edition. No output, or "not recognized", means the C++ tools are missing.

**Do:**

```powershell
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive"
```

This downloads a few GB and shows the Visual Studio installer's progress window. Windows asks
for administrator permission. Wait until the installer window closes, then run the check again.

**If it fails:** start *Visual Studio Installer* from the Start menu. Then choose *Modify* on
*Build Tools 2022*, tick *Desktop development with C++* and click *Modify*.

### Step 4: Install Rust

**Check first:**

```powershell
cargo --version
rustup show active-toolchain
```

Expected: `cargo 1.` followed by numbers, and a toolchain ending in `-x86_64-pc-windows-msvc`
(for example `stable-x86_64-pc-windows-msvc (default)`). If both are right, go to step 5.

**Do:**

```powershell
winget install --id Rustlang.Rustup -e
```

**Close the terminal and open a new one**, then:

```powershell
rustup default stable-msvc
```

Run the check again.

**If it fails:**

- `cargo` is not recognized: open a new terminal. If it is still missing, the installer did not
  update the PATH. Add `%USERPROFILE%\.cargo\bin` to it, or run the installer from
  <https://rustup.rs> instead.
- The toolchain ends in `-gnu`: run `rustup default stable-msvc`.

Rustfort builds with current stable Rust. It is tested with Rust 1.99.

### Step 5: Get the code

Pick a folder for Rustfort that OneDrive does not sync (the build folder is large and changes
often) and that is **not inside the DF folder**. For example, your user folder:

**Do:**

```powershell
cd $HOME
git clone https://github.com/GordonMohrin/rustfort.git
cd rustfort
```

**Check:**

```powershell
Test-Path tools\dfhack\rustfort-export.lua
```

Expected: `True`. Run all later commands in this `rustfort` folder.

**If it fails:** if `git clone` reports a network or proxy error, check the internet
connection. If the folder `rustfort` already exists, either `cd rustfort` and use it, or clone
into another folder.

### Step 6: Build

**Do:**

```powershell
cargo build --release -p rf-app
```

The first build downloads the dependencies and compiles everything. It takes several minutes
(5-15 on a typical desktop). Later builds only compile what changed.

**Check:**

```powershell
Test-Path target\release\rustfort.exe
```

Expected: `True`. The build itself ends with a line starting with `Finished` and mentioning
`release`.

**If it fails:** see [Troubleshooting](#troubleshooting). The most common cause is
`link.exe not found` (step 3 is missing).

### Step 7: Start Rustfort

**Do:** either of these, from the Rustfort folder:

```powershell
.\rustfort.cmd
```

```powershell
target\release\rustfort.exe
```

`rustfort.cmd` rebuilds Rustfort when the code has changed, then starts it, and it always runs
from its own folder. You can also double-click `rustfort.cmd` in Explorer. Rustfort keeps
`saves\` and `export\` in the folder it was started from, so don't start `rustfort.exe` from
another folder.

**Check:** a window titled "Rustfort" opens with the title screen. Close it with **Quit** (or
the window's close button).

**If it fails:** an error in the terminal names the problem. Also see
[Troubleshooting](#troubleshooting).

### Step 8: The first start

On the title screen:

- If Rustfort found Dwarf Fortress, you see DF's own title art and logo, "RUSTFORT", and the
  buttons **Continue**, **Load game**, **Import from Dwarf Fortress**, **New synthetic fort
  (dev)**, **Settings** and **Quit**.
- **Continue** is greyed out on the first start. It becomes active once you have a save or a
  fort export.
- If Rustfort did **not** find Dwarf Fortress, the title screen says **"Dwarf Fortress not
  found"** and points you to Settings or `RUSTFORT_DF_DIR`. It is a plain text screen without
  DF's art, because the graphics, the font and the raws all come from the DF folder.

To set the DF folder, open **Settings**. The *DF folder* field takes a click, typing, or Ctrl+V
to paste. **Default** goes back to automatic detection. A line under the field says what it
found:

- "Found Dwarf Fortress and DFHack."
- "Found Dwarf Fortress, but no DFHack (needed for the import)."
- "No Dwarf Fortress there (graphics, font and import need it)."

**Back** (or Esc) saves the settings to `saves\settings.json`.

To look around without your own fort, choose **New synthetic fort (dev)**: a small generated
test fort. The headless tools (`rustfort.exe bench` and the examples) fall back to the same
test fort when they find no fort export, and they say so.

In the game: **Space** pauses and resumes, **1**-**4** set the speed, the **mouse wheel** changes
the z-level, **Ctrl+wheel** zooms, and **F1** shows all keys. **Esc** opens the menu with save,
load, settings and *Save and return to title*.

### Step 9: Import your own fort

Rustfort copies a fort from a **running** Dwarf Fortress through DFHack. The copy is read-only:
DF is never paused and nothing in DF or its saves is changed. DF only runs a little slower
while the export runs.

**9a. Install DFHack.** DFHack is free. On Steam it is its own app called *DFHack*: search the
Steam store for it and install it. It installs into the DF folder. Without Steam, see
<https://dfhack.org>.

**Check:**

```powershell
Test-Path "C:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress\hack\dfhack-run.exe"
```

(use your DF folder from step 1). Expected: `True`.

**9b. Start Dwarf Fortress** from Steam and load your fort in fortress mode. With DFHack
installed, DF shows the DFHack version in a corner of its title screen. The game can keep
running.

**9c. In Rustfort**, choose **Import from Dwarf Fortress**. After a moment it shows what it found,
for example "Found Trussroads (Tileshtulon) in the world Zilirróth." with the year, citizens and
DFHack version. Click **Import and play**. A progress bar follows the export, which takes about
a minute for a large fort. The fort is then loaded and saved once as an autosave, and the game
starts. **Continue** on the title screen picks it up next time.

Each import gets its own folder, `export\<fort>-<yyyymmdd-hhmmss>\` (UTC time) in the Rustfort
folder. Earlier exports are kept, and **Load game** > *DF exports* lists them.

**If it fails:** the dialog says what is wrong and offers **Retry**:

| message | what to do |
|---|---|
| "DFHack was not found." | Check the DF folder in Settings, and install DFHack (9a). |
| "Dwarf Fortress is not running." | Start DF, load the fort, press Retry. |
| "Dwarf Fortress is running, but DFHack does not answer." | Make sure DFHack is installed and enabled (its version shows on DF's title screen). See also [Troubleshooting](#troubleshooting). |
| "DFHack answers, but no fort is loaded." | Load a fort in fortress mode (not the title screen, world generation or adventure mode), then Retry. |
| "The Rustfort DFHack scripts were not found" | Start Rustfort from its folder (step 7), so it finds `tools\dfhack`. |

The DFHack scripts were developed against DF 53.16 with DFHack 53.16-r2. If an export fails
with another version, please open an issue with the message from `error.json` in the export
folder.

#### Exporting by hand

You can run the exporter yourself, for example from a script. In the Rustfort folder, with DF
running and the fort loaded:

```powershell
$DF = "C:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress"   # your DF folder
& "$DF\hack\dfhack-run.exe" lua -f "$PWD\tools\dfhack\rustfort-export.lua" "$PWD\export\myfort"
```

The command returns at once while the export runs inside DF. The export is finished when
`export\myfort\manifest.json` exists. If `export\myfort\error.json` appears instead, the export
failed. While it runs, `progress.json` in that folder shows the phase.

- The exporter accepts any loaded fort. `--expect NAME` makes it refuse to run unless the
  loaded fort's native name is NAME.
- Without an output folder, it writes to `<DF folder>\rustfort-export\<native fort name in
  lower case>` and prints where it wrote. Giving a folder under `export\`, as above, keeps
  everything in the Rustfort folder, and Rustfort finds the export there by itself.
- All options, such as `--budget`, `--status` and `--cancel`, are in
  [tools/dfhack/README.md](../tools/dfhack/README.md).

Play the export with:

```powershell
target\release\rustfort.exe play --export export\myfort
```

**Which export Rustfort uses by default:** the game (when started straight into play), `bench`
and the example tools take `--export DIR` if given. Otherwise they use the environment variable
`RUSTFORT_EXPORT`. Failing that, they use the most recently modified complete export in
`export\` (a folder with the exporter's `manifest.json`). If there is none, they use the
built-in synthetic test fort and say so.

### Step 10: Update

**Do:**

```powershell
git pull
cargo build --release -p rf-app
```

(or just `.\rustfort.cmd`, which builds and starts).

**Check:** the build ends with `Finished`. The title screen then works as before, and
**Continue** loads your newest save.

Saves stay loadable across updates: new fields get defaults, and saves from v0.9 and older are
converted on load. Exports made by an older exporter revision load as well. They just lack the
newer data.

**If it fails:** if `git pull` reports local changes, you edited files in the repository. Run
`git stash` to put your edits aside, or `git checkout -- .` to discard them, then pull again.
Your `saves\` and `export\` folders are not tracked by Git and are not touched.

### Step 11: Uninstall

**Do:** delete the Rustfort folder. That removes the code, the build, your saves (`saves\`) and
your fort exports (`export\`). Copy those two folders first if you want to keep them.

Rustfort installs nothing system-wide and writes only inside its own folder. The one exception
is the manual exporter: run without an output folder, it writes to
`<DF folder>\rustfort-export\`. Delete that folder too if it exists. The import from the title
screen always writes into the Rustfort folder.

Optionally, uninstall the tools in *Settings* > *Apps*: *Rustup* (or run `rustup self uninstall`),
*Visual Studio Build Tools 2022* and *Git*. DFHack can be uninstalled in Steam.

## Troubleshooting

**"Dwarf Fortress not found" on the title screen**

- Check that the folder contains `data\vanilla` (step 1). An older DF (before v50) does not
  work.
- Set the folder in **Settings** > *DF folder*. You can also set `RUSTFORT_DF_DIR` before
  starting Rustfort. `setx RUSTFORT_DF_DIR "D:\Games\Dwarf Fortress"` stores it for new
  terminals.
- The variable wins over the Settings value. If Settings seems to have no effect, check that
  `RUSTFORT_DF_DIR` is not set to an old folder: `echo $env:RUSTFORT_DF_DIR`.

**`error: linker 'link.exe' not found`, or a build error about `cl.exe` or the C compiler**

- The C++ Build Tools are missing or incomplete. Do step 3, then open a **new** terminal and
  build again.
- Check that the Rust toolchain is the MSVC one: `rustup show active-toolchain` must end in
  `-msvc`. If not, run `rustup default stable-msvc`.

**DFHack does not answer**

- DF must be running with DFHack loaded, and DF shows the DFHack version on its title screen.
  Starting DF through Steam with the DFHack app installed loads it.
- `dfhack-run` talks to DF over `localhost` port 5000 (`DFHACK_PORT` changes the port).
  If Windows asks whether DF may use the network, allow it for private networks. Security
  software that blocks local connections can also stop it.
- Check DFHack by hand with the read-only probe, from the Rustfort folder:

  ```powershell
  $DF = "C:\Program Files (x86)\Steam\steamapps\common\Dwarf Fortress"   # your DF folder
  & "$DF\hack\dfhack-run.exe" lua -f "$PWD\tools\dfhack\rustfort-probe.lua"
  ```

  Expected: one line of JSON starting with `{"ok":true` that names the loaded fort
  (`"map_loaded":false` means no fort is loaded yet). No output at all means nothing answered
  on the port.

**The build is very slow**

- The first build compiles about a hundred crates, so several minutes is normal.
- Antivirus scanning of the build folder can make it much slower. You can add an exclusion for
  the Rustfort folder in *Windows Security* > *Virus & threat protection* > *Manage settings* >
  *Exclusions*. That is your decision to make. Rustfort does not need it.
- Other heavy programs (including Dwarf Fortress) running at the same time slow the build down.

**Out of disk space**

- `cargo clean` deletes the build folder (`target\`, 2-4 GB). The next build starts from scratch.
- Old fort exports in `export\` (about 70 MB each) and old saves in `saves\` (a few MB each)
  can be deleted. Autosaves rotate by themselves (the newest 3 per fort are kept).

**Saves or exports seem to be missing**

- Rustfort looks in `saves\` and `export\` of the folder it was started from. Start it with
  `rustfort.cmd`, or from the Rustfort folder (step 7).

**Something crashed**

- Rustfort catches most crashes and offers to save the last good state. Details of every crash
  are in `saves\crash.log`. Please attach that file when you report a bug.

## Linux (best effort)

Linux is **untested**, so expect rough edges. Reports and fixes are welcome.

1. **Dwarf Fortress:** the Steam Linux version (v50 or newer). Rustfort looks for Steam in
   `~/.steam/steam`, `~/.local/share/Steam` and the Flatpak location
   `~/.var/app/com.valvesoftware.Steam/.local/share/Steam`, then in every library from
   `steamapps/libraryfolders.vdf`, for `steamapps/common/Dwarf Fortress`. Otherwise set
   `RUSTFORT_DF_DIR` or the folder in Settings, as on Windows.
2. **System packages** for building (a C compiler, and the X11/OpenGL headers the window
   library needs):
   - Debian/Ubuntu: `sudo apt install build-essential git pkg-config libx11-dev libxi-dev libgl1-mesa-dev`
   - Fedora: `sudo dnf install gcc git pkgconf-pkg-config libX11-devel libXi-devel mesa-libGL-devel`
   - Arch: `sudo pacman -S base-devel git pkgconf libx11 libxi mesa`
3. **Rust:** `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh` (or your
   distribution's `rustup` package), then open a new shell. Check: `cargo --version`.
4. **Build and start:**

   ```sh
   git clone https://github.com/GordonMohrin/rustfort.git
   cd rustfort
   cargo build --release -p rf-app
   ./target/release/rustfort
   ```

5. **Import:** DFHack for Linux installs `hack/dfhack-run` into the DF folder, and the import
   uses it. Telling "DF is not running" apart from "DFHack does not answer" only works on
   Windows, so on Linux both show as "DFHack does not answer". On Linux Rustfort reads key
   bindings and announcement settings only from the DF folder's `prefs/` and `data/init/`, not
   from your DF user folder.

## Notes for AI agents

If you are an AI agent installing Rustfort for a user:

- **Ask before installing any software** (Git, Build Tools, Rust, DFHack) **or changing any
  setting** (environment variables, PATH, antivirus exclusions, firewall). Say what you want
  to install and why, and wait for a yes.
- **Never modify the Dwarf Fortress installation or the user's DF saves.** Do not copy, move,
  rename or delete anything in the DF folder or in `%APPDATA%\Bay 12 Games\Dwarf Fortress`.
  Only read them. Leave DFHack's installation to the user, through Steam (step 9a).
- Run the steps in order. Do the *Check* of each step, compare its output with the expected
  output, and stop and report when a step fails and its *If it fails* advice does not help.
- Start the app normally (`.\rustfort.cmd` or `target\release\rustfort.exe`) with no special
  flags. Don't use debug, test or screenshot options. Close it again with **Quit**, or let the
  user do it.
- Do not start an import unless the user asks for it. If they do, only use the title screen
  (step 9) or the manual exporter command above. Both are read-only towards DF.
- At the end, report what you installed, which checks passed, which DF folder Rustfort found
  (the title screen or Settings shows it), and anything that failed, with the exact error
  message.
