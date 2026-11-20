# Desktop automatic saved-playlist sync

This client is opt-in. In AMP Cast, connect to the chosen station using its
existing access code, then choose **Enable sync with this station**. An
authenticated sync snapshot must confirm the exact server and library identities
before the binding is saved. Sync is separate from live Cast routing: starting a
local broadcast does not change the paired canonical server. No new IP-address
settings or automatic selection of the first discovered station is introduced.

While the desktop app is running, a background worker checks saved normal and
Smart Playlists about every 15 seconds. It synchronizes playlist documents, not
audio files, analysis, cues, tags, playing queues, decks or unsaved editor drafts.
Opening another music root pauses the binding. **Pause playlist sync** disables
automatic work without changing saved playlists or deleting retained proposals.
No application/service is installed by enabling this feature.

The current implementation requires the same library identity on both sides.
This supports the same NAS share and a deliberate copy retaining its identity and
relative layout. An unrelated local library is refused: source/origin remapping
has not been implemented. Music-source mappings and audio availability are not
inferred from matching filenames. Existing playlists outside `ampdj/playlists`
are not silently enrolled. Deletion synchronization is disabled until the server
supports tombstones.

A saved playlist already visible through the same physical NAS share is observed
without POSTing another write or increasing its revision. Separate copies use
remote revision compare-and-swap. First-pair differences, concurrent edits and
unacknowledged publications are retained for review instead of choosing a winner.
An uncertain operation is never automatically resubmitted under a new ID or
cleared merely because content now matches (including when both files are missing).
The first desktop client reports and retains these conflicts; an interactive
resolution panel is not implemented. Retained outbox files should not be edited
to force a retry without reviewing the server receipt and both saved versions. Normal
ordered references, duplicates and missing audio entries remain in saved data;
a missing member may still prevent loading that playlist for playback until it
is available. Smart documents are transported/hydrated as raw JSON, retaining all
current rule fields and compatible additional fields during synchronization.
Existing typed editor Save behavior is separate: unknown future-schema fields
are not editable by this client and require compatibility review before older
editors change those documents.

Private local state lives under the installation's device configuration directory
in `playlist-sync`. It stores the canonical baseline/revision, pending complete
operations, durable offline desired data and conflict snapshots. It does not
consume the older generic metadata outbox. Windows access codes are protected by
current-user DPAPI; Linux uses owner-only directory/file permissions (0700/0600),
not OS-vault encryption. Credentials are never included in status/error messages.
The existing server protocol uses authenticated local HTTP, so use a trusted LAN
or its protected tunnel; this change does not add public exposure.

Saved browser lists refresh without replacing normal-playlist reservations or
Smart Playlist performance snapshots. Unsaved Smart editor drafts remain open;
normal queue edits and their old saved revision are retained, so a later Save
conflicts safely if the document changed meanwhile.

Validation covers real loopback server requests for shared-storage no-op and
same-identity copied-library publication, retained uncertainty, offline desired
state, CAS conflicts, missing references/duplicates and saved-browser versus
playing-queue/draft noninterference. Actual cross-device acceptance still requires
paired desktop and Android clients connected to the updated server; installing
an artifact alone is not that evidence.
