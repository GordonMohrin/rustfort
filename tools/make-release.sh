#!/usr/bin/env bash
# Build a LOCAL PLAY FOLDER for a tagged Rustfort version (Windows, Git Bash):
#   tools/make-release.sh v0.12.1                     ->  ../rustfortress-v0.12.1
#   tools/make-release.sh v0.12.0 --with-export       ->  ../rustfortress-v0.12, plus the newest
#                                                          complete fort export under export/
#   tools/make-release.sh v0.12.0 --with-export=DIR   ->  the same with the export folder DIR
#
# This is a folder to play a fixed version next to the checkout, NOT a publishable release.
# It holds rustfort.exe (built from the tag), the DFHack scripts, README/PROGRESS, LICENSE-MIT,
# LICENSE-APACHE and THIRD-PARTY-NOTICES.md (when the tag has them) and "Start Rustfort.cmd".
# Saves go to <folder>\saves. DF graphics and raws are read from the DF install at runtime and
# are never copied. A fort export is DF data (it comes from a DF save): it is copied only with
# --with-export, and such a folder is for local use only and must never be published.
# Without an export the folder starts with the built-in test fort until a fort is imported.
#
# A public binary release would also have to ship the licenses of all compiled-in
# dependencies (for example generated with cargo-about); this script does not do that.
set -euo pipefail
usage="usage: tools/make-release.sh vX.Y.Z [--with-export[=DIR]]"
tag=""
with_export=""
export_dir=""
for arg in "$@"; do
    case "$arg" in
        --with-export) with_export=1 ;;
        --with-export=*) with_export=1; export_dir="${arg#--with-export=}" ;;
        -h|--help) echo "$usage"; exit 0 ;;
        -*) echo "unknown option $arg"; echo "$usage"; exit 2 ;;
        *) if [ -n "$tag" ]; then echo "$usage"; exit 2; fi; tag="$arg" ;;
    esac
done
[ -n "$tag" ] || { echo "$usage"; exit 2; }
repo="$(cd "$(dirname "$0")/.." && pwd)"
short="$(echo "$tag" | sed -E 's/^v([0-9]+)\.([0-9]+)\.0$/v\1.\2/')"
out="$(cd "$repo/.." && pwd)/rustfortress-$short"
if [ -e "$out" ]; then
    echo "exists: $out (not overwritten)"
    exit 1
fi
# The export to copy (only with --with-export): DIR, else the newest complete export under
# export/ (manifest.json present, no export.lock), the one the game itself would pick.
if [ -n "$with_export" ]; then
    if [ -z "$export_dir" ]; then
        while IFS= read -r m; do
            d="$(dirname "$m")"
            if [ ! -e "$d/export.lock" ]; then export_dir="$d"; break; fi
        done < <(ls -t "$repo"/export/*/manifest.json 2>/dev/null)
    fi
    if [ -z "$export_dir" ] || [ ! -f "$export_dir/manifest.json" ]; then
        echo "--with-export: no complete fort export found (${export_dir:-none under export/})"
        exit 1
    fi
fi
export PATH="$HOME/.cargo/bin:$PATH"
src="$repo/target/release-src"
git -C "$repo" worktree remove --force "$src" 2>/dev/null || rm -rf "$src"
git -C "$repo" worktree add -q --detach "$src" "$tag"
CARGO_TARGET_DIR="$repo/target/release-build" cargo build -q --release -p rf-app --manifest-path "$src/Cargo.toml"
mkdir -p "$out/export" "$out/tools"
cp "$repo/target/release-build/release/rustfort.exe" "$out/"
cp -r "$src/tools/dfhack" "$out/tools/"
cp "$src/README.md" "$out/README.md"
cp "$src/PROGRESS.md" "$out/PROGRESS.md"
for f in LICENSE-MIT LICENSE-APACHE THIRD-PARTY-NOTICES.md; do
    if [ -f "$src/$f" ]; then
        cp "$src/$f" "$out/$f"
    else
        echo "note: $tag has no $f"
    fi
done
if [ -n "$with_export" ]; then
    cp -r "$export_dir" "$out/export/"
    echo "copied export: export/$(basename "$export_dir")"
    echo "contains a DF fort export: for local use only, never publish this folder"
else
    echo "no fort export copied (use --with-export for one): the folder starts with the built-in test fort"
fi
printf '@echo off\r\nrem Rustfort %s - saves go to saves\\ in this folder.\r\ncd /d "%%~dp0"\r\nstart "" "%%~dp0rustfort.exe" %%*\r\n' "$tag" > "$out/Start Rustfort.cmd"
git -C "$repo" worktree remove --force "$src"
(cd "$out" && timeout 60 ./rustfort.exe play --screenshot shot_test.png --frames 10 > /dev/null 2>&1 && rm -f shot_test.png && rm -rf saves) && echo "smoke test ok"
echo "ready: $out (local play folder, not a release)"
