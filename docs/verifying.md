# Verifying a download

Every release has `SHA256SUMS`, listing a checksum for each file, and
`SHA256SUMS.asc`, a signature over that list made with the HFPredict release
key.

```sh
# Once: import the release key (docs/hfpredict-release-key.asc in the repository).
gpg --import hfpredict-release-key.asc

# For each download, in the folder holding the files:
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum --check --ignore-missing SHA256SUMS
```

The first command must report a good signature from the release key below, the
second `OK` for every file you downloaded.

**Release key fingerprint:** *(added with the first signed release)*

## The APK signing certificate

Android checks the signature itself, and only installs an update signed like
the installed app. To check the certificate of a downloaded APK, with the
Android SDK's `apksigner`:

```sh
apksigner verify --print-certs HFPredict-X.Y.Z-arm64-v8a.apk | grep SHA-256
```

**Certificate SHA-256:** *(added with the first signed release)*

Obtainium and AppVerifier can pin this certificate, so an APK signed by anyone
else is refused.
