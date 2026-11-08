# Milestones and acceptance gates

1. **0.1 implemented foundation:** build Player/Core/Cast separately; real local audio via FFplay; folder tree and relative-path playlists; prepared cues; independent Cast MP3 broadcast; optional password; unit/API tests; real encoder/network smoke test; Windows/Linux CI and tag-gated prereleases. Verification results belong in `VERIFICATION.md`.
2. **Desktop reliability:** actual format/tag/duration metadata, persist settings/playlists atomically, cue editor, seek/pause, background rescans, multi-root/NAS availability, device selection and an in-process audio engine. Require audio fixtures, device-error recovery and manual listening on Windows/Linux.
3. **DJ preparation:** cue collections/beatgrids, two-deck mixer and crossfade, measured gapless Auto DJ, prepared stem playback. Require deterministic rendered-audio tests and listening tests; then expand deck count/FX/MIDI/OSC/Companion.
4. **Cast operations:** persisted station playlists, soundcard monitor/device routing independent of browser listeners, schedules, better diagnostics, graceful service installation and controller discovery. Test concurrent listeners, restart and decoder failure, memory bounds and overnight endurance. Later evaluate optional Cast dual-path redundancy; no bonding product.
5. **Integrated Android:** first preview code includes compact local playback, a foreground media service/session, SAF file imports copied to local storage, saved queue and three themes. Real-device verification is required before claiming background reliability. Next add folder-tree library import and prepared metadata. Local manual cues override server cues. Then simplified local Auto DJ, LAN discovery and password remote control. Require real-device screen-off/background endurance and offline tests before claiming support.
6. **Optional AMP Lab and updates:** manual verified release download/launch first, versioned project handoff and compatible add-on manifest, then explicit checksum-verified updates. Preserve independently distributed MP3 Lab and all licenses. Never silently execute downloads.

Future work is intentionally listed as acceptance criteria rather than disabled controls that suggest it already works.

Android handoff preference: when an AMP Verse APK is built for the user, install it on the Pixel and leave it installed. Do not uninstall it as test cleanup.


Android source-flow correction: build desktop library/playlist read and streaming endpoints, then Android Remote browse/listen-on-phone, then explicit Save offline into the radio collection. Phone-folder imports are not that radio-download collection. Direct phone-folder playback remains as a separate local-player option; retain existing imports.


2026-11-08 update: first desktop key/tempo/cue analysis, cue collection editor, batch metadata writing and portable versioned sidecars are now implemented. Broader real-song accuracy validation, Android sidecar import/local overrides and network metadata transfer remain outstanding.
