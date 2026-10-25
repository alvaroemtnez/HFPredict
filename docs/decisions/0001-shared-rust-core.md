# 0001. One Rust core, two thin native apps

Status: accepted (2026-10-25)

HFcast keeps most of its logic in TypeScript around the engine. HFPredict moves
all of it (requests, correction, survey, lattices, places, readings, state,
cache, and the map image itself) into `core/`, used directly by the GNOME app
and through UniFFI by Android. The apps only lay out widgets, handle gestures
and choose colours. Consequences: a number cannot differ between platforms; a
feature lands on both or neither; the map is the same picture everywhere; Kotlin
holds no forecasting logic.
