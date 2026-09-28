#!/usr/bin/env bash
# dx can't set the Android icon, app name or theme (DioxusLabs/dioxus#3685), so overlay android/res onto
# the gradle project it generates and repackage. Args go to `dx build --android`.
set -euo pipefail
cd "$(dirname "$0")"
dx build --android "$@"
profile=debug; [[ " $* " == *" --release "* ]] && profile=release
app=../../target/dx/mobile/$profile/android/app
cp -r android/res/. "$app/app/src/main/res/"
(cd "$app" && ./gradlew -q assembleDebug)
echo "$app/app/build/outputs/apk/debug/app-debug.apk"
