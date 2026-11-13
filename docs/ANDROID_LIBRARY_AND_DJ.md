# Android playback and library boundaries (0.2.11)

Android includes a lightweight, service-owned DJ Engine: two main decks, cue decks,
samplers and crossfade execution. DjLogic contains prepared cue/entry policy,
repeat selection and conservative pitch-preserving tempo-ratio decisions. It
consumes the desktop preparation format. It is currently a Java mobile policy,
not the desktop Rust renderer compiled into Android. No BPM/key analyzer or stem
separator is installed on the phone. Missing preparation remains usable for basic
playback; a missing BPM does not invent a tempo match.

Library > Add music indexes selected songs or folders recursively using persistent
Android document permissions. Songs are referenced in place. Songs, Artists and
Albums organize the UI; removing a reference does not delete its original. Earlier
private imports are retained for compatibility, not deleted or silently relocated.
Unavailable grants require choosing the source again.

Library > Offline exposes downloaded station playlists. DJ > Choose playlist also
lists these downloads, with saved cues and preparation. On Android 10+ new audio
is published through MediaStore into Music/AMP Library/<playlist>, with readable,
sanitized names and collision handling by MediaStore. Only completed downloads
are published. Failed/cancelled operations remove their newly staged media and
keep previous completed downloads. The app's catalog retains playlist membership
and cue documents privately; the public Music folder contains audio, not loose
JSON sidecars. Android 9 retains the previous private offline download location.

Local references never enter the station sync/download operation. Reconnecting
never uploads phone edits to the host. Existing downloaded snapshots are retained;
this release does not add automatic resync deletion or two-way synchronization.

Storage implementation follows Android's app-owned media guidance:
https://developer.android.com/training/data-storage/shared/media

Verification: build, Android lint (existing warnings retained), eight targeted
Pixel 7 instrumentation tests, including a local HTTP download fixture, actual
MediaStore publication, checksum/cue retention, stalled-network cancellation,
local reference persistence, recursive discovery and two-deck Auto DJ progression.
