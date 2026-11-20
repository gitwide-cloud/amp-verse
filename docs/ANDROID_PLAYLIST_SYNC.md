# Android saved-playlist sync — review branch

This is a client implementation for the authenticated phase-1 server API. It is not deployed by this branch and has not had physical-device/end-to-end desktop acceptance.

## Enable and ownership

Connect to the desired Cast host with its private access code, then choose **Enable saved-playlist sync**. The client pins the already-selected Cast server/library IDs and verifies them again through authenticated snapshot access before saving credentials. The saved binding is independent of station IDs and discovery. Changing Cast stations does not redirect synchronization. Each server/library pair keeps a separate private state file; selecting another pair does not erase previous drafts.

The existing selected-client credential is reused only after explicit consent. It is encrypted using Android Keystore AES-GCM with the authority-specific key as authenticated associated data. It is never placed in URLs, playback items, ordinary playlist preferences, logs or exported files. A changed access code must be authorized again. The application disables backup already; synchronization files use its no-backup private directory.

## Automatic behavior and safe editing

Synchronization runs every 30 seconds while the app process is alive, on app resume and after saving a synchronized edit. There is no claim of continuous background synchronization after Android terminates the process. Pending work survives restart and resumes when the app is reopened.

Server playlists appear alongside phone-only playlists with a synchronization marker. **+ Server playlist** and **+ Server Smart** create synchronized definitions. Existing phone-only playlists remain phone-only: they are not automatically assigned a different library identity or uploaded. Imported server definitions have a dedicated editor; original local preferences are not rewritten during pulls.

A saved edit captures the editor's original canonical revision. Polling updates the saved browser state, not an open editor's copied draft, loaded decks, Player queue, cues or analysis. The controller does not own playback. Normal ordering and duplicate/missing references remain in the shadow document. Publication uses a durable playlist-only AtomicFile outbox written before network requests.

Only the exact saved pending operation is retried after transport/5xx failures, allowing the server to return its recorded acknowledgment. A 409/400 blocks that document and retains its proposal. There is no automatic new operation after an uncertain receipt. Explicit review can adopt the server version or make a new operation using a field-level three-way merge; competing changes to the same field/array remain blocked. Own successful acknowledgments patch later saved local changes without reverting other own fields, and serial smart writes advance only matching own container baselines.

Full raw smart rules and compatible extensions, source IDs, companion flags and revisions are retained. The phone editor currently changes smart name, style and target energy only; advanced source/filter editing remains on desktop/web. It does not replace the entire Settings bundle or upload preparation/metadata.

## Music availability

Track identity is the server library ID plus SHA256 of its portable relative path. Numeric catalog positions, titles and Android URIs are never used as server identities. Server normal references are validated before import, and unresolved references are not dropped.

Use **Match server songs to phone files** to explicitly save device-local mappings. Phone files and existing station downloads can be mapped without copying or altering them. No title/hash guessing or cross-library automatic remapping occurs. Only mapped available songs play locally, in the saved normal order including duplicates; missing entries stay saved. Smart local playback uses mapped eligible files and saved filters, not the server's live generated order. It does not promise full shared Smart Radio ordering parity.

The searchable/add-song catalog is bounded to its first 2,000 server rows plus references already present in normal playlists. State is capped at 16 MiB and responses at 4 MiB; an over-limit operation fails without replacing the prior durable file. No music is downloaded by this sync client. Rename migration, phone-only Smart source conversion, deletion/tombstones, boot/background jobs, automatic portable metadata publication and conflict deletion are not implemented.

## Checks

Pure Java model fixtures cover authority isolation, missing/duplicate references, immutable unsaved editor snapshots, durable operation replay after restart, uncertain/conflict pause, field-level review safety, own-Ack/newer-edit reconciliation, smart raw extension preservation and numeric normalization. The branch workflow builds/lints Full and Lite, runs Full unit tests (including the shared model), and compiles Cast/Remote companions. Lite unit compilation is not requested because preexisting shared Lab unit tests refer to Full-only classes; this follows the existing Android workflow's Full-unit convention.


Repeated unchanged saves do not generate publication/revision churn. Overlapping local editors retain both saved proposals instead of silently replacing one. Explicit conflict review is fenced to the exact version displayed; a newer server version requires reopening review. Invalid names and oversized reference lists fail before durable outbox changes.

The Cast panel exposes synchronization through an opt-in Listener capability/callback instead of depending directly on the sync UI. Full MainActivity/TV opt in. Standalone Cast companions default to hiding this action until an authorized AMP Player IPC bridge is supplied; the Full-edition setup path is currently required. This seam is intended to preserve SAG-87 shared-panel/FullCastPanel routing rather than overwriting that independent flow.
