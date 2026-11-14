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
| Android | App Bundle (arm64-v8a, armeabi-v7a), signed with the release key as upload key, re-signed by Google Play, without the in-app updater | Google Play, on demand (closed testing is the `alpha` track) | testers of that track, or anyone once in production |
| iOS | App Store IPA, signed with the team's Apple Distribution certificate and App Store profile | the App Store (after review) and TestFlight internal testing, on demand | anyone, from the App Store; testers of the team in TestFlight |

The GitHub Release (and so Obtainium) gets every release. The stores only get the releases the
maintainer ships with the manual `store.yml` workflow (Actions > Store > Run workflow, from
`main`), since each one goes through Apple's and Google's review. `scripts/store-guard.sh` runs
in CI and fails if a store submission appears in a workflow, script or `Makefile` other than
`store.yml`, if `store.yml` gets a trigger other than `workflow_dispatch`, or if an iOS icon has
an alpha channel.

Bundle identifier and Android package: `io.github.basilelt.overseer`, the desktop identifier. The
Google Play build (`OVERSEER_DISTRIBUTION=play`) uses the package `io.github.basilelt.overseer.pro`:
Google Play rejects any upload whose package does not match the app, and a free Play app can never
become paid, so the paid Play app is a new one. The GitHub APK keeps `io.github.basilelt.overseer`,
so Obtainium keeps updating it.

## Install on Android with Obtainium

