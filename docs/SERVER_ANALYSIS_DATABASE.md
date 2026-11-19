# AMP Verse server analysis and shared database

Owner architecture, 2026-11-19. This document distinguishes implemented foundations from the remaining integration work.

## Required platforms

AMP Verse Server is a Windows AND Linux product. Both platforms require library intake/management, analysis orchestration, database/API, virtual stations, uploads/sync and physical audio output for received desktop mixes. OS-specific audio/GPU/service adapters sit behind common server semantics. Validate Windows device/path/service behavior independently from Linux; a successful Linux build is not Windows acceptance. Keep the existing Windows/Linux build matrix. Browser controls must not depend on the server OS.

## Product split

The server is the always-on music hub, library authority and primary preparation machine: discover completed imports from Lidarr, the downloader, folders, or other import paths; identify the exact recording; reuse existing information; compute missing features and stems; refine with approved web providers; publish versioned prepared results.

Server playback/mixing has a simple, mobile-like surface: Player, Library/Smart Playlists, Auto Mix, queue and outputs. It can play/mix independently of an open desktop. It must also host persistent virtual stations backed by Smart Playlists, with station-specific queues, mixing rules, controls and listener state. Multiple stations require independent playback owners and isolation; the current Cast implementation advertises one station per server, so multi-station hosting is a remaining architectural change, not a shipped capability.

Server music management receives completed Lidarr/download jobs, desktop uploads and folder imports through one intake path. It sorts and maintains the master library according to the owner's organization/exclusion rules, indexes it and prepares it. Uploads land in staging and are validated before publication; duplicate/conflict handling must not silently overwrite or delete existing audio. Downloader source support must follow the source's supported access method.

Desktop is a lighter performance workstation: VirtualDJ/Mixxx-style browsing, cueing, manual mixing and Auto Mix consume prepared database records. Desktop may write reviewed corrections and explicit preparation results. Full-library background preparation belongs on the server. Offline desktop performance uses cached prepared data. Audio can be read directly from a LAN share or deliberately downloaded/synced to local storage. A desktop download can later be uploaded/imported into the master library. Show file availability and active playback location clearly; sync must be resumable and explicit about direction, version conflicts and deletions.

Mobile has the simple playback/mixing and station-browsing experience, and reads/caches prepared data for playback, mixing and offline use. It does not run the analysis engine or publish analysis corrections. Local playback state/offline queues are separate from shared analysis writes. Existing mobile features that can edit preparation must be audited before claiming this access split is enforced everywhere.

The browser is a control/view surface. Analysis jobs run on the server, not in the browser audio callback. Server and desktop share track identities and preparation semantics instead of inventing different analysis formats.

## Shared playlists and media sync

Normal playlists and Smart Playlist rules synchronize bidirectionally across server, desktop and Android. Mobile read-only access refers to ANALYSIS, not playlist editing. Preserve stable IDs, revisions, offline edits, ordering, rename/delete behavior, conflicts and existing playlist data. Do not silently overwrite a concurrent edit. Keep shared rules separate from each device's available media/selection. Playlist metadata sync is distinct from downloading/syncing audio files.

## Cast: laptop master to physical mixer

The main Cast requirement is desktop DJ MASTER AUDIO -> LAN -> server receiver -> selected server physical audio device -> external mixer. The desktop performs the live mix; the server plays the finished stream without recreating it from track/playlist events. Preserve local headphone/private cue unless explicitly routed. The server's web controls expose the local output, receiver state, volume, stop/reconnect and honest buffering/latency telemetry. Independent server Smart Radio, Google-speaker casting, remote control and file sync remain separate capabilities. High-performance mode needs measured end-to-end latency/dropout/clock-drift testing; buffer targets are not measurements. Reconnection discards stale backlog and resumes current live audio.

## Data model

- Library tracks and paths are associations, not the identity of a musical recording. Exact file identity starts with SHA-256; different encodings/retags require separately validated audio-identity mapping before reuse. Artist/title alone is never enough, especially for DJ edits and remixes.
- Store immutable observations with source hash, producer, producer version, settings hash, kind, and content-derived record ID. Keep conflicting measurements and provenance; do not replace one provider's result with another without review.
- Distinguish provider hints, native measurements, prepared summaries, reviewed grids/cues, and stem manifests. A BPM/key lookup is not an exact beatgrid or a stem file.
- A resolved preparation references its source observations and reviewed revisions. Desktop edits need revision checks and explicit authorship. Automated refinement must preserve reviewed corrections.
- Audio/stem objects live in managed storage; the database references validated objects/checksums instead of duplicating audio per device or embedding it in ordinary metadata responses.
- One server owns the working database on local storage. Clients use paged APIs and cached read-only snapshots. Do not put a writable shared SQLite/WAL database on SMB.

