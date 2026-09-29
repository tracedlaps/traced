---
name: release
description: Build, sign and publish a new Traced APK to the public tracedlaps/tracedlaps repo and refresh tracedlaps.com.
disable-model-invocation: true
argument-hint: "[version, e.g. 0.3.0]"
---

Two repos: this private one (source, `git@github.com:tracedlaps/traced.git`) and the public
`tracedlaps/tracedlaps` (built site on `main` served by Pages at tracedlaps.com, APKs as GitHub
Releases). The site's download button links `releases/latest/download/traced.apk`, and the app's
updater (`packages/mobile/src/update.rs`) installs each release's `.apk` asset, so every release
attaches its APK under exactly that name. What's new lives in `packages/changelog/{en,pt}.md`: the
site shows `en.md`, and `publish-web.sh` publishes both to tracedlaps.com/changelog/, where the app
reads its language's before offering the update.

## Steps

1. **Preflight.** On `main`, working tree clean, level with `origin/main`. Done when all three hold;
   otherwise stop and ask.
2. **Version.** Use the argument, or propose one from `git log $(git describe --tags --abbrev=0)..HEAD`
   (semver pre-1.0: any `feat` → minor, only `fix` → patch) and confirm it with the user. Done when
   the user has agreed on `X.Y.Z`.
3. **Changelog.** From the commits since the previous tag, add a `## X.Y.Z - YYYY-MM-DD` entry
   (today's date) at the top of `packages/changelog/en.md` and the same entry, translated, in every
   other language file (European Portuguese in `pt.md`: "telemóvel", "ecrã", "pista"). Write for
   drivers, not developers: one `- ` line per user-visible `feat`/`fix` in plain words, internals
   left out, each change on a single line. Done when `cargo test -p changelog` passes and the user
   has approved the entry in every language.
4. **Bump.** Set `version` in `packages/mobile/Cargo.toml` and the `vX.Y.Z beta` label in the
   `Download` component of `packages/web/src/main.rs`. Run `cargo check -p mobile` to update
   `Cargo.lock`. Done when `grep -rn "<old version> beta" packages/web/src` is empty.
5. **Build.** `packages/mobile/build-android.sh --release --target aarch64-linux-android`; it prints
   the signed APK path. Done when both checks pass on that path:
   - `$ANDROID_HOME/build-tools/36.0.0/aapt2 dump badging <apk> | head -1` shows
     `versionName='X.Y.Z'` and `versionCode` = X·10000 + Y·100 + Z.
   - `$ANDROID_HOME/build-tools/36.0.0/apksigner verify --print-certs <apk>` shows
     `CN=Traced, O=tracedlabs` (the release key; `CN=Android Debug` means the wrong key).
6. **Commit and tag.** Commit the changelog and bump as `chore: release vX.Y.Z`, tag `vX.Y.Z`, push
   `main` and the tag to `origin`.
7. **Site.** `scripts/publish-web.sh`, before the release, so the app finds the notes as soon as it
   sees the release (it ignores changelog entries without one).
8. **Publish.** Extract the English entry as the release notes:
   `awk '/^## /{p=$2=="X.Y.Z"; next} p' packages/changelog/en.md > <notes>`, then
   `gh release create vX.Y.Z <apk>#traced.apk --repo tracedlaps/tracedlaps
   --title "Traced vX.Y.Z beta" --notes-file <notes>`. The asset must be named `traced.apk`.
9. **Verify.** Done when
   - `curl -sIL -o /dev/null -w '%{http_code}' https://github.com/tracedlaps/tracedlaps/releases/latest/download/traced.apk`
     prints `200`,
   - `gh release view --repo tracedlaps/tracedlaps` shows `vX.Y.Z` as latest, and
   - `curl -s https://tracedlaps.com/changelog/pt.md | grep -c '^## X.Y.Z'` prints `1` (Pages can
     take a minute or two to deploy; retry before calling it failed).

   Report the release URL.

## Signing key

`~/.config/traced/release.jks`, password in `release.pass` beside it. Every release must be
signed with it: a different key makes Android refuse the update, in-app or not, forcing users to
uninstall and lose their tracks. If the file is missing, stop — never generate a new one.

A phone running a debug build (`CN=Android Debug`) can't take a release APK over it. Before
uninstalling, save the data: `adb exec-out run-as com.tracedlaps.app cat files/traced.db > backup.db`.
