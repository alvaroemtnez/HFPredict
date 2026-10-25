# Roadmap

Open work only. Done items go into the commit history and CHANGELOG.md.

## Before the first release

- **Release keys**: Android keystore and GPG key, Forgejo secrets
  (docs/releasing.md); then the fingerprints into docs/verifying.md.
- **Translations review**: es, ca are written natively; de, ja and ar start from
  HFcast's translations and need a native reader's pass.
- **Test on real devices**: a phone and a tablet (GrapheneOS), including the
  compass on real hardware (the emulator's sensor is virtual), and a Linux
  laptop through the Flatpak and the AppImage.

## Distribution

Release assets on Forgejo (mirrored to GitHub) are the base everything else
builds on.

Android:
- **Obtainium**: works today from the release APKs (per-ABI and universal);
  keep the asset names stable.
- **F-Droid**: metadata in fdroiddata (or our own repo first). Wants a build
  from source with no prebuilt binaries: the Rust core through cargo-ndk in
  their recipe, the engine's git dependency pinned by commit (it already is).
  Reproducible builds let F-Droid ship our signature.
- **Accrescent**: takes an APK set made by bundletool (1.11.4 or later) from a
  signed app bundle, 128 MiB at most, never debug-signed; a reviewer checks
  that every permission is needed. Needs `bundleRelease` plus a bundletool
  step in `android/build.sh release`, the release key, and a developer
  console account.

Linux:
- **Flathub**: flatpak-cargo-generator sources instead of the local vendor
  step; metainfo screenshots and release notes are in place.
- **Forgejo / GitHub releases**: the Flatpak bundle and the AppImage, with the
  signed SHA256SUMS (done by the release workflow).

## Later

- **Compute maps ahead** (HFcast's background job): left out on purpose (it
  needs a foreground service and the notification permission). Revisit if map
  computation feels slow on old devices; the cache covers browsing.
- **Device benchmark** (HFcast's "Measure this device"): only needed with the
  above.
- **Chance of contact at a tapped cell**: the probe outlines the cell and
  shows its quality band; a path forecast to that point (one engine run)
  would give the percentage, and a "set as destination" action.
- **Coastline smearing** near the antipode rim of the map.
