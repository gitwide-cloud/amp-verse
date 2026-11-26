# Saved playlist delivery

The enabled binding chooses the authority for new normal and Smart Playlists. The primary Library create actions now use that binding; explicit Phone-only creation remains available separately. Existing phone-only playlists and their files are preserved and labelled. They are never automatically converted or uploaded.

To share an existing phone-only draft, choose Publish saved version / Publish reviewed copy. Publication creates a new server copy only after review. Every required phone song is explicitly matched to a server catalog reference; titles and filesystem paths never imply identity. Normal order and duplicates survive. Smart genres, filters and other rule fields survive, with an explicit choice of server library or matched selection as the source. Local source paths and phone IDs are not sent. Cancel keeps the original untouched.

Queued, blocked/conflicted and server-verified states are distinct. Enabling a binding is not proof of playlist delivery. An applied operation acknowledgment is followed by a new server snapshot before the normal verified status is reported. Other clients continue polling the same authority; the phone does not claim acknowledgment from clients that this protocol cannot observe.

The desktop boundary guard canonicalizes both the persisted root and the nearest existing target ancestor. Windows comparison uses complete path components, with ASCII case equivalence only for the drive/UNC prefix (host/share). Canonical normal components remain exact, preserving boundaries on case-sensitive Windows directories as well as rejecting sibling-prefix escapes. Lexical parent traversal remains rejected; out-of-library symlinks/junctions remain rejected.

Regression coverage: exact mapping and no title guessing; ordered duplicates and untouched originals; nonempty Smart genres/filters; queued/conflict states; real server operation ingestion followed by independent desktop hydration; canonical aliases and symlink escapes; Windows UNC case, actual mixed-case filesystem roots and junction escapes. Java sources parse, Rust formatting and diff checks pass. A std-only extraction of the actual guard passed its Linux alias/escape test. Full Android/JVM and Windows/Linux workspace tests require CI; no live configuration, phone files or owner playlists were changed for validation.


## Whole-library reference selection

Publication matching and normal-playlist Add now share a server-side search picker. It uses the selected authority's existing Studio `q` endpoint, requesting at most 200 matches per debounced query across the entire library; the initial 2,000-row cache limit is unchanged. Results display friendly titles plus relative paths to distinguish versions. Search is read-only and never queues publication or guesses identity.

Each request publicly verifies the exact server/library before using the private credential for Studio and verifies that identity again afterward. Cancellation disconnects active HTTP; query generations and selected-binding generations reject stale results. Only an explicitly chosen, verified reference is added to durable cache, under the same current-binding guard. Switching/pause/reselection invalidates an open publication review. Errors use actionable generic UI text without exposing server URLs or addresses.

Search tests cover a selectable song beyond catalog row 2,000 (row 20,573), exact ordered duplicate publication from that reference, public identity rejection before private dispatch, stale selection and cancellation refusing cache/publication, and network failure remaining failure. Full JVM/Android execution remains CI validation; no owner server/library or phone data was modified during source work.
