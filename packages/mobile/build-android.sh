#!/usr/bin/env bash
# dx can't set the Android icon, app name or theme (DioxusLabs/dioxus#3685), so overlay android/res onto
# the gradle project it generates and repackage. Args go to `dx build --android`.
# --release also signs with ~/.config/traced/release.jks (password in release.pass beside it).
set -euo pipefail
cd "$(dirname "$0")"
dx build --android "$@"
[[ " $* " == *" --release "* ]] && profile=release || profile=debug
app=$(realpath ../../target/dx/mobile/$profile/android/app)
cp -r android/res/. "$app/app/src/main/res/"
if [[ $profile == debug ]]; then
    (cd "$app" && ./gradlew -q assembleDebug)
    echo "$app/app/build/outputs/apk/debug/app-debug.apk"
    exit
fi
# R8 would strip the plugin classes Rust looks up by name over JNI; release lint crashes on JDK 25
sed -i -e 's/isMinifyEnabled = true/isMinifyEnabled = false/' \
    -e '0,/^android {/s//android {\n    lint { checkReleaseBuilds = false }/' "$app/app/build.gradle.kts"
(cd "$app" && ./gradlew -q assembleRelease)
key=~/.config/traced
out=$app/app/build/outputs/apk/release
"$ANDROID_HOME"/build-tools/36.0.0/apksigner sign --ks "$key/release.jks" --ks-pass "file:$key/release.pass" \
    --out "$out/traced.apk" "$out/app-release-unsigned.apk"
echo "$out/traced.apk"
