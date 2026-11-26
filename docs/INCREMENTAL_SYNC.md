# Incremental playlist sync and shared DJ decisions

Desktop 1.0.10 / Android 0.2.13.

## Everyday use

Connect to an AMP Cast host, choose a shared playlist and use Save offline with
my cues again to update it. The app fetches a fresh playlist manifest first.
The result reports how many unique audio files were downloaded and reused.
Library > Offline and DJ > Choose playlist expose the saved playlist.

Only AMP-managed playlist downloads are synchronized. Phone-folder music stays
in place and is never uploaded, removed or replaced by this operation. Reconnecting
does not start a sync or publish phone edits to the host.

## Identity, audio and publication

Server metadata includes a stable library-relative song key, content SHA-256 and
metadata revision. Server/station/library IDs isolate independent collections.
The phone updates one existing playlist catalog instead of making a new offline
playlist for each save. Legacy downloads are matched conservatively by verified
content or unambiguous legacy identity; their media IDs are retained.

Unchanged audio is verified locally and reused; only new, changed, missing or
corrupt files are downloaded. Metadata-only changes do not redownload audio. A library reindex between manifest
and metadata requests is rejected instead of downloading the wrong song. Changed
source files can sync before reanalysis; stale automatic preparation is flagged
and excluded from phone mixing decisions while manual cues remain intact.
The shared host still verifies each song's metadata and checksum; incremental
sync reduces audio transfers, not all disk reads. Do not advertise zero I/O.

Staging and checksum verification finish before playlist publication. On failure
or Cancel, newly staged files are removed and the previous completed playlist is
retained. Current playback owns its loaded MediaItem and prepared snapshot, so a
sync does not seek or replace a playing deck. Changed originals are not deleted.
Android 10+ downloads remain under Music/AMP Library/<playlist>. Android 9 keeps
the earlier app-private location. Only the arm64 Android build is currently shipped.

An existing playlist can sync down to zero members without deleting its saved
audio or settings. A first-time empty download still reports that no songs exist.
Removed playlist members disappear from the active list but their audio, stable
IDs and preparation are retained for re-addition. This intentionally uses storage;
automatic deletion of completed audio is not part of sync. Interrupted-process
publication can leave an unreferenced completed audio file; it cannot silently
replace the previous playlist. There is no automatic orphan-file purge.

## Cues, settings and conflicts

The last received remote preparation/rules are retained as the merge baseline.
Independent changes merge by field. Phone edits win conflicting changes, with the
remote proposal retained alongside them. Embedded preparation, stable hot-cue IDs,
local BPM/key/rating/energy preferences and DJ preferences are not reset by sync.

Inherited desktop cue slots are distinguished from explicit phone overrides;
legacy slots are compared against their old baseline before migration. Conflicting
hot cues can be reviewed from the sync result or by holding the offline playlist.
Keep phone versions acknowledges the proposals without discarding the edits.
Use desktop hot cues explicitly removes the conflicting local slot overrides;
the new cue values apply on the next preparation/reload. Other settings conflicts
remain reviewable. Sync never writes these phone edits back to the host.

## Shared logic packaging

crates/amp-dj-policy is dependency-free decision code, without audio decoding,
analysis, stems, catalog access or network code. Desktop/server use its tempo
match, half/double-time distance, harmonic/energy pair scoring, beat boundary and
fade-limit functions. Android calls the same compiled code through primitive-only
JNI methods. Source eligibility, platform UI and advanced desktop performance
orchestration remain outside the shared crate; this is not full feature parity.

Android preBuild runs tools/build_android_dj.py and packages its generated JNI
library. Install the aarch64-linux-android Rust target and NDK 27.0.12077973;
ANDROID_NDK_HOME can select another compatible NDK. Set ANDROID_HOME and optionally
AMP_PYTHON/CARGO_HOME/RUSTUP_HOME for the local toolchain. Generated .so files stay
in app/build, not source control. The library uses 16 KB page alignment.

## Verification

Rust unit tests cover matching limits, half-time interpretation, boundaries,
fade limits and prepared-neighbor scoring. Android exercises the actual packaged
JNI library against the same numeric fixtures. Local HTTP fixtures verify unchanged
resync, metadata-only edits, server index reordering, new/changed/corrupt audio,
failed transfer rollback, cancellation, membership removal/re-addition and cue/rule
conflicts. These checks do not imply a multi-day network endurance test or physical
Linux/Google Cast receiver testing.

Final local checks: 338 Rust workspace tests passed, two opt-in local downloader
fixtures passed, and strict workspace Clippy passed. The changed-audio Cast
endpoint also passed its targeted test suite after the final metadata revision
change. Android build/lint completed and 27 Pixel 7 instrumentation tests passed.
Windows 1.0.10 Preview and Android 0.2.13 were installed locally. Versioned artifacts
and SHA-256 lists are staged under test-output/releases (not source control).
No public GitHub binary release was created by this change.
