# Changelog

Each release's entry becomes its release notes (tools/release-notes.sh), so
write it for the people installing the app.

## 0.3.0

- Windows and macOS: HFPredict checks for a new version at start and
  offers it. One click downloads it, checks it against the release's
  checksums and their signature by the HFPredict release key, installs it
  and opens the app again. "Don't remind me"
  silences that version; *Main menu → Check for updates* is always there.
  The check only runs with the network on in Preferences.
- AppImage: carries update information, so Gear Lever and AppImageUpdate
  find new versions by themselves.
- Desktop: on the map, the details of a tapped grid square wrap to stay
  inside a narrow map instead of being cut off.

## 0.2.1

- Desktop: at its narrowest the window no longer cuts off the close button
  and the right edge of the cards, in any language.
- Desktop: with the cards stacked in a wide, short window, the map keeps to
  part of the window's height instead of filling it.
- Android: nothing changes.

## 0.2.0

- Desktop: the map fills its card. Beside the bands it is a wide rectangle,
  so zooming in uses the whole width; with the cards stacked (a narrow
  window) it is a square as wide as the window.
- Windows and macOS: "Use my location" works, through the system's own
  location service. On Windows, turn on Settings → Privacy & security →
  Location, including access for desktop apps; on macOS, allow HFPredict
  when asked.
- Android: nothing changes.

## 0.1.1

- Android: changing the home-screen icon no longer closes the app.
- Android: the release also carries the APK set for the Accrescent store.
- macOS: the app opens (0.1.0 bounced in the Dock and never showed a window).
- Windows x64: the app opens (0.1.0 closed at once on Windows 11, before showing a window). The installer is now built natively on Windows with a stable GTK.

## 0.1.0

First release: HFPredict for Android and for Linux (Flatpak and AppImage).

- Forecasts from the HFcast engine on your own device: Truecast (the default) or VOACAP, with no network needed.
- Coverage map of where each band reaches, hour by hour, with near-vertical incidence marked; full screen, and tap a spot for its locator, distance, bearing and quality.
- On Android, point your antenna with the phone's compass: at the destination (short or long path), at a spot on the map, or to read a beam's direction into your station.
- Every band at every hour, to one place or as the share of directions reached, and the usable window.
- Your station: mode, power, antenna, height and direction; several stations can be saved.
- Live solar readings from NOAA and the nearest ionosonde from GIRO when online; place search beyond the built-in list through Open-Meteo. All of it can be switched off.
- Local time at your station is the legal time there, summer time included, worked out on the device.
- On Android, pick the home-screen icon from six colourways.
- Light, dark and low-light (red) themes following the system's colours; English, Spanish, Valencian, German, Japanese and Arabic.
