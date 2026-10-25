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
gpg --armor --export "HFPredict releases" > docs/hfpredict-release-key.asc     # public: commit this
gpg --armor --export-secret-keys "HFPredict releases" > hfpredict-release-key.secret.asc   # private: CI secret only
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
| `MIRROR_GITHUB_TOKEN` | a GitHub fine-grained token for `alvaroemtnez/HFPredict` with *Contents: read and write* |

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
7. The release workflow (about half an hour) publishes:
   `HFPredict-X.Y.Z-{universal,arm64-v8a,armeabi-v7a,x86_64}.apk`,
   `HFPredict-X.Y.Z-x86_64.flatpak`, `HFPredict-X.Y.Z-x86_64.AppImage`,
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

## Later: Flathub and F-Droid

- **Flathub** needs the manifest in a Flathub repository, with the cargo
  sources listed through `flatpak-cargo-generator` instead of the local vendor
  directory `packaging/flatpak/build.sh` uses. The engine's coefficient maps
  come from the pinned git checkout, which Flathub's builder fetches the same
  way.
- **F-Droid** builds from source. The script-based build and the pinned
  toolchain already fit; it needs a metadata file and the NDK version recorded.
