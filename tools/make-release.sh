#!/usr/bin/env bash
# Build a play-ready sibling folder for a tagged Rustfort version (Windows, Git Bash):
#   tools/make-release.sh v0.11.0      ->  ../rustfortress-v0.11
# The folder holds rustfort.exe (built from the tag), the DFHack scripts, README/PROGRESS, a
# start script and, when there are any, the fort exports of this checkout: export/tileshtulon
# if it exists, else everything under export/. Without an export the release starts with the
# built-in test fort until a fort is imported. Saves go to <folder>\saves. DF graphics and raws
# are read from the DF install at runtime (never copied).
set -euo pipefail
tag="${1:?usage: tools/make-release.sh vX.Y.Z}"
repo="$(cd "$(dirname "$0")/.." && pwd)"
short="$(echo "$tag" | sed -E 's/^v([0-9]+)\.([0-9]+)\.0$/v\1.\2/')"
out="$(cd "$repo/.." && pwd)/rustfortress-$short"
if [ -e "$out" ]; then
    echo "exists: $out (not overwritten)"
    exit 1
fi
export PATH="$HOME/.cargo/bin:$PATH"
src="$repo/target/release-src"
git -C "$repo" worktree remove --force "$src" 2>/dev/null || rm -rf "$src"
git -C "$repo" worktree add -q --detach "$src" "$tag"
CARGO_TARGET_DIR="$repo/target/release-build" cargo build -q --release -p rf-app --manifest-path "$src/Cargo.toml"
mkdir -p "$out/export" "$out/tools"
cp "$repo/target/release-build/release/rustfort.exe" "$out/"
cp -r "$src/tools/dfhack" "$out/tools/"
if [ -d "$repo/export/tileshtulon" ]; then
    cp -r "$repo/export/tileshtulon" "$out/export/"
    echo "copied export: export/tileshtulon"
elif [ -d "$repo/export" ] && [ -n "$(ls -A "$repo/export" 2>/dev/null)" ]; then
    cp -r "$repo/export/." "$out/export/"
    echo "copied exports: all of export/"
else
    echo "no fort export copied (none under export/): the release starts with the built-in test fort"
fi
cp "$src/README.md" "$out/README.md"
cp "$src/PROGRESS.md" "$out/PROGRESS.md"
printf '@echo off\r\nrem Rustfort %s - saves go to saves\\ in this folder.\r\ncd /d "%%~dp0"\r\nstart "" "%%~dp0rustfort.exe" %%*\r\n' "$tag" > "$out/Start Rustfort.cmd"
git -C "$repo" worktree remove --force "$src"
(cd "$out" && timeout 60 ./rustfort.exe play --screenshot shot_test.png --frames 10 > /dev/null 2>&1 && rm -f shot_test.png && rm -rf saves) && echo "smoke test ok"
echo "ready: $out"
