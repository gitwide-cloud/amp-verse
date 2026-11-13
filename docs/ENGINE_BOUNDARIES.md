# Engine boundaries (1.0.9)

Library Engine owns shared catalogs, the durable analysis queue, source discovery,
content classification, preparation, duplicate detection, organization and metadata
revision protection. Its shared Rust namespace is amp_core::library_engine. Heavy
analysis remains the optional amp-engine process; Library UI calls the same core
for organization and storage. No second implementation is introduced for servers.
The add-on is labelled AMP Library Engine; the executable/protocol name stays
compatible with existing installations and download assets.

DJ Engine owns real-time rendering, transport, scratch/fade execution, effects and
stem audio. Desktop mixer.rs is compiled as the dj_engine module and owns the
stems worker module. Shared stem primitives live in amp_core::dj_engine. The
Library Prepare Stems screen is a caller of that worker, not another separator.
Separation remains optional, resource-controlled and outside the audio callback.
Android continues consuming synced analysis/stems without installing an analyzer.

DJ Logic owns choice and scheduling. Desktop performance.rs is compiled as the
dj_logic module; pure phrase/transition planners live in amp_core::dj_logic.
It tells the playback engine what to execute rather than implementing a second
renderer. Compatibility aliases retain existing call sites and API names.

This is an ownership/module refactor around working implementations, not a new
DLL/service architecture or an AI learning system. Some desktop views remain in
their feature modules. A future feedback interface can propose cue improvements
from measured handoffs, but no automatic cue coaching/training is claimed here.
Approved cues and loaded playback snapshots must remain protected. Separate
installable DJ/logic packages or an independently shared native renderer are not
introduced by this change.
