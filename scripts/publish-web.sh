#!/usr/bin/env bash
# Builds the website and pushes it to the public tracedlaps repo (served by GitHub Pages).
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
pub=${PUBLIC_REPO:-$root/../tracedlaps}

[ -d "$pub/.git" ] || git clone git@github.com:tracedlaps/tracedlaps.git "$pub"
git -C "$pub" ls-remote -q --exit-code origin main >/dev/null && git -C "$pub" pull --ff-only

cd "$root" && dx bundle --package web --platform web --release
find "$pub" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +
cp -r target/dx/web/release/web/public/. "$pub"
# Raw, for the app to show what's new before an update.
mkdir -p "$pub/changelog" && cp packages/changelog/*.md "$pub/changelog/"
# The page shell too, so tracedlaps.com/changelog and /privacy load the router's pages directly.
cp "$pub/index.html" "$pub/changelog/"
mkdir -p "$pub/privacy" && cp "$pub/index.html" "$pub/privacy/"
echo tracedlaps.com > "$pub/CNAME" && touch "$pub/.nojekyll"

git -C "$pub" add -A
git -C "$pub" commit -m "site: $(git rev-parse --short HEAD)" && git -C "$pub" push -u origin HEAD:main || echo "No changes"
