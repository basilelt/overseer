# Mobile app: Android and iOS

The Tauri app also builds for Android and iOS (ADR 0032). The projects are generated into
`app/src-tauri/gen/android` and `app/src-tauri/gen/apple` and committed.

Both builds run on phones and tablets. The iOS target is universal (`TARGETED_DEVICE_FAMILY = "1,2"`,
iPhone and iPad, every iPad orientation), and the Android APK installs on tablets. The layout
follows the width: below 1024 px (a phone, a tablet in portrait, an iPad Split View) the app shows
the phone layout; from 1024 px (a tablet in landscape) it shows the desktop layout with touch. QA
covers each case with the `android`, `android-tablet`, `iphone`, `ipad` and `-landscape` modes of
`scripts/qa/drive.py` (`docs/qa-path.md`).

| Platform | Package | Where it goes | Who can install |
|---|---|---|---|
| Android | universal release APK (arm64-v8a, armeabi-v7a), signed with the Overseer release key | the GitHub Release of every release, `Overseer_<version>_android.apk` + `.sha256` | anyone: in-app updates, Obtainium or by hand |
| Android | AAB signed with the same key (the Play upload key) | the Play Store, on demand | anyone, from Google Play |
| iOS | App Store IPA, signed with the team's Apple Distribution certificate and App Store profile | the App Store (after review) and TestFlight internal testing, on demand | anyone, from the App Store; testers of the team in TestFlight |

The GitHub Release (and so Obtainium) gets every release. The stores only get the releases the
maintainer ships with the manual `store.yml` workflow (Actions > Store > Run workflow, from
`main`), since each one goes through Apple's and Google's review. `scripts/store-guard.sh` runs
in CI and fails if a store submission appears anywhere but `store.yml` and `app/fastlane/`, or if
`store.yml` gets a trigger other than `workflow_dispatch`.

Bundle identifier and Android package: `io.github.basilelt.overseer`, the desktop identifier.

## Install on Android with Obtainium

