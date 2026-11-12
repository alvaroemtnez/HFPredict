# Releasing

A release is a tag. Pushing `vX.Y.Z` runs `.forgejo/workflows/release.yml`,
which builds everything, signs it and publishes the release on Forgejo and on
the GitHub mirror. This page covers the one-time set-up, then each release.

## One-time set-up

### 1. The Android signing key

Android only installs an update signed with the same key as the installed app,
so this key is permanent: lose it and every user has to uninstall and reinstall.
Make it once, keep two offline copies (for example an encrypted USB stick and
your password manager), and never commit it.

Use a key for HFPredict alone, not one shared with MeshConnect or Skypost: a
leaked key then affects one app, and the app can change hands, or move to a
store that holds keys, without taking the others with it.

Use the JDK the Android build already downloaded (`android/build.sh setup`
fetches it):

```sh
android/.tools/jdk-21/bin/keytool -genkeypair -v \
  -keystore hfpredict-release.jks -storetype PKCS12 \
  -alias hfpredict -keyalg RSA -keysize 4096 -validity 36500 \
  -dname "CN=Álvaro Edo Martínez, O=HFPredict"
```

It asks for a password (PKCS12 keystores use the same password for the store
and the key). Print the certificate fingerprint and keep it with the key:

```sh
android/.tools/jdk-21/bin/keytool -list -v -keystore hfpredict-release.jks -alias hfpredict | grep SHA256
```

That fingerprint goes into [verifying.md](verifying.md) (and later into
Obtainium/F-Droid metadata), so users can check an APK really is yours.

To sign locally, outside CI:

```sh
HFPREDICT_KEYSTORE=~/keys/hfpredict-release.jks HFPREDICT_KEY_ALIAS=hfpredict android/build.sh release
```

### 2. The GPG key for SHA256SUMS

Every release has a `SHA256SUMS` file covering all its files, and a detached
signature `SHA256SUMS.asc`. A dedicated signing key keeps your personal key off
the CI server:

```sh
gpg --quick-generate-key "HFPredict releases <releases@example.org>" ed25519 sign 5y
# Export by the fingerprint gpg printed: a name matches every key that has it.
gpg --armor --export <FINGERPRINT> > docs/hfpredict-release-key.asc     # public: commit this
gpg --armor --export-secret-keys <FINGERPRINT> > hfpredict-release-key.secret.asc   # private: CI secret only
```

Commit the public key (`docs/hfpredict-release-key.asc`) and put its
fingerprint in [verifying.md](verifying.md). Put whichever address you want public in
the key's name (the example's is a placeholder): it ships with every release.

The Windows and macOS builds carry this public key (`core/src/openpgp.rs`
reads it from `docs/hfpredict-release-key.asc` at build time) and install an
update only if its `SHA256SUMS` has a good signature by it. So:

- The app reads the key alone, an Ed25519 key that signs, with no subkeys
  (as the command above makes). A different kind of key, or signing with a
  subkey, would make every update fail its check.
- It ignores the key's expiry, so an installed copy keeps updating after
  2031-11-10. Extend the expiry anyway, for people checking by hand
  (`gpg --quick-set-expire <FINGERPRINT> 5y`, then export and commit the
  public key again): the key itself does not change.
- To replace the key, first release a version that carries the new public
  key, signed with the old one; installed copies reach it through the old
  key and trust the new one from then on. A copy that skipped that release
  has to be updated by hand.

### 3. Forgejo secrets

In the repository: *Settings → Actions → Secrets*. All are optional, and a
release still goes out without them (unsigned APKs, no `.asc`), but set them
before the first public release. Without the GPG key, Windows and macOS
never offer the release as an update (they require `SHA256SUMS.asc`).

