# Roadmap

Open work only. Done items go into the commit history and CHANGELOG.md.

## Distribution

Release assets on Forgejo (mirrored to GitHub) are the base everything else
builds on.

Android:
- **Obtainium**: works today from the release APKs.
- **F-Droid**: from 25 January 2027 (three months of development). Submit
  with the first release signed by the fixed `android/build.sh`: F-Droid's
  build then verifies against ours, so it ships our signature. Recipe
  (with `Binaries:` and `AllowedAPKSigningKeys:`), store text, steps and
  reviewer notes: docs/releasing.md, "F-Droid".
- **Accrescent**: sign-up is closed (November 2026). The release attaches
  the signed APK set (`HFPredict-<version>.apks`), which meets the
  publishing requirements; when sign-up opens, register, verify the domain
  by DNS and upload (docs/releasing.md, "Accrescent").

Linux:
- **Flathub**: on hold for a few months of history and real-world use.
  Before submitting: `https://alvaroedo.io` must answer over HTTPS, the
  manifest and pull request are written by hand, and the build uses only
  the reviewed languages (docs/releasing.md, "Flathub").

## Windows and macOS

Linux stays first; these are by-products of the same code.

- **Windows**: `packaging/windows/build.sh` builds an installer (NSIS,
  per user), `dist/HFPredict-<version>-windows-<arch>-setup.exe`, two
  ways: natively in MSYS2 (x64 and ARM64; the release installers, which
  `.github/workflows/windows.yml` builds on the GitHub mirror), or x64
  cross-compiled in Fedora's local container for trying things (MinGW GTK
  4.21, a development release: its 0.1.0 installer crashed on Windows 11). The engine links files with Unix
  symlinks, so this build alone patches a private copy of it
  (`packaging/windows/hfcast-engine-windows.patch`: a copy where there are
  no Unix symlinks, identical on Unix); worth offering upstream, after which
  the patch goes. "Use my location" uses Windows' location service
  (`windows` crate), not yet tried on a real machine.
- **macOS**: `.github/workflows/macos.yml` on the GitHub mirror builds
  .dmg files for Apple silicon and Intel with `packaging/macos/build.sh`
  (Homebrew's GTK bundled into the .app, ad-hoc signed); untested on a real
  Mac, and so is "Use my location" there (CoreLocation, `objc2-core-location`). To open without warnings it would need an Apple Developer ID and notarising.
- The Forgejo release job fetches both from the mirror's runs
  (`tools/fetch-github-builds.sh`) and signs them in `SHA256SUMS`.

## Later

- **Prefetch more**: while the app is open it readies every hour's coarse map
  and the current band's fine maps (Core::prefetch). Other bands, and tiles
  for the next hours, could follow if scrubbing still waits on a slow phone.
  A background service stays out (owner's call).
- **Android widget**: brainstorm what is worth a glance on the home screen
  (the band open now to the destination, the best band this hour, the day's
  window, the live/offline readings), then implement it. Without
  Google: Glance or plain RemoteViews, refreshed by the system's own update
  schedule, no background service or new permission.
