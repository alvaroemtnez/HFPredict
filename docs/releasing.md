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

### 3. Forgejo secrets

In the repository: *Settings → Actions → Secrets*. All are optional, and a
release still goes out without them (unsigned APKs, no `.asc`), but set them
before the first public release.

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

The Windows x64 zip, `HFPredict-X.Y.Z-windows-x86_64.zip`, is built on
Forgejo with the rest: the release workflow's `windows` job cross-compiles it
in Fedora with `packaging/windows/build.sh` (MinGW GTK, the same set-up as the
local container, `tools/containers/windows-deps.sh`).

Forgejo has no Mac runner, and nothing on Linux packages GTK for Windows on
ARM, so two workflows run on the GitHub mirror (Forgejo reads only
`.forgejo/workflows`). When the mirror pushes a `v*` tag:

- `.github/workflows/macos.yml` builds `HFPredict-X.Y.Z-macos-arm64.dmg`
  (Apple silicon) and `HFPredict-X.Y.Z-macos-x86_64.dmg` (Intel) with
  `packaging/macos/build.sh` (Homebrew's GTK, bundled into the .app).
- `.github/workflows/windows.yml` builds `HFPredict-X.Y.Z-windows-arm64.zip`
  natively on an ARM64 runner with `packaging/windows/build.sh` in MSYS2
  (CLANGARM64: MSYS2's GTK and libadwaita, Rust's
  `aarch64-pc-windows-gnullvm`).

Either can also be started by hand: *Actions → macos/windows → Run workflow*.
Actions must be enabled on the mirror (*Settings → Actions → General*);
public repositories get these runners for free.

The Forgejo release job collects them by itself
(`tools/fetch-github-builds.sh`): after its own builds it polls the mirror's
runs for the tag, every minute for up to two hours, downloads their artifacts
and puts the .dmg and the ARM64 .zip into the release before `SHA256SUMS` is
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
location" says it is unavailable, as on Windows.

## Each release

1. Bump `version` in the root `Cargo.toml` (the only place it lives: Android
   reads it from there, versionCode `X*10000 + Y*100 + Z`).
2. Add the `## X.Y.Z` entry to `CHANGELOG.md`, written for users. The release
   notes are this entry; the release fails without one.
3. Add a `<release version="X.Y.Z" date="YYYY-MM-DD">` to
   `gnome/data/io.alvaroedo.HFPredict.metainfo.xml`.
4. Refresh the sunspot table if a month or two has passed:
   `tools/update-ssn.py` (check the diff looks sane).
5. Run the checks: `gnome/build.sh test` and `android/build.sh test`.
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
   `HFPredict-X.Y.Z-{x86_64,aarch64}.AppImage`,
   `HFPredict-X.Y.Z-macos-{arm64,x86_64}.dmg`,
   `HFPredict-X.Y.Z-windows-{x86_64,arm64}.zip`,
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

- Store text, icon, screenshots and per-version changelogs:
  `fastlane/metadata/android/<locale>/` (en-US, es-ES, ca; the other
  languages fall back to English). Add `changelogs/<versionCode>.txt` with
  every release (500 characters at most).
- Build recipe: `packaging/fdroid/io.alvaroedo.HFPredict.yml`. F-Droid
  brings its own JDK, SDK and NDK; the recipe runs
  `HFP_EXTERNAL_SDK=1 android/build.sh rust` with them, then Gradle.
- Submit: fork https://gitlab.com/fdroid/fdroiddata, copy the recipe to
  `metadata/`, check it with `fdroid lint` and `fdroid build -v -l
  io.alvaroedo.HFPredict` (their docker image), open a merge request.
- After the first release, reproducible builds let F-Droid ship our
  signature instead of theirs: add `AllowedAPKSigningKeys` with the
  certificate's SHA-256 from docs/verifying.md.
- IzzyOnDroid, the other F-Droid repository, does not take apps written
  with generative AI.

### Accrescent

- `HFPREDICT_KEYSTORE=… android/build.sh accrescent` builds
  `android/dist/HFPredict-<version>.apks`: the app bundle split by bundletool
  (pinned, downloaded into `android/.tools/`) and signed with the release
  key (v2 and v3 signatures, one signer). Accrescent never takes a
  debug-signed app.
- Register in the developer console, then upload the `.apks`. The two
  permissions (internet, coarse location) are both explained in the manifest
  for the reviewer; location is one Accrescent reviews by hand.

### Flathub

- Flathub (October 2026) accepts AI-assisted apps with disclosure, but the
  Flathub manifest itself must not be AI-generated or AI-assisted, and the
  submission pull request, its description and the replies must be written
  by a person. So the Flathub manifest is yours to write: use
  `packaging/flatpak/io.alvaroedo.HFPredict.yml` only as a reference for what
  the app needs (GNOME 51 runtime, Rust SDK extension, network and display
  permissions), and list the cargo sources with `flatpak-cargo-generator`
  instead of the local vendor step.
- The metainfo (`gnome/data/io.alvaroedo.HFPredict.metainfo.xml`) with
  screenshots and release notes, the desktop file and the icons are ready.
