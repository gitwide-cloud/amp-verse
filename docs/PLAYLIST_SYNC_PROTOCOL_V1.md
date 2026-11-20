# Playlist synchronization protocol v1 — staged contract

Status: design/staging only. No sync routes or automatic client synchronization are enabled by this document. Existing browser editing APIs remain unchanged.

## Authority and transport

A client explicitly binds to (server_id, library_id). Discovery is never authority selection. Use existing private Cast authorization and origin protections, require both exact identities for reads and writes, and reject mismatches before reading an operation receipt. Device identity is attribution, not authorization. Android may edit playlists; this contract never grants metadata/preparation writes.

Proposed GET /api/sync/v1/playlists returns protocol=1, server_id, library_id, snapshot_token, documents, tombstones. Use bounded paging with a pinned snapshot token or restart pagination if publication changes. Do not infer deletion from a missing page, hidden/unshared entry, malformed file, or unavailable library.

Proposed POST /api/sync/v1/playlist-operations accepts operation_id, device_id, server_id, library_id, document_id, kind, document_version, base_revision, operation (create/update/delete), and complete document for create/update. Reject unknown operation kinds and oversize payloads. IDs must be bounded, path-free tokens. An operation ID is bound to the canonical request digest, authority and device; reuse with different content is a conflict.

Responses include the operation ID, canonical complete document/tombstone, canonical revision and snapshot token. Revision mismatch is HTTP409 with the current canonical document; validation is400, identity mismatch403, unknown ID404, temporary storage failure503. Acknowledgment means durable publication and durable retry evidence, not merely queued in memory.

## Documents and references

Preserve existing IDs: normal_<existing filename stem> and smart_<existing rule ID>. Preserve companion_available, unknown compatible JSON fields, and all saved smart rules. Normal order is an ordered array, including duplicate tracks. Do not deduplicate or silently drop unavailable tracks.

Wire track identity is (library_id, track_key), where existing track_key is song_id(normalized root-relative path). Carry the portable relative path as a mapping hint, never an absolute desktop path or Android URI. Validate that the key matches that hint; reject traversal, absolute paths and mismatched library binding. An unresolved legitimate key remains in the canonical list with resolution state; it does not become a different song. Path-derived IDs do not transparently survive renames. Duplicate content hashes never authorize automatic remapping.

Smart definitions are definitions, not generated playback orders. Retain source_library and all rule fields and references. Device-local source_root_hint must not become an authoritative shared absolute path. Unknown incompatible rule versions are rejected; unknown compatible fields round-trip. Normal snapshots read raw JSON through catalog-aware storage instead of Playlist::load, which currently rejects unresolved audio references. Validation and resolution must be distinct.

Initially smart updates may CAS the existing complete Settings revision. This deliberately conflicts across concurrent edits to different smart definitions instead of losing one. Do not silently migrate to per-list revisions without explicit storage compatibility.

Deletes require a revisioned tombstone. Stale update-vs-delete and stale delete-vs-update must conflict. Recreation requires an explicit operation against the tombstone revision. Retain tombstones/receipts until a defined device-acknowledgment/retention policy exists; do not expire them opportunistically.

## Crash-safe publication prerequisite

Current coordination::save_document atomically publishes ONE document under its shared record lock. catalog_db::write can publish one packed record via its own database/shard snapshot. There is no existing atomic playlist + operation receipt + tombstone publication primitive or catalog deletion API.

A private server intent journal followed by document publish then receipt is insufficient: crash after publication followed by a desktop edit can erase any document marker. On retry the server cannot prove whether that exact operation committed. Comparing current content with the proposal cannot resolve this ambiguity and must not trigger a blind retry.

Narrow implementation proposal:
1. Add a playlist-specific durable pending journal in library-owned coordination storage, and integrate recovery into EVERY existing save_document writer while holding the SAME per-document lock.
2. Journal exact before revision/digest, exact intended canonical bytes, request digest, operation identity and result. Sync journal and parent before publication.
3. Before any later writer publishes, finish/resolve the pending operation under that lock and durably record its result; only then admit the later writer.
4. Recovery: before digest => publish intended bytes; intended digest => finish receipt; another digest => stop with explicit ambiguous/recovery-required status, never overwrite it. Journal fencing and migration locking must cover catalog compaction and direct packed writes too.
5. Deletes need catalog-aware visibility and a durable tombstone readable by all playlist enumeration paths. Physical removal alone cannot prevent resurrection by an older client.
6. Prefer one catalog transaction containing document, tombstone and receipt when all writers can share it; sharded catalogs and unpacked JSON require equivalent guarantees. Do not claim atomicity merely because each file rename is atomic.

This touches shared writer and packed-catalog boundaries. Implementing only an HTTP handler is not a safe shortcut. Keep routes disabled until crash/restart and mixed-writer fault tests establish this invariant.

## Client phases (not implemented here)

Desktop and Android each need durable baseline/outbox, canonical binding, stable local media mappings, automatic retry after explicit opt-in, and conflict UI. Existing offline::Edit remains explicit metadata review and must not be auto-published. Existing Android RadioOffline continues read-only metadata reconciliation. Preserve loaded decks while canonical playlists update.

Acceptance gates: ordered duplicates and unresolved refs; full smart/unknown-compatible-field round-trip; wrong identity/auth/cross-origin; retry after acknowledgment; crash at every journal/publish/receipt boundary with a subsequent desktop writer; competing reorder; delete-vs-edit; packed/unpacked/sharded catalog; compaction during publication; offline Android playlist edit returning through server to desktop; metadata writes denied.


## Conservative first endpoint option

An alternative first implementation may retain a server-private pending intent and perform exact-revision CAS, while explicitly declining exactly-once publication. Persist intent before any write. On restart, any pending intent without a durable result becomes an Uncertain receipt; never infer success from equal document content, automatically retry publication, silently acknowledge, or discard its draft. Return structured recovery-required status plus canonical current document and retained proposal for review. A fresh explicitly reviewed operation must use the current baseline and a new operation ID. This can be safe without changing every writer, but is not durable exactly-once processing. Delete stays unsupported until atomic tombstones and visibility exist.

## Staged code

crates/amp-core/src/playlist_sync_contract.rs defines pure wire types, bounded validation, canonical-authority checks, portable path/key binding, operation request digests and Applied/Uncertain receipt representations. No route registration, storage publication, runtime configuration or client synchronization was added. Its validator deliberately rejects deletion. Applied is a future wire representation, not evidence that publication is implemented. Tests exercise identity/path rejection, duplicate unresolved references, retained compatible fields, complete smart rules, request-digest binding and unsupported deletion. Crash recovery, packed storage, deletion races and end-to-end clients remain acceptance gates for a later implementation, not passing claims for this staged contract.
