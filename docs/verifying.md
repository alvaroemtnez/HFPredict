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

On Windows and macOS, HFPredict makes both checks itself before it installs an
update, with this key built into the app.

**Release key fingerprint:** `4E5E D724 6337 53C0 8313  C543 5899 97F4 47B7 7166`
(ed25519, `HFPredict releases <support@alvaroedo.io>`, expires 2031-11-10)

## The APK signing certificate

Android checks the signature itself, and only installs an update signed like
the installed app. To check the certificate of a downloaded APK, with the
Android SDK's `apksigner`:

```sh
apksigner verify --print-certs HFPredict-X.Y.Z-arm64-v8a.apk | grep SHA-256
```

**Certificate SHA-256:**
`EA:8D:CF:D9:41:98:F4:E7:2A:86:70:56:79:FE:09:6C:D2:31:9C:6E:FA:BF:3C:9D:7D:50:F8:A5:27:8F:AD:59`

The same in the form F-Droid's `AllowedAPKSigningKeys` and AppVerifier take:
`ea8dcfd94198f4e72a86705679fe096cd2319c6efabf3c9d7d50f8a5278fad59`

Obtainium and AppVerifier can pin this certificate, so an APK signed by anyone
else is refused.