## Implemented in this branch

Original Rust module `amp_core::knowledge` provides an additive device-local SQLite store with immutable, idempotent observations, indexed source lookup, bounded pages/batches, schema/application identity checks, and record integrity checks. Different outputs, versions and settings coexist.

`amp-cast` exposes authenticated routes:

- `GET /api/knowledge/status`
- `GET /api/knowledge/audio/{lowercase_sha256}?limit=50&after=RECORD_ID`
- `POST /api/knowledge/import` with `{ "offset": 0, "limit": 50 }`

Import processes at most 100 already-loaded preparation summaries per request. It does not scan, decode, rehash, upload, change source music or modify playback. Follow `next_offset` until null. This is a snapshot backfill, not a claim that every on-disk track has been imported.

New completed server analysis also indexes its preparation summary into `knowledge.ampdb` beside the server configuration. Knowledge indexing errors are reported separately from analysis failure and do not discard completed preparation. A missing configuration directory disables these operations explicitly.

The summary includes measured/confidence metadata and review flags, but excludes paths, personal cues and source/stem audio. Its source identity is inherited from saved preparation, not freshly verified during import. Therefore summaries explicitly have `automatic_reuse_eligible=false`. The store is an evidence foundation; it does not yet replace the portable catalog or feed Auto Mix directly.

The new API accepts no arbitrary client-written observation/document. Its import command asks the server to index its own preparation. Dedicated desktop writer credentials and revision-checked correction endpoints are not implemented yet. Existing station authentication applies; do not claim a global desktop/mobile role system has shipped.

## Remaining implementation sequence

1. Source adapters feed one bounded server queue. Wait for stable/completed downloads; ignore partial files, excluded speech/audiobooks and unsupported inputs; give each item one identity/versioned job. Reconcile moves, duplicates, retries and rescans rather than continually redoing completed work.
2. Add identity-verified native feature/stem observations, model/settings manifests, resumable preparation, and migration from current catalog. Cache hits must bypass expensive DSP and preserve manual data.
3. Resolve observations plus reviewed desktop edits into versioned preparation. Add explicit writer authorization and optimistic revision checks; mobile receives read-only analysis scope. Retain rollback/export and offline caches.
4. Add provider adapters with exact-version/duration matching, provenance, positive/negative caches, rate limits, and bounded retries. Start with existing tags/open metadata. Paid jobs and audio/fingerprint uploads require an explicitly selected provider/configuration.
5. Desktop Auto Mix and Android consume the same resolved records. Prepared stems/grids must be present and verified; otherwise use an honest fallback. Validate latency, quality and phase alignment; do not synthesize confidence or silently fetch heavy assets during playback.
6. Benchmark 1k/10k/30k catalog reads and queue writes, then compare rendered transition fixtures and real listening. UI must show real preparation state and why a feature is unavailable.

## Open-source engine research boundaries

AMP's server/core are already Rust. Study upstream architecture and documented algorithms, then implement/test bounded improvements. A Rust translation of GPL source remains a port with its upstream obligations; do not silently place it under AMP's MIT header. This branch contains no copied/translated Mixxx, Rubber Band or OwnTone code and does not claim to port their complete engines.

- Mixxx engine architecture: https://github.com/mixxxdj/mixxx/wiki/Developer-Guide-Engine (real-time thread discipline and separate output buses).
- Rubber Band technical notes: https://breakfastquay.com/rubberband/technical.html (time-stretching/transient/phase problems; not a claim that AMP implements the same engine).
- OwnTone documentation: https://owntone.github.io/owntone-server/ (server control and output architecture).
- SQLite network/WAL restrictions: https://www.sqlite.org/useovernet.html and https://www.sqlite.org/wal.html.

Stem ML inference should initially use an isolated, versioned open-source worker. Rewriting an inference framework or model into Rust is a separate benchmarked project, not a prerequisite for a Rust scheduler/database or a promise of faster processing.
