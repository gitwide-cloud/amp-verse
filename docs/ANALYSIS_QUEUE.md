# Durable analysis and bounded song catalogs (1.0.9)

The library has no fixed song or catalog-count ceiling. Each active song catalog
contains at most 500 track documents. `ampdj/catalog.ampdb` is the small lookup
index and shared analysis queue; `ampdj/catalogs/songs-*.ampdb` hold song metadata.
Copy the whole ampdj directory with the music to transport the library.

Library, headless Cast analysis, and ordinary amp-prepare use the same queue.
Discovery walks directories/subdirectories without rereading every song's tags or
preparation. File size and modification time identify new/changed songs. New work
appends to the FIFO; unchanged completed/failed work is not requeued. Explicit
CLI --force remains a deliberate reanalysis operation, protected by track leases.
Desktop discovery runs every two minutes while queue monitoring is enabled;
headless discovery uses its configured interval and runs separately from workers.
The existing Quiet/Balanced/Fast and playback pause/slow policy remain available.

A waiting job becomes in progress when an instance claims it. Its ownership token,
originating device, and expiration are stored in the shared index. Workers renew
claims every 20 seconds; expired 90-second queue claims can be reclaimed. A stale
worker cannot complete a replacement claim. The separate existing shared track
lease and metadata revision checks remain authoritative for actual publication.
A job claimed by an older/different coordinator is deferred rather than marked
completed. Completed analysis/cues are not deleted when a queue entry changes state.
Failures are recorded and other songs continue. Fixing/replacing a failed source
changes its file signature and appends it for another attempt.

Index writes use a local SQLite copy, the existing NAS-compatible publication
lock, and atomic replacement. Writable SQLite journals never run on the NAS.
Updates publish a new immutable snapshot for only the affected <=500-song catalog,
then atomically point the index at it. Readers see the old or new complete revision.
An interrupted pre-index publication can leave an unreferenced snapshot, which
readers ignore; it never replaces a newer revision. Unchanged discovery batches
and empty queue checks do not rewrite the index. Read/lease/conflict validation
is retained where necessary to protect manual metadata and multi-device work.

First use migrates the old catalog in the background, streaming rows in bounded
batches. A byte-for-byte original `catalog-before-shards-*.ampdb` recovery copy is
retained. This recovery archive can exceed 500 songs; it is not an active catalog.
Older binaries reject the new catalog version rather than overwrite it. Upgrade
Player, Analysis Engine and Cast together before using the migrated library.
Android station downloads still receive the same portable preparation and audio;
the phone's independent offline catalog does not need to read NAS shard internals.

The UI result channel holds at most eight analysis results and handles at most
eight per frame. Workers retain paths and small source signatures, not a second
full copy of every preparation. Full library scans move preparation from their
lookup map into tracks rather than cloning the complete metadata set.

Tests cover a 6,001-job persisted queue with restart after 5,001 simulated
completions, FIFO append/change behavior, failed songs, exclusive shared claims,
expired-owner rejection, no-write unchanged/empty checks, and a 5,001-track catalog
split across eleven <=500-record files. Metadata updates preserve manual cues,
unrelated files and the recovery archive; rejected publication leaves the current
index unchanged. These scale fixtures are not 6,001 real audio-analysis jobs and
are not proof of a particular historical crash cause.

Song/queue keys and catalog snapshot references are relative to the library root.
The organizer rewrites destination song/playlist references and retains prepared
metadata. Its device-local preview/recovery journal retains absolute original
locations for verification; these are not portable playback references. Device
source connection settings likewise identify where each root is mounted.

Content type can be set in Library preparation (Music, Audiobook, Speech, Podcast)
and saved through normal revision protection. Explicit types override tag/folder
hints. Clear spoken genres and dedicated Audiobooks/Speeches/Podcasts folder names
are recognized conservatively; duration and arbitrary titles are not classifiers.
New/changed unknown files may need one tag inspection, but recognized spoken audio
skips BPM/key/cue analysis and is left in its original folder. Broad Smart Playlist
folder sources exclude it; explicit song/playlist selections may include it. The
organizer preview reports spoken files left in place rather than moving them into
the consolidated music directory. Extra duplicates still remain for review; no
new unreviewed quarantine or physical book/speech moves are performed.

Cached known tracks seed the FIFO without a blocking file-by-file confirmation.
Saved waiting jobs start before background file-signature checks finish. Successful
completion records the current signature; temporary missing sources get an empty
signature so reconnecting discovery can retry them. Retry failed songs explicitly
appends failures after current waiting work. Each additional source is enrolled
into its own catalog by the Library Engine before automatic analysis publishes
results, rather than continuing to accumulate loose metadata files.

Directory scans carry file size/time observations into queue discovery; queue
ingestion uses those observations rather than stat-ing every file again. Scan-only
values are removed before publishing Library UI snapshots. Cached queue resume
does not run a file-signature confirmation pass.
