# Workflow audit — 2026-11-13

Scope: desktop shared catalog/analysis queue publication, podcast purge safeguards,
update staging, Android local imports, offline playlist downloads, lightweight DJ
playback, Smart Playlists and Android update recovery. This is a targeted workflow
audit, not a claim that every feature is defect-free.

Fixed in Android 0.2.12:

- Folder imports now read both the original catalog and the desktop v2 index with
  its referenced song snapshots. Previously v2 looked empty to the phone, losing
  prepared cues/BPM for folder imports.
- Metadata/catalog errors no longer erase a successful directory scan or prevent
  an otherwise playable song from entering the library. The importer reports the
  missing preparation and retains original files. A new scan clears old source
  preparation state so cues cannot leak from an earlier source.
- Loading a different DJ track resets the previous track's tempo adjustment.
  Auto DJ can then deliberately apply the next prepared tempo match.
- Releases without an android-arm64 package explain that no Android installer is
  available and retain the current app, instead of exposing a JSON lookup error.
- Removed an unused method that conflated saving a playback queue with replacing
  the complete local library.

Desktop finding investigated and closed without a change:
Song snapshot retirement appeared vulnerable to an old-index reader. Tracing the
public read entry points confirmed they already hold the same publication lock
as writers. No speculative retry layer was retained.
Podcast purge checks managed identity, unchanged size/time, pinning, active-file
protection and real-directory boundaries. Updater staging preserves validation,
checksum checks and rollback behavior. These existing safeguards were retained.

Checks:

- 335 desktop workspace tests passed; 57 opt-in/hardware tests remain ignored.
- Strict workspace Clippy passed with warnings denied.
- Android debug APK and instrumentation APK built successfully.
- Android lint completed successfully with existing warnings; this is not a claim
  of zero warnings or accessibility conformance.
- 23 Pixel 7 instrumentation tests passed: recursive and split catalog import,
  invalid metadata isolation, local references, actual MediaStore publication with
  local HTTP download fixtures, checksums/cues, network cancellation, updater
  recovery, real two-deck playback and Smart Playlist eligibility/order rules.
- Installed Android 0.2.12 over the existing app without clearing its data.

Remaining boundaries:

- Android DJ policy remains a separate Java implementation over the compatible
  preparation schema; desktop Rust policy is not yet one compiled shared module.
- Offline saves are retained snapshots, not incremental two-way synchronization.
  No automatic deletion of previous downloads or publishing offline edits was added.
- No AI cue coaching/training or physical Linux/controller/receiver validation was
  performed in this audit. Passing tests does not establish multi-day endurance.
- No public GitHub installer/APK release was published as part of this audit.
