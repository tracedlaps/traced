#!/usr/bin/env bash
# Renders the Play Store icon, feature graphic and phone screenshots into out/.
set -euo pipefail
cd "$(dirname "$0")"
mkdir -p out
# The launcher-visible 72dp of the master, square: Play rounds it itself
sed 's/viewBox="0 0 108 108"/viewBox="18 18 72 72"/' ../assets/icon.svg | rsvg-convert -w 512 -h 512 -o out/icon-512.png
chromium --headless --disable-gpu --hide-scrollbars --allow-file-access-from-files --window-size=1024,500 \
    --virtual-time-budget=5000 --screenshot="$PWD/out/feature-graphic.png" "file://$PWD/feature-graphic.html" 2>/dev/null
# Play wants the feature graphic without alpha
magick out/feature-graphic.png -alpha off out/feature-graphic.png
# Phone screenshots per language, in store order: raw/<lang>/<name>.png and its caption (*orange*)
shots() {
    local lang=$1; shift
    mkdir -p "out/$lang"
    for i in $(seq 1 $#); do
        IFS='|' read -r name caption <<< "${!i}"
        out="$PWD/out/$lang/screenshot-$i-$name.png"
        chromium --headless --disable-gpu --hide-scrollbars --allow-file-access-from-files --window-size=1080,1920 \
            --virtual-time-budget=5000 --screenshot="$out" \
            "file://$PWD/screenshot.html?shot=$lang/$name&caption=$(python3 -c 'import sys,urllib.parse;print(urllib.parse.quote(sys.argv[1]))' "$caption")" 2>/dev/null
        magick "$out" -alpha off "$out"
    done
}
shots en \
    "live|Live delta to your *best*" \
    "results|See where you *braked*" \
    "charts|Every run against your *best*" \
    "editor|Draw any track on the *map*" \
    "tracks|Circuits, runs and *sectors*" \
    "history|Every session, kept on *your phone*"
shots pt \
    "live|Diferença ao vivo para a sua *melhor*" \
    "results|Veja onde *travou*" \
    "charts|Cada percurso contra o seu *melhor*" \
    "editor|Desenhe qualquer pista no *mapa*" \
    "tracks|Circuitos, troços e *setores*" \
    "history|Cada sessão, guardada no *seu telemóvel*"