[Obtainium](https://github.com/ImranR98/Obtainium) installs and updates apps from GitHub releases.
The repository is public, so Obtainium needs no GitHub token.

1. Add App: on the phone, with Obtainium installed, open
   [Add Overseer to Obtainium](https://apps.obtainium.imranr.dev/redirect?r=obtainium://app/%7B%22id%22%3A%22io.github.basilelt.overseer%22%2C%22url%22%3A%22https%3A%2F%2Fgithub.com%2Fbasilelt%2Foverseer%22%2C%22author%22%3A%22basilelt%22%2C%22name%22%3A%22Overseer%22%2C%22preferredApkIndex%22%3A0%2C%22additionalSettings%22%3A%22%7B%5C%22includePrereleases%5C%22%3Afalse%2C%5C%22apkFilterRegEx%5C%22%3A%5C%22Overseer_.%2A_android%5C%5C%5C%5C.apk%5C%22%2C%5C%22invertAPKFilter%5C%22%3Afalse%7D%22%2C%22overrideSource%22%3A%22GitHub%22%7D). It opens Obtainium with every setting below prefilled;
   tap Add. Or add it by hand:
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
and installs the same signed APK; use one or the other. The Google Play build has no in-app
updater: Play updates it.

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
release, checks the signature of the release's `SHA256SUMS` (`SHA256SUMS.sig`, made with the
updater key) and the binary against that file, caches it in the app
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
keeps updating from GitHub; Play and the GitHub APK are signed with different keys, so switching
sources means uninstalling first (see
[Google Play](#google-play-once-by-the-play-console-account-holder)).

Testers on the App Store Connect team also get every store build in TestFlight (internal testing
group), usually 10 to 30 minutes after the store run, before App Store review ends.

Anyone else can join the public TestFlight beta with the
[invitation link](https://testflight.apple.com/join/DZgZFcMk) (external testing group). A new
version reaches them after Beta App Review; later builds of the same version usually need none.

## How the builds run

- Pull requests: the `android` CI job builds a debug APK for arm64 (`pnpm tauri android build
  --debug --apk --target aarch64`) so the mobile code compiles, and uploads it as the
  `android-debug-apk` artifact. `store-guard` checks that store submissions stay in the manual
  workflow. iOS is not built on pull requests.
- Releases (`release.yml`, push to `main`):
  - `app-android` on `ubuntu-24.04`: stamps the version (`scripts/set-version.sh`),
    builds `pnpm tauri android build --apk --target aarch64 --target armv7` with the release
    keystore, checks the signature (and the certificate fingerprint when `ANDROID_CERT_SHA256` is
    set) with `apksigner` (`scripts/android-apk.sh`), checks 16 KB page alignment
    (`scripts/android-16k.sh`), and hands the APK to `publish`. The release does not wait for it
    to succeed. It is skipped, with a warning on the run, while its secrets are missing (the
    `tag` job checks them). No App Bundle, no iOS build and no store upload run on a release.
- Store releases (`store.yml`, manual, from `main`, inputs `tag`, `release_notes`, `ios`, `android`, `track`; see
  docs/ci.md, "Store releases"):
  - `ios` on `macos-26`: checks out the tag, stamps its version, writes `APPLE_TEAM_ID` into the
    Xcode project (`gen/apple/project.yml` and `project.pbxproj` are committed with an empty
    team), then `fastlane ios app_store` from `app/` (`app/fastlane/Fastfile`). Tauri signs with
    `IOS_CERTIFICATE` and `IOS_MOBILE_PROVISION`; the lane builds with `pnpm tauri ios build --export-method
    app-store-connect`, checks the IPA's versions, and submits it with `upload_to_app_store`
    (`submit_for_review: true`, `automatic_release: false`, screenshots skipped). The only
    metadata it sends is the version's "What's New" (`en-US`), required for every update: the
    `release_notes` input, or "Bug fixes and improvements." when empty.
    `CFBundleShortVersionString` is the release version; `CFBundleVersion` is the `store.yml` run
    number (`<run>.<attempt>` on a re-run, so a re-run never reuses a build number).
  - `android` on `ubuntu-24.04`: checks out the tag, stamps its version and builds the Google
    Play App Bundle with `OVERSEER_DISTRIBUTION=play` (`pnpm tauri android build --aab --target
    aarch64 --target armv7` with the release keystore; the in-app updater is not compiled into
    it: no Rust module, no Kotlin plugin, no web view code, no `REQUEST_INSTALL_PACKAGES`). It
    checks the bundle's signature, manifest, dex and alignment (`scripts/android-aab.sh`,
    `scripts/android-16k.sh`), then uploads it with `r0adkll/upload-google-play` to the chosen
    track (`alpha`, closed testing, by default; or `production`, `beta`, `internal`), status
    `draft` until the app has passed its first review (the API refuses other statuses for an app
    that was never published; the draft is then sent for review from the Console), with the
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

### Google Play (once, by the Play Console account holder)

Needs a Google Play developer account and the Android release key above.

1. Create the app in the [Play Console](https://play.google.com/console) with the package
   `io.github.basilelt.overseer.pro`, fill in the store listing, the content rating, the data safety,
   target audience and app access forms, and the privacy policy URL
   (`https://github.com/basilelt/overseer/blob/main/PRIVACY.md`).
2. Play App Signing: Overseer's app signing key was generated by Google (certificate SHA-256
   `C9:D8:52:31:…:E2:AF`), so it differs from the release key that signs the GitHub APK
   (`ANDROID_CERT_SHA256`, `D2:24:F6:…:97:E1`). Google keeps that key and it cannot be switched
   back to the release key. The release key is the upload key: `ANDROID_KEYSTORE_*` sign the AAB
   and Play re-signs it.
3. First upload by hand: the Play Developer API refuses uploads for an app that has none. Done
   for the former free app `io.github.basilelt.overseer` (0.27.1, `versionCode` 27001); the
   `.pro` app needs it again. Build the AAB locally with `OVERSEER_DISTRIBUTION=play pnpm --dir app tauri
   android build --aab --target aarch64 --target armv7` and the release keystore
   (`ANDROID_KEYSTORE_PATH`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`,
   `ANDROID_KEY_PASSWORD`), and upload it to a testing track in the Console.
4. Service account: in a Google Cloud project, enable the Google Play Android Developer API,
   create a service account (no Cloud role) and a JSON key for it. Play Console > Users and
   permissions > Invite new users: the service account's e-mail, with access to Overseer and the
   release permissions (release to testing tracks, release to production, manage testing tracks).
5. Repository secret `PLAY_SERVICE_ACCOUNT_JSON`: the JSON key, as is. The variables
   `PLAY_TRACK` and `PLAY_RELEASE_STATUS` of the former automatic upload are no longer used and
   can be deleted.

Then run Actions > Store > Run workflow on `main` with `android` checked and the track to release
to (`alpha` for closed testing). A production release still goes through Google's review before
it reaches users.

Because the two builds are signed with different keys, Android refuses to update a Play install
with the GitHub APK and the other way round: a user picks one source, and switching means
uninstalling first. Play builds have no in-app updater; Play updates them.

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
   workflow uploads no screenshots and no metadata except the version's "What's New" text
   (`release_notes` input), so the listing stays as edited in App Store Connect; update it there before a submission when the app
   changed visibly.
4. TestFlight > Internal Testing > + : a group, with automatic distribution on so every processed
   build reaches it. Add the testers (they must be App Store Connect users of the team).
5. Export compliance: `ITSAppUsesNonExemptEncryption` is `false` in the iOS `Info.plist`
   (`gen/apple/overseer-app_iOS/Info.plist`; Overseer only uses standard encryption: SSH and
   HTTPS), so builds do not wait for the question. Answer the same way in App Store Connect if
   asked.
6. Create the Apple Distribution certificate and the App Store profile, from any OS, with an
   App Store Connect API key with the Admin role (kept off CI):

   ```sh
   openssl req -new -newkey rsa:2048 -nodes -keyout dist.key -out dist.csr -subj "/CN=Overseer iOS Distribution"
   ```

   Then, with a JWT for that key, `POST /v1/certificates` with `certificateType: DISTRIBUTION` and
   the CSR body (without the PEM armour), and `POST /v1/profiles` with
   `profileType: IOS_APP_STORE`, the bundle id `io.github.basilelt.overseer` and that
   certificate. Pack the certificate with its key and the `AppleWWDRCAG3` intermediate:
   `openssl pkcs12 -export -legacy -inkey dist.key -in dist.pem -certfile wwdrg3.pem -out dist.p12`.
   Both expire after one year: repeat this step and update the three `IOS_*` secrets before then.
   Keep `dist.key` and the `.p12` outside the repository, in a password manager.
7. Repository secrets:

   | Secret | Value |
   |---|---|
   | `APPLE_TEAM_ID` | the 10-character Apple Developer team id (Membership details) |
   | `ASC_KEY_ID` | App Store Connect API key id (Users and Access > Integrations; the App Manager or Admin role, needed to submit for review; separate from the Developer-role notarization key in `APPLE_KEY_ID`) |
   | `APPLE_ISSUER_ID` | issuer id of the account (shared by every key) |
   | `ASC_KEY_CONTENT` | that key's `.p8` file content |
   | `IOS_CERTIFICATE` | `base64 -w0 dist.p12` |
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

## Regenerating the projects

`.github/workflows/mobile-init.yml` (manual, Actions > Mobile init > Run workflow, choose the
branch) runs `pnpm tauri android init --ci` on `ubuntu-24.04` and `pnpm tauri ios init --ci`
on `macos-26` (without a development team: the release job sets it at build time), regenerates the mobile icons from `app/src-tauri/icons/icon.png`, and commits
`app/src-tauri/gen/android` and `app/src-tauri/gen/apple` to that branch as
`github-actions[bot]`. It does nothing for a platform whose directory exists unless "force" is
checked. After a forced Android run, re-apply the hand edits to `app/src-tauri/gen/android`:

- the release signing block and the SDK level (36, preinstalled on the hosted runner) in
  `app/build.gradle.kts`;
- `app/src/selfupdate/java/io/github/basilelt/overseer/ApkInstallerPlugin.kt` (the in-app installer,
  outside `src/main` so the Google Play build leaves it out; keep the file);
- `<uses-permission android:name="android.permission.REQUEST_INSTALL_PACKAGES" />` in
  `app/src/main/AndroidManifest.xml`, and the `OVERSEER_DISTRIBUTION` block in
  `app/build.gradle.kts` with `app/src/play/AndroidManifest.xml` (removes it for Google Play). The installer also relies on the generated `FileProvider`
  (`${applicationId}.fileprovider`) and its `cache-path` in `res/xml/file_paths.xml`.

The icons come from the 512x512 `icon.png`; the 1024x1024 App Store icon is upscaled. A
1024x1024 source would give a sharper icon.
