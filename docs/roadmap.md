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
- **Obtainium**: works today from the release APKs.
- **F-Droid**: recipe and store text ready (docs/releasing.md, "F-Droid");
  submit the merge request to fdroiddata once v0.1.0 is tagged.
- **Accrescent**: `android/build.sh accrescent` builds the signed APK set;
  register in the developer console and upload.

Linux:
- **Flathub**: the manifest must be written by hand (Flathub's AI policy;
  docs/releasing.md, "Flathub"); the metainfo is ready.
- **Forgejo / GitHub releases**: the Flatpak bundle and the AppImage, with the
  signed SHA256SUMS (done by the release workflow).

## Windows and macOS

Linux stays first; these are by-products of the same code.

- **Windows**: `packaging/windows/build.sh` cross-compiles in a Fedora
  container (MinGW GTK 4.21, libadwaita 1.7 built there) into
  `dist/HFPredict-<version>-windows-x86_64.zip`. The engine links files with
  Unix symlinks, so this build alone patches a private copy of it
  (`packaging/windows/hfcast-engine-windows.patch`: a copy where there are
  no Unix symlinks, identical on Unix); worth offering upstream, after which
  the patch goes. "Use my location" says it is unavailable (no portal).
  Builds; not yet run on a real Windows machine.
- **macOS**: the code should build as is (the engine is Unix-clean), but
  GTK and libadwaita apps need a Mac to build and bundle (Homebrew, then an
  .app made with gtk-mac-bundler or by hand) and, to open without warnings,
  an Apple Developer ID to sign and notarise. A GitHub Actions macOS runner on
  the mirror could do the build; untested.

## Later

- **Prefetch more**: while the app is open it readies every hour's coarse map
  and the current band's fine maps (Core::prefetch). Other bands, and tiles
  for the next hours, could follow if scrubbing still waits on a slow phone.
  A background service stays out (owner's call).
