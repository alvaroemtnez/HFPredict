# 0003. No background "compute maps ahead"

Status: accepted (2026-10-25), owner's choice

HFcast can compute a year of maps in the background with a progress
notification, which needs a foreground service and POST_NOTIFICATIONS. With the
engine native (a whole-world fine map in about 0.1-0.5 s on a desktop, a second or
two on an old phone), HFPredict only caches maps computed while browsing (disk,
128 MB). No extra permissions. Revisit if old devices feel slow.