[Obtainium](https://github.com/ImranR98/Obtainium) installs and updates apps from GitHub releases.
The repository is public, so Obtainium needs no GitHub token.

1. Add App:
   - App source URL: `https://github.com/basilelt/overseer` (the source is detected as
     GitHub);
   - "Filter APKs by regular expression": `Overseer_.*_android\.apk`;
   - leave "Include prereleases" off.
2. Install. Android asks once to allow Obtainium to install unknown apps.
3. Updates: Obtainium checks the latest release and offers the new APK. Allow background updates
   in its settings if you want them applied without opening it.

Every release is signed with the same key, so Android accepts each APK as an update of the
previous one. A debug APK from a pull request (artifact `android-debug-apk`,
`overseer-debug-arm64.apk`, never attached to a release) is signed with a debug key:
uninstall it before installing a release APK, and the other way round.

## Updates on Android

Overseer updates itself from the GitHub releases (ADR 0032, addendum). Obtainium still works
and installs the same signed APK; use one or the other.

The repository is public, so checking for and downloading updates needs no GitHub token.

1. When a release is newer than the installed version, an "Update x.y.z" pill appears in the top
   bar. Tap it, read the notes, tap "Download & install". The APK is downloaded, checked against
   the release's `.sha256`, then:
2. The first time only, Android must allow installs from Overseer: the sheet explains it and
   opens the setting; turn on "Allow from this source" and come back. The installer opens on its
   own.
3. Confirm in the Android installer. Overseer restarts on the new version.

Checks run at start, every 6 hours while the app runs, when it comes back to the foreground (at
most every 15 minutes) and from the buttons. A debug APK from a pull request is signed with
another key: the app says so and the release APK must be installed by hand once after
uninstalling it.

## Installing the server from the phone

A host without the Overseer server (or with an older one) no longer needs the desktop app
first. When the phone connects, it maps `uname -sm` to the release asset
(`overseer-<version>-x86_64-unknown-linux-musl`, `-aarch64-unknown-linux-musl`,
`-x86_64-apple-darwin`, `-aarch64-apple-darwin`) of its own version, downloads it from the GitHub
release, checks it against the release's `SHA256SUMS`, caches it in the app
cache folder, uploads it over the SSH session and starts it. The Connect screen shows "Downloading
server for linux/x86_64", "Installing server", "Starting". A running older daemon is hot-upgraded
as from the desktop.

The download needs no token (public releases).

## Install on Android by hand

```sh
gh release download --repo basilelt/overseer --pattern '*android.apk*'
sha256sum -c Overseer_*_android.apk.sha256
adb install -r Overseer_*_android.apk
```

Or copy the APK to the phone and open it. The release `SHA256SUMS` lists the APK too.

The release workflow pins the signing certificate when the repository variable
`ANDROID_CERT_SHA256` is set; compare it with `apksigner verify --print-certs` on the downloaded
APK.

## Install from the stores

Install Overseer from the App Store or Google Play. Store versions lag behind the GitHub releases:
they are shipped by hand, a few releases at a time. A phone that installed the APK from GitHub
keeps updating from GitHub; switching sources needs the same signing key on both sides (see
[Android Play Store](#android-play-store-once)).

Testers on the App Store Connect team also get every store build in TestFlight (internal testing
group), usually 10 to 30 minutes after the store run, before App Store review ends.

## How the builds run

- Pull requests: the `android` CI job builds a debug APK for arm64 (`pnpm tauri android build
  --debug --apk --target aarch64`) so the mobile code compiles, and uploads it as the
  `android-debug-apk` artifact. `store-guard` checks that store submissions stay in the manual
  workflow. iOS is not built on pull requests.
- Releases (`release.yml`, push to `main`):
  - `app-android` on `ubuntu-24.04`: stamps the version (`scripts/set-version.sh`),
    builds `pnpm tauri android build --apk --target aarch64 --target armv7` with the release
    keystore, checks the signature (and the certificate fingerprint when `ANDROID_CERT_SHA256` is
    set) with `apksigner` (`scripts/android-apk.sh`), and hands the APK to `publish`. The release does not wait for it
    to succeed. It is skipped, with a warning on the run, while its secrets are missing (the
    `tag` job checks them).
- Store releases (`store.yml`, manual, from `main`, inputs `tag`, `ios`, `android`, `track`; see
  docs/ci.md, "Store releases"):
  - `ios` on `macos-26`: checks out the tag, stamps its version, writes `APPLE_TEAM_ID` into the
    Xcode project (`gen/apple/project.yml` and `project.pbxproj` are committed with an empty
    team), then `fastlane ios app_store` from `app/` (`app/fastlane/Fastfile`). Tauri signs with
    `IOS_CERTIFICATE` and `IOS_MOBILE_PROVISION`; the lane builds with `pnpm tauri ios build --export-method
    app-store-connect`, checks the IPA's versions, and submits it with `upload_to_app_store`
    (`submit_for_review: true`, `automatic_release: false`, metadata and screenshots skipped).
    `CFBundleShortVersionString` is the release version; `CFBundleVersion` is the `store.yml` run
    number (`<run>.<attempt>` on a re-run, so a re-run never reuses a build number).
  - `android` on `ubuntu-24.04`: checks out the tag, stamps its version, builds `pnpm tauri
    android build --aab` with the release keystore, then `fastlane android play_store` uploads
    the AAB to the chosen track (`internal`, `alpha`, `beta` or `production`) with the
    `PLAY_SERVICE_ACCOUNT_JSON` service account.
  - Both fail when their secrets are missing: they only run when asked.
- Versions: Android `versionName` is the release version and `versionCode` is
  `major * 1000000 + minor * 1000 + patch` (Tauri's default), so every release upgrades the
  previous one, on GitHub and on Play (which rejects a `versionCode` it has already seen).
- The mobile bundles carry no daemon binaries (`bundle.resources` is emptied in
  `app/src-tauri/tauri.android.conf.json` and `tauri.ios.conf.json`).

### Why one universal APK

One file per release is what Obtainium and `gh release download` handle without per-device
filters. arm64-v8a covers every current phone; armeabi-v7a covers older 32-bit-only devices.
x86 and x86_64 are left out: they are for emulators and a few Chromebooks, and each extra ABI is
another release build of the Rust crate on the runner. Emulators can run the arm64 debug APK on
Apple Silicon or use `pnpm tauri android dev`.

## One-time setup

Every secret below is optional for `release.yml`: while the Android set is incomplete its
release job is skipped with a warning and the rest of the release goes on (docs/ci.md,
"Secrets"). The store jobs need theirs.

### Android (once, by whoever holds the release key)

Create a release keystore and keep it outside the repository, with its passwords, in a password
manager:

```sh
keytool -genkeypair -v -storetype PKCS12 -keystore overseer-release.jks -alias overseer \
  -keyalg RSA -keysize 4096 -validity 10000
keytool -list -v -keystore overseer-release.jks -alias overseer | grep SHA256
```

Repository secrets and variable (Settings > Secrets and variables > Actions):

| Name | Kind | Value |
|---|---|---|
| `ANDROID_KEYSTORE_BASE64` | secret | `base64 -w0 overseer-release.jks` |
| `ANDROID_KEYSTORE_PASSWORD` | secret | the store password |
| `ANDROID_KEY_ALIAS` | secret | `overseer` |
| `ANDROID_KEY_PASSWORD` | secret | the key password (same as the store password for a PKCS12 keystore) |
| `ANDROID_CERT_SHA256` | variable | the `SHA256:` fingerprint printed above (optional: pins the key in `scripts/android-apk.sh`) |

A lost or changed key means users must uninstall the app before they can install a new release.
Rotating the key also means updating `ANDROID_CERT_SHA256`.

### iOS (once, by the Apple Developer account holder)

Needs a paid Apple Developer Program membership. Nothing in the repository names a team: the
team id comes from the `APPLE_TEAM_ID` secret at build time.

1. Register the bundle id: developer.apple.com > Certificates, Identifiers & Profiles >
   Identifiers > + > App IDs > App, description "Overseer", explicit bundle ID
   `io.github.basilelt.overseer`. No capability is needed.
2. Create the app record: App Store Connect > Apps > + > New App, platform iOS, name
   "Overseer" (App Store names are global: if it is taken, pick another; it does not need to
   match the app), primary language, bundle ID `io.github.basilelt.overseer`, SKU `overseer`.
3. Fill the App Store listing by hand, once: description, keywords, support URL, screenshots
   (iPhone and iPad), App Privacy answers, age rating, content rights and the price. The store
   workflow never uploads metadata or screenshots (`skip_metadata`, `skip_screenshots`), so the
   listing stays as edited in App Store Connect; update it there before a submission when the app
   changed visibly.
4. TestFlight > Internal Testing > + : a group, with automatic distribution on so every processed
   build reaches it. Add the testers (they must be App Store Connect users of the team).
5. Export compliance: `ITSAppUsesNonExemptEncryption` is `false` in the iOS `Info.plist`
   (Overseer only uses standard encryption: SSH and HTTPS), so builds do not wait for the
   question. Answer the same way in App Store Connect if asked.
6. Signing: an Apple Distribution certificate and an App Store provisioning profile for
   `io.github.basilelt.overseer` (developer.apple.com > Certificates and Profiles). Export the
   certificate with its private key from Keychain Access as a `.p12` with a password, and
   download the profile (`.mobileprovision`). The profile expires after a year: renew it there
   and update `IOS_MOBILE_PROVISION`.
7. Repository secrets:

   | Secret | Value |
   |---|---|
   | `APPLE_TEAM_ID` | the 10-character Apple Developer team id (Membership details) |
   | `APPLE_KEY_ID` | App Store Connect API key id (Users and Access > Integrations, role App Manager or Admin) |
   | `APPLE_ISSUER_ID` | issuer id of that key |
   | `APPLE_KEY_CONTENT` | the `.p8` file content |
   | `IOS_CERTIFICATE` | `base64 -w0` of the distribution `.p12` |
   | `IOS_CERTIFICATE_PASSWORD` | password of that `.p12` |
   | `IOS_MOBILE_PROVISION` | `base64 -w0` of the App Store `.mobileprovision` |

   `APPLE_ITC_TEAM_ID` (App Store Connect team id) is read by `app/fastlane/Appfile` when it is in
   the environment; it only matters for an Apple account in several App Store Connect teams, and
   the API key already selects the team.

Then run Actions > Store > Run workflow on `main` with `ios` checked. Only ship tags created
after store releases moved to `store.yml`: the build number restarts with that workflow's run
number, and App Store Connect refuses a build number it already has for the same version. Never
call the `app_store` lane, `upload_to_app_store` or `submit_for_review` from another workflow or
script: `store-guard` would fail the pull request.

### Android Play Store (once)

Needs a Google Play developer account and the Android release key above.

1. Play Console > Create app: name "Overseer", app, free; package `io.github.basilelt.overseer`
   is set by the first upload.
2. Play App Signing: choose to use your own key and upload the existing release key
   (`overseer-release.jks`, with Google's PEPK tool as the Console explains). Play then signs the
   store build with the same key as the GitHub APK, so a phone can move between Obtainium and
   Play without uninstalling. With a key generated by Google instead, the two builds have
   different signatures and Android refuses to update one with the other. Either way the release
   key is the upload key, so `ANDROID_KEYSTORE_*` sign the AAB.
3. Fill the store listing, content rating, data safety, target audience and app access forms.
4. Upload the first AAB by hand: the Play Developer API refuses uploads for an app that has none.
   Build it locally with the release key (`ANDROID_KEYSTORE_PATH`, `ANDROID_KEYSTORE_PASSWORD`,
   `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` set, then
   `pnpm --dir app tauri android build --aab`) and upload it to Testing > Internal testing.
5. Google Cloud: in a project, enable the Google Play Android Developer API, create a service
   account and a JSON key for it. Play Console > Users and permissions > Invite new users: the
   service account's email, with access to Overseer and the release permissions (release to
   testing tracks and to production).
6. Repository secret `PLAY_SERVICE_ACCOUNT_JSON`: the content of that JSON key.

Then run Actions > Store > Run workflow on `main` with `android` checked and the track to release
to. A production release still goes through Google's review before it reaches users.

## Regenerating the projects

`.github/workflows/mobile-init.yml` (manual, Actions > Mobile init > Run workflow, choose the
branch) runs `pnpm tauri android init --ci` on `ubuntu-24.04` and `pnpm tauri ios init --ci`
on `macos-26` (without a development team: the release job sets it at build time), regenerates the mobile icons from `app/src-tauri/icons/icon.png`, and commits
`app/src-tauri/gen/android` and `app/src-tauri/gen/apple` to that branch as
`github-actions[bot]`. It does nothing for a platform whose directory exists unless "force" is
checked. After a forced Android run, re-apply the hand edits to `app/src-tauri/gen/android`:

- the release signing block and the SDK level (36, preinstalled on the hosted runner) in
  `app/build.gradle.kts`;
- `app/src/main/java/io/github/basilelt/overseer/ApkInstallerPlugin.kt` (the in-app installer, keep the
  file);
- `<uses-permission android:name="android.permission.REQUEST_INSTALL_PACKAGES" />` in
  `app/src/main/AndroidManifest.xml`. The installer also relies on the generated `FileProvider`
  (`${applicationId}.fileprovider`) and its `cache-path` in `res/xml/file_paths.xml`.

The icons come from the 512x512 `icon.png`; the 1024x1024 App Store icon is upscaled. A
1024x1024 source would give a sharper icon.
