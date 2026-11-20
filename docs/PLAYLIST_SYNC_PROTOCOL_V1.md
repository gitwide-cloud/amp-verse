# Playlist synchronization protocol v1 — staged contract

Status: conservative phase-1 source implementation on an isolated review branch, not installed. Playlist snapshot and operation routes exist; automatic client synchronization does not. Existing browser editing APIs remain unchanged.

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

crates/amp-core/src/playlist_sync_contract.rs defines pure wire types, bounded validation, canonical-authority checks, portable path/key binding, operation request digests and Applied/Uncertain receipt representations. The pure contract module itself performs no publication. The separate conservative server implementation below adds source routes and CAS publication on this review branch; no runtime installation or automatic client synchronization has occurred. Its validator deliberately rejects deletion. Applied is a future wire representation, not evidence that publication is implemented. Tests exercise identity/path rejection, duplicate unresolved references, retained compatible fields, complete smart rules, request-digest binding and unsupported deletion. The conservative API fixture tests below cover interrupted intents and packed storage. Atomic tombstones, exact crash completion, deletion races and end-to-end clients remain later acceptance gates.

## Conservative API implemented on this branch

apps/amp-cast/src/playlist_sync.rs registers the two proposed routes. Both require existing private authorization plus exact x-amp-server and x-amp-library headers; POST also validates authority in its body before creating ledger files. The current mounted library identity is checked without creating it.

GET returns at most 1,000 canonical documents and 4 MiB of document JSON, with a content-derived snapshot token; oversized/unreadable snapshots fail explicitly rather than truncating or implying deletion. It is a collection of revisioned documents, not a transactional multi-document snapshot. Clients must CAS every update independently. Normal paths come from catalog-aware raw storage, not transient numeric indices; unresolved references and duplicates survive.

POST stores operation intents under private AMP_CAST_CONFIG_DIR/playlist-sync/operations, synchronizing intent and its directory before attempting shared publication. An existing unacknowledged intent always returns409 uncertain, including after restart and when current content happens to equal the proposal. A successful existing core CAS followed by verified canonical readback permits atomic acknowledgment; repeated acknowledged requests re-sync and return the recorded result. Request-digest mismatch rejects operation-ID reuse. Ambiguous publication, readback or acknowledgment errors preserve intent/proposal and return uncertainty. This is NOT exactly-once crash completion.

Normal create requires an absent baseline. Smart create in an existing Settings container requires that container's current revision, just like smart update; this preserves other definitions. The existing core save_document lock and catalog-aware atomic publication enforce final CAS. Full raw JSON is retained, including companion_available and compatible rule/document/container extensions. Unknown incompatible document schemas, cross-library source rules are rejected rather than implicitly remapped. Existing source_root_hint values round-trip as opaque hints; this API cannot change them or introduce a hint on creation, and never interprets them as filesystem destinations.

Delete remains unsupported and no tombstone advertisement is made. There is no automatic client retry worker, synchronization opt-in UI, metadata/preparation upload, ledger garbage collection or uncertain-operation resolution UI. Clients must preserve drafts and request explicit review/new operation after uncertainty.

### Verified fixture coverage

The server tests use temporary generated libraries/configuration only. They exercise packed and unpacked normal documents with unresolved duplicate tracks and extensions, smart updates and creates preserving other definitions/container extensions, exact acknowledged replay and digest mismatch, concurrent duplicate operations, stale CAS, interrupted intent before publication, interrupted publication followed by both identical canonical content and a later edit, authority/password rejection before ledger writes, unsupported deletion, changed mounted identity, and symlink containment. No process-kill/disk-failure hardware guarantee or automatic-client-sync acceptance is claimed.

## Review hardening

Sync endpoints require a configured NONEMPTY password and valid existing password authorization in addition to exact identities. Public server/library IDs alone never authorize data reads or writes; passwordless and empty-password Cast deployments cannot use these endpoints.

Existing safe normal filename stems (spaces, dots and Unicode included) keep their exact IDs and paths. Updates accept these bounded path-free legacy IDs; creates retain strict token IDs. Nested packed playlist paths are explicitly reported as unsupported_unenrolled_playlist_path instead of being silently flattened to the conventional root. No rename, omission or duplicate top-level mapping occurs.

Snapshot JSON is counted incrementally without allocating serialized copies; a document is appended only while the count and byte budgets permit it. The final envelope is also counted against 4 MiB. Normal wire conversion caps 20,000 references. Loose record reads check file size and use a 2 MiB + 1 bounded reader before JSON parsing; oversized records fail explicitly. Packed records use existing catalog_db::read (catalog_db.rs:162-179) and catalog_shards::read, whose SELECT document currently allocates one complete SQLite String without a prefetch byte cap. The API rejects that record above 2 MiB before parsing/cloning, but cannot prevent the core allocation without a separate bounded-reader change. It does not accumulate unbounded packed documents into its response.
