# Shared Library coordination — protocol 1

Use the same library-root boundary on every participating computer; overlapping roots are rejected rather than silently rebinding another library’s records. A library has one persistent UUID in its root `.ampverse/library.json`. Each installation has a different UUID in its private `device.json`. Set `AMP_DEVICE_CONFIG_DIR` to private persistent local storage; never share or clone this directory between servers. Moving a library together with `.ampverse` preserves its identity. Separate libraries get separate IDs. There is no permanent master computer.

## Claims and fencing

Library, the headless `amp-prepare` executable and Cast background analysis all call the same `amp-core::coordination` implementation. The CLI now belongs to the headless Cast package, so ARM/server builds do not compile the desktop GUI.

Unchanged valid analysis is reused using the engine version, stored SHA-256 and source size/mtime signature. Missing/changed signatures trigger fingerprint verification. This avoids repeatedly reading and hashing every unchanged audio file on older NAS hardware. An engine-checked marker also records completed proposal generation, so protected or declined suggestions do not trigger repeated automatic computation. The engine version and logical job type are defined once in the analysis module. Older participating builds reuse a newer completed version from the same engine family rather than automatically downgrading it, including during force verification. `--force` explicitly rechecks the source through the analyzer. Same-size content changes that deliberately preserve mtime require force verification. Replaced audio with a different fingerprint is reported for review rather than attaching old approved cues to different content. Rename/content-index reuse is not implemented yet.

For work that remains, a worker attempts an exclusive short OS file lock on the track's canonical metadata lock file. While holding it, it atomically reads/claims a type-specific lease record with Library ID, Device ID, random ownership token, generation, analysis type, state and UTC expiry. Different analysis types have separate leases; all publication still serializes through the same track lock. A busy lock or unexpired claim skips the job. Filesystem/locking errors pause or fail safely.

The analysis lease is 120 seconds, renewed approximately every 40 seconds by a lightweight heartbeat. Renewals require the same unexpired token and generation. Reclaiming an expired job increments the persisted generation. Publication reacquires the same track lock and verifies ownership/expiry immediately before replacing metadata. An expired worker cannot renew or overwrite a replacement owner's completed result. Completion is persisted; after a crash, valid results are reused even if the completion marker was not written.

Lock files are permanent and must not be deleted to unlock a job. OS locks are released on process exit. Lease files are atomic JSON records, not existence-only sentinel files. Malformed coordination records fail closed rather than being treated as missing. Orphan `.tmp` files are ignored; they are never loaded or substituted for completed metadata. They may be removed during maintenance when no writer is running.

## Required filesystem behavior

The share must provide coherent OS locking across all clients and atomic same-directory replacement. Linux uses `F_OFD_SETLK` byte-range locks (kernel 3.15+) rather than `flock`, which older Linux SMB clients do not propagate remotely. Windows uses its native file-lock API. There is no fallback to process-associated POSIX locks, which can be released by another thread closing an unrelated descriptor. Windows SMB, Linux SMB/NFS and native QNAP mounts must be tested together before production. Mounts configured with local-only/no-lock semantics are unsupported. Client clocks must be synchronized; a large forward clock jump can expire a lease early, and a backward clock jump can delay recovery. Lease ownership and generation checks remain mandatory; clock time alone never authorizes publication.

Local tests and emulated CI are not proof of a particular NAS/mount combination. The LAG acceptance gate includes two different computers attempting the same job, takeover after power/process loss, and a stale owner attempting publication. Do not enable AUHUBNAS production analysis until those tests pass. Update all metadata writers (desktop editors, analyzers and servers) before enabling cooperative shared-library writes. Older Linux writers use a different lock domain and older analyzers do not participate in claims; read-only listening may continue during rollout.

## Metadata revisions and merges