| Secret | Value |
| --- | --- |
| `HFPREDICT_KEYSTORE_B64` | `base64 -w0 hfpredict-release.jks` |
| `HFPREDICT_KEYSTORE_PASS` | the keystore password |
| `HFPREDICT_KEY_ALIAS` | `hfpredict` |
| `HFPREDICT_KEY_PASS` | the key password (the same as the keystore's for PKCS12) |
| `GPG_PRIVATE_KEY` | the contents of `hfpredict-release-key.secret.asc` |
| `GPG_PASSPHRASE` | its passphrase |
| `RELEASE_TOKEN` | only if the automatic token cannot create releases: a Forgejo access token with *repository: read and write* |
| `MIRROR_GITHUB_TOKEN` | a GitHub fine-grained token for `alvaroemtnez/HFPredict` with *Contents: read and write* and *Actions: read* (the release fetches the macOS and Windows builds) |

Delete the local `.secret.asc` and base64 copies after pasting them.

### 4. The runner

The workflows use the `docker` label, as Skypost's do. The Flatpak job runs
Flathub's image with `--privileged`, because flatpak-builder sandboxes every
build step with bubblewrap. A Forgejo runner only honours that if its
configuration allows it, in `config.yml`:

```yaml
container:
  privileged: true   # or list the option under valid options for your runner version
```

With a docker-in-docker runner that is already privileged, nothing else is
needed. If the Flatpak job fails for want of privileges, the release still
publishes everything else; run `packaging/flatpak/build.sh` locally and attach
the bundle by hand.

### 5. The GitHub mirror

Create `alvaroemtnez/HFPredict` on GitHub and set up a push mirror from
Forgejo (*Settings → Repository → Mirror settings → Push mirror*, with a GitHub
token that can push). The release job creates the GitHub release itself, at the
tagged commit, so it does not wait for the mirror to sync. Obtainium (the
README's button) reads the GitHub releases.

### 6. ARM builds (arm64 Flatpak and AppImage)

The release workflow also builds for arm64 on the same x86 runner, under
QEMU emulation. Nothing to set up on the host: the workflow's `qemu` job, a
privileged Debian container, registers QEMU for arm64 in the host kernel's
`binfmt_misc` with the "F" flag (the interpreter is loaded at registration,
so containers without QEMU in them, and flatpak-builder's sandbox, can run
arm64 programs). It needs the same `privileged` permission as the Flatpak
job, lasts until the host reboots, and every release checks it again.

To check or undo it on the host:

```sh
cat /proc/sys/fs/binfmt_misc/qemu-aarch64        # "enabled", flags: FPC
echo -1 > /proc/sys/fs/binfmt_misc/qemu-aarch64  # remove it (as root)
```

Emulated builds are slow (expect one to two hours for both), so they run
only for releases; the jobs allow five hours. An AppImage's ELF header
carries a mark the QEMU rule does not accept, so the AppImage build zeroes it
in the linuxdeploy it downloads (`packaging/appimage/build.sh`).

### 7. Windows and macOS

Windows comes as an installer, `HFPredict-X.Y.Z-windows-<arch>-setup.exe`
(NSIS, `packaging/windows/installer.nsi`: per user, no administrator rights,
Start menu shortcut, uninstalled from *Settings → Apps*). There is no
portable build: GTK needs its DLLs, icons and settings schemas beside the
program, so it cannot be a single file.

Forgejo has no Mac or Windows runner, so two workflows run on the GitHub
mirror (Forgejo reads only `.forgejo/workflows`). When the mirror pushes a
`v*` tag:

- `.github/workflows/macos.yml` builds `HFPredict-X.Y.Z-macos-arm64.dmg`
  (Apple silicon) and `HFPredict-X.Y.Z-macos-x86_64.dmg` (Intel) with
  `packaging/macos/build.sh` (Homebrew's GTK, bundled into the .app).
- `.github/workflows/windows.yml` builds the x64 and ARM64 installers
  natively, on an x64 and an ARM64 runner, with `packaging/windows/build.sh`
  in MSYS2 (CLANG64 / CLANGARM64: MSYS2's GTK and libadwaita, Rust's
  `*-pc-windows-gnullvm`).

The local cross-build (`packaging/windows/build.sh` on Linux, Fedora's MinGW
GTK) is not used for releases: Fedora ships a development GTK there, and
the 0.1.0 x64 installer built that way crashed at start-up on Windows 11.

Either can also be started by hand: *Actions → macos/windows → Run workflow*.

Both builds update themselves from the GitHub mirror's latest release
(`core/src/update.rs`), and the AppImage's updaters read it too, so the job
creates that release as a draft and publishes it only once every file is
attached. Keep the asset names stable (`HFPredict-X.Y.Z-windows-<arch>-setup.exe`,
`HFPredict-X.Y.Z-macos-<arch>.dmg`, `HFPredict-X.Y.Z-<arch>.AppImage.zsync`,
`SHA256SUMS`): an installed copy finds its update by them. A release
missing a platform's file (say the ARM64 installer, attached by hand later)
is simply not offered there until it is up. The Windows installer runs as
`setup.exe /S /RELAUNCH` for an update: it waits for the app to quit,
installs and reopens it.
Actions must be enabled on the mirror (*Settings → Actions → General*);
public repositories get these runners for free.

The Forgejo release job collects them by itself
(`tools/fetch-github-builds.sh`): after its own builds it polls the mirror's
runs for the tag, every minute for up to two hours, downloads their artifacts
and puts the .dmg files and the ARM64 installer into the release before `SHA256SUMS` is
made and signed. It needs *Actions: read* on `MIRROR_GITHUB_TOKEN`. A run
that failed, is missing or is still going at the deadline is skipped with a
warning and the release goes out without it; then, by hand, download the
files from the run's page, attach them to both releases, add their
`sha256sum` lines to `SHA256SUMS`, sign it again with
`gpg --armor --detach-sign SHA256SUMS` and replace both files.

The app has only an ad-hoc signature (no Apple Developer ID, no
notarisation), so the first time, macOS asks: right-click the app, *Open*,
then *Open* again (or *System Settings → Privacy & Security → Open Anyway*).
It needs the macOS version the runner had (macOS 15) or newer, because
Homebrew builds its libraries for the system they are installed on. "Use my
location" goes through CoreLocation; with only an ad-hoc signature, macOS
may ask for the permission again after an update.

## Each release

1. Bump `version` in the root `Cargo.toml` (the only place it lives: Android
   reads it from there, versionCode `X*10000 + Y*100 + Z`).
2. Add the `## X.Y.Z` entry to `CHANGELOG.md`, written for users. The release
   notes are this entry; the release fails without one.
3. Add a `<release version="X.Y.Z" date="YYYY-MM-DD">` to
   `desktop/data/io.alvaroedo.HFPredict.metainfo.xml`.
4. Refresh the sunspot table if a month or two has passed:
   `tools/update-ssn.py` (check the diff looks sane).
5. Run the checks: `desktop/build.sh test` and `android/build.sh test`.
6. Commit, tag and push:
   ```sh
   git commit -am "Release X.Y.Z"
   git tag -a vX.Y.Z -m "HFPredict X.Y.Z"
   git push && git push --tags
   ```
7. The release workflow (two to three hours, most of it the emulated arm64
   builds) publishes:
   `HFPredict-X.Y.Z-{universal,arm64-v8a,armeabi-v7a,x86_64}.apk`,
   `HFPredict-X.Y.Z-{x86_64,aarch64}.flatpak`,
   `HFPredict-X.Y.Z-{x86_64,aarch64}.AppImage` (each with its `.zsync`),
   `HFPredict-X.Y.Z-macos-{arm64,x86_64}.dmg`,
   `HFPredict-X.Y.Z-windows-{x86_64,arm64}-setup.exe`,
   `SHA256SUMS` and `SHA256SUMS.asc`, on Forgejo and GitHub.

Don't create the release by hand as well: the job fails if it already exists.

## Building a release by hand

Everything the workflow does runs locally too:

```sh
android/build.sh release          # android/dist/ (signed if HFPREDICT_KEYSTORE is set)
packaging/flatpak/build.sh        # dist/
packaging/appimage/build.sh       # dist/
(cd dist && cp ../android/dist/*.apk . && sha256sum * > SHA256SUMS && gpg --armor --detach-sign SHA256SUMS)
```

## App stores

Everything the stores read from the repository is in place; what is left
is accounts and submissions, which are yours to do. Each store has its own
policy on AI-written code, and this app was written with an AI assistant:
read the current policy before submitting and disclose it where asked.

### Obtainium

Nothing to do: it reads the release APKs from Forgejo or the GitHub mirror.
Keep the asset names (`HFPredict-<version>-<abi>.apk`) stable.

### F-Droid

Not before 25 January 2027: F-Droid's inclusion policy wants three months of
active development (Quality, point 6), and the first commit is from 25
October 2026. F-Droid has no generative-AI policy (November 2026); whether
to mention the AI assistant in the merge request is the author's call.

- Store text, icon, screenshots and per-version changelogs:
  `fastlane/metadata/android/<locale>/` (en-US, es-ES, ca; the other
  languages fall back to English). Add `changelogs/<versionCode>.txt` with
  every release (500 characters at most).
- Build recipe: `packaging/fdroid/io.alvaroedo.HFPredict.yml`, in the form
  `fdroid rewritemeta` writes (it drops comments, so the notes are here).
  F-Droid brings its own JDK, SDK and NDK (r30 is in its list); the recipe
  installs a C toolchain (`build-essential`: Rust builds `cargo-ndk` and the
  crates' build scripts for the build machine), runs
  `HFP_EXTERNAL_SDK=1 android/build.sh rust` with F-Droid's NDK, then
  Gradle, and takes the universal APK.
- Reproducible builds: F-Droid builds the release, copies the signature of
  our APK (`Binaries:`) onto its own build and checks it verifies with the
  certificate in `AllowedAPKSigningKeys:` (docs/verifying.md); if so it
  ships our signed APK, so F-Droid and Forgejo/Obtainium installs update each
  other. This only works if signing changes nothing else in the APK, so
  `android/build.sh` signs Gradle's output as it is
  (`apksigner sign --alignment-preserved true`, no zipalign). Releases up to
  0.3.1 were zipaligned and cannot be verified: submit with a later one.
  Checked in November 2026: F-Droid's build of v0.2.1 contained the same
  files as ours, and our build of that tag, signed this way, passed
  F-Droid's verification. It cannot be switched on after inclusion (the
  APKs would already carry F-Droid's signature), so it has to work in the
  submission.
- Submit (in the fork of https://gitlab.com/fdroid/fdroiddata):
  1. copy the recipe to `metadata/io.alvaroedo.HFPredict.yml`, with
     `commit:` set to the full hash of the release's tag
     (`git rev-parse vX.Y.Z^{commit}`; the merge request template refuses
     tags; F-Droid's auto-update writes hashes itself afterwards);
  2. in F-Droid's container (`registry.gitlab.com/fdroid/fdroidserver:buildserver`,
     as in their Quick Start Guide; with podman add `--userns=keep-id`), run
     `fdroid readmeta`, `fdroid rewritemeta io.alvaroedo.HFPredict`,
     `fdroid lint io.alvaroedo.HFPredict` and
     `fdroid build --verbose --on-server --no-tarball io.alvaroedo.HFPredict:<versionCode>`,
     which must end in a verified build;
  3. commit as "New App: io.alvaroedo.HFPredict", push, and open the merge
     request titled "New app: HFPredict" from the template.
- Expect the reviewer to ask about:
  - the Rust toolchain: `tools/rust.sh` installs rustup and the toolchain
    pinned in `rust-toolchain.toml` into `.tools/`, and cargo builds
    `cargo-ndk` and every crate from source (`Cargo.lock`, `--locked`);
  - the engine: a git dependency pinned by commit in `Cargo.toml`, fetched
    from `code.alvaroedo.io/alvaroemtnez/hfcast-engine` (Apache-2.0 upstream,
    https://github.com/jonathanmcsweet/hfcast). That mirror must stay public.
    The template prefers external code as git submodules (their scanner
    covers those), so this may come up;
  - JNA from Maven Central, whose AAR carries a prebuilt `libjnidispatch.so`
    (LGPL-2.1 / Apache-2.0, built by the JNA project). UniFFI's Kotlin
    bindings need it;
  - network use: NOAA SWPC (space weather), GIRO/LGDC FastChar (ionosonde
    readings) and Open-Meteo geocoding (place search). Optional, switchable
    off, and none track the user; the forecasts are computed on the phone.
    No update check on Android. A reviewer might still consider the
    NonFreeNet anti-feature.
- IzzyOnDroid, the other F-Droid repository, does not take apps written
  with generative AI.

### Accrescent

- `HFPREDICT_KEYSTORE=… android/build.sh accrescent` builds
  `android/dist/HFPredict-<version>.apks`: the app bundle split by bundletool
  (pinned, downloaded into `android/.tools/`) and signed with the release
  key (v2 and v3 signatures, one signer). Accrescent never takes a
  debug-signed app. The release workflow builds it too (when the keystore
  secret is set) and attaches it to the release.
- Sign-up is closed (November 2026: only allowlisted GitHub accounts, no new
  requests). When it opens: register, upload the `.apks` and a 512 x 512
  PNG icon (`fastlane/metadata/android/en-US/images/icon.png`), and prove
  the domain of the app ID with a DNS `TXT` record at
  `_accverify.alvaroedo.io` holding the code the reviewer emails.
- Checked against Accrescent's publishing requirements (November 2026,
  https://accrescent.app/docs/guide/appendix/requirements/), on the 0.2.1
  `.apks`: one signer, v2 and v3 signatures, not a debug certificate, target
  SDK 37 (Accrescent follows Google Play's target SDK rules with no
  extensions: API 36 for new apps and updates from 31 August 2026), not
  debuggable, no self-updater on Android, no root or ADB use. Every 32-bit
  ABI ships with its 64-bit one. After 0.3.1 the bundle carries no x86
  split: only JNA's and AndroidX's libraries were in it, not the Rust core,
  so `android/app/build.gradle.kts` excludes the ABIs the core is not built
  for. Accrescent has no generative-AI policy.
- The two permissions (internet, coarse location) are both explained in the
  manifest for the reviewer; coarse location is on Accrescent's list of
  permissions reviewed by hand.

### Flathub

On hold until the project has a few months of history (decided November
2026). Read Flathub's requirements again before submitting: the AI policy
changed three times in 2026. As of November 2026
(https://docs.flathub.org/docs/for-app-authors/requirements):

- **Generative AI policy.** Disclose the AI-generated material in the app
  and its packaging, naming the parts and the approximate extent (the pull
  request template has a field for it). For HFPredict that is nearly all of
  it: code, documentation, translations and packaging scripts were written
  with an AI assistant. The disclosure is judged at the reviewers'
  discretion and does not guarantee acceptance.
- **The Flathub manifest must not contain AI-generated or AI-assisted
  content**, and disclosure does not exempt it.
- **The pull request, its commit messages, description and replies** must
  be written by a person, without AI tools or agents, and AI-agent reviews
  must not be requested.
- **Translations.** Flathub refuses machine-generated translations. English,
  Spanish and Valencian were checked by the author; German, Japanese and
  Arabic were generated and not reviewed. Build the desktop app with the
  environment variable `HFPREDICT_LANGUAGES=en,es,ca` (`desktop/src/i18n.rs`):
  it offers only those languages, and falls back to English for the rest.
  Add a language to the list once someone who speaks it has reviewed it.
  The desktop file carries only reviewed translations.
- **Development history.** Flathub generally refuses apps that have existed
  only briefly, and asks for evidence of real-world use and maintenance
  (the first commit is from 25 October 2026).
- **Domain.** The app ID `io.alvaroedo.HFPredict` makes Flathub check
  `https://alvaroedo.io`, which must answer over HTTPS. In November 2026
  the bare domain had no DNS record (only `code.alvaroedo.io` and others
  did). Flathub may also ask for a token at
  `https://alvaroedo.io/.well-known/org.flathub.VerifiedApps.txt`.

What the app repository has ready: the metainfo
(`desktop/data/io.alvaroedo.HFPredict.metainfo.xml`, screenshots and release
notes), the desktop file and the icons. The metainfo passes
`flatpak-builder-lint appstream` (from `org.flatpak.Builder`) and the desktop
file `desktop-file-validate`. Linting a local build directory also reports
`appstream-external-screenshot-url`; Flathub's own build mirrors the
screenshots, so that one only shows locally. The self-updater does not
exist in Linux builds, so the Flatpak never checks GitHub for updates.
