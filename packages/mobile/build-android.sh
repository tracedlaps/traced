#!/usr/bin/env bash
# dx can't set the Android icon, app name or theme (DioxusLabs/dioxus#3685), so overlay android/res onto
# the gradle project it generates and repackage. Args go to `dx build --android`.
# --release also signs with ~/.config/traced/release.jks (password in release.pass beside it).
# --aab builds a signed release bundle for Google Play instead, with the `play` feature (no self-updater).
set -euo pipefail
cd "$(dirname "$0")"
args=() aab=
for a in "$@"; do [[ $a == --aab ]] && aab=1 || args+=("$a"); done
[[ $aab ]] && args+=(--release --features play)
start=$(mktemp)
dx build --android "${args[@]}"
[[ " ${args[*]} " == *" --release "* ]] && profile=release || profile=debug
app=$(realpath ../../target/dx/mobile/$profile/android/app)
# Gradle packs every ABI in jniLibs: drop libraries left by earlier builds for other targets
find "$app/app/src/main/jniLibs" -name '*.so' ! -newer "$start" -delete && rm "$start"
compgen -G "$app/app/src/main/jniLibs/*/*.so" >/dev/null || { echo "dx built no native library" >&2; exit 1; }
cp -r android/res/. "$app/app/src/main/res/"
if [[ $profile == debug ]]; then
    (cd "$app" && ./gradlew -q assembleDebug)
    echo "$app/app/build/outputs/apk/debug/app-debug.apk"
    exit
fi
# R8 would strip the plugin classes Rust looks up by name over JNI; release lint crashes on JDK 25
sed -i -e 's/isMinifyEnabled = true/isMinifyEnabled = false/' \
    -e '0,/^android {/s//android {\n    lint { checkReleaseBuilds = false }/' "$app/app/build.gradle.kts"
# versionCode must grow each release or Android won't treat it as an update: 0.2.0 -> 200
IFS=. read -r maj min pat < <(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
sed -i "s/versionCode = .*/versionCode = $((maj * 10000 + min * 100 + pat))/" "$app/app/build.gradle.kts"
# Gradle's strip step leaves libmain.so whole (36 MB): strip it here, keeping the symbols for Play's crash reports
syms=$app/app/build/outputs/native-debug-symbols.zip
rm -rf "$app/symbols" "$syms" && cp -r "$app/app/src/main/jniLibs" "$app/symbols"
"${ANDROID_NDK_HOME:-$NDK_HOME}"/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip --strip-unneeded "$app"/app/src/main/jniLibs/*/*.so
key=~/.config/traced
if [[ $aab ]]; then
    (cd "$app/symbols" && zip -qr "$syms" .) && echo "$syms"
    # Play rejects REQUEST_INSTALL_PACKAGES without an installer use case; dx regenerates this each build
    sed -i '/REQUEST_INSTALL_PACKAGES/d' "$app/plugins/storageplugin/src/main/AndroidManifest.xml"
    (cd "$app" && ./gradlew -q bundleRelease)
    out=$app/app/build/outputs/bundle/release
    jarsigner -keystore "$key/release.jks" -storepass:file "$key/release.pass" \
        -signedjar "$out/traced.aab" "$out/app-release.aab" traced >/dev/null
    echo "$out/traced.aab"
    exit
fi
(cd "$app" && ./gradlew -q assembleRelease)
out=$app/app/build/outputs/apk/release
"$ANDROID_HOME"/build-tools/36.0.0/apksigner sign --ks "$key/release.jks" --ks-pass "file:$key/release.pass" \
    --out "$out/traced.apk" "$out/app-release-unsigned.apk"
echo "$out/traced.apk"