Canonical records remain beside each audio folder in `.ampverse/tracks/<audio-name>.json`; legacy adjacent sidecars remain readable. `_amp_sync` adds a record revision, last Device/Library IDs, per-field revisions and per-field author IDs. Existing preparation fields and unknown extensions survive.

A save validates the candidate and source identity, locks the record, reloads current data, and compares the original read snapshot. Independent JSON fields merge. Concurrent changes to the same field are rejected, including revision changes that return the field to an earlier value. Cue arrays, loops stored as one extension and ordered playlist/rule arrays are conservative atomic fields. Disjoint edits within the same cue array require review rather than an unsafe automatic merge.

A checked, synced temporary file is atomically renamed over the record. A stale or unsupported record is never treated as an empty record. A newer revision is not replaced with an older snapshot. Legacy records enroll on explicit guarded saves. Playlists/rules also record origin and revision; conflicting proposed versions are written to separate `.conflict-<uuid>.json` files. The current version remains unchanged. Reload saved rules reads the actual shared version; a rejected save does not silently advance a draft's baseline.

Approved/manual cues, grids, key/BPM corrections, loops and other preparation retain priority. Existing protected analysis produces recommendations. If approval happens while an automatic job is running, publication preserves current fields and stages only the worker’s deltas against current values. Independent derived waveform data can coexist. Type/owner-stamped `analysis_proposals` retain concurrent pending results; `analysis_recommendation` is the primary Library review view. A general multi-type proposal browser remains future work.

## Offline changes and playback

Core `offline::Edit` stores the library/device identity, original revision snapshot and local proposal. Preview compares shared revisions without applying anything. Only `publish_approved` writes, and it repeats conflict checks under the normal metadata transaction. A conflict retains both the local document and current shared metadata. Failed playlist/rule saves retain local document proposals as well. Failed desktop preparation saves queue a local draft under the private device `outbox` when available; reconnecting does not upload it automatically.

The Android download/import and **Sync My Changes** review screen are still pending. The core protocol and simulated offline-conflict regression are implemented; no Android sync capability is claimed by this milestone. Desktop queued drafts likewise have no general outbox-management screen yet; the current editor keeps its draft open and can retry explicitly. Do not edit an outbox JSON into the NAS automatically.

Loaded DJ decks hold independent snapshots. Library refresh/review does not replace the running decoder's source offset, analysis, energy feedback or scrolling cue overlay. Fresh metadata is read when a track is explicitly loaded again. Each instance has its own audio/control session. This does not add an SSD audio cache: playback can only survive NAS loss while the required audio remains buffered or is locally available.

## Acceptance evidence

| Requested scenario | Evidence/status |
|---|---|
| Two workers, same song | Simultaneous claim test; two-device native CLI smoke analyzes six songs exactly once. |
| Worker crashes | Subprocess exits with a live lease; real lease expiry permits generation-2 takeover. |
| Expired publisher | Old token cannot renew/publish after takeover; replacement's approved metadata remains. |
| Same cue edited twice | Shared version retained; second edit reports conflict. |
| Different fields/types | Energy and waveform-type claims merge; per-field authors retained. |
| Playlist/rule collisions | Current file preserved and conflicting proposal retained separately; legacy enrollment tested. |
| Android offline reconnect | Core local-proposal preview/conflict test; Android UI/transport pending. |
| NAS disconnect | Simulated missing/corrupt claim fails closed; real network interruption still requires LAG hardware test. |
| Cue changes during playback | Muted native device regression verifies old offset/BPM until explicit reload. |
| Independent radio servers | Native two-server smoke checks distinct persistent IDs and isolated start/stop; wrong selected-server ID denied. |

Locking references: [Linux flock SMB behavior](https://man7.org/linux/man-pages/man2/flock.2.html), [Linux OFD byte-range locks](https://man7.org/linux/man-pages/man2/F_OFD_SETLK.2const.html). Disable local-only/offline metadata caching on shared mounts; cached audio is separate from authoritative `.ampverse` transactions.
