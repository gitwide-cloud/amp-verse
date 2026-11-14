# Desktop UI latency fixes — 1.0.20

Two recurring costs were found in the animation path:

1. Remote regenerated the full song JSON catalog, scene list and all playlist
   memberships on the GUI thread every two seconds, even with unchanged data.
2. DJ browsing lowercased every song title during every frame with an empty
   search. Regular-playlist rows also repeatedly performed a linear library
   search for every playlist member.

## Changes

- Catalog setup and reconstruction run on a background worker. Initial Remote
  discovery/listening starts after its catalog is ready, so a connecting device
  does not receive a temporary empty library. Setup shows progress and can be
  stopped without creating a server later.
- Subsequent rebuilds use library/preparation revisions, saved playlist settings
  and scene revisions as their key. Unchanged data does not start a worker. Only
  one builder runs at a time, and newer changes remain eligible after it finishes.
- Catalog replacement and disposal happen on the worker, using the selected
  service instance's sink. A stopped service cannot publish into a newer one.
- Remote playback-state publication is capped at ten updates per second instead
  of serializing state on every UI repaint. Commands still wake the GUI directly;
  audio processing and MIDI handling are not throttled.
- Empty DJ searches skip text processing. Nonempty query results are cached
  against the displayed source indices and reset on explicit list refresh.
- A path index is built once per displayed library snapshot. Regular playlists
  retain their ordering and repeated entries without repeatedly scanning the
  entire library for each row.
- Saved Library preparation/tag changes invalidate the remote catalog; transient
  transport/volume changes do not.

A changed catalog still takes a one-time in-memory library snapshot before the
worker starts. This is not a zero-allocation or measured frame-time guarantee.
Source scans, explicit downloads and analysis keep their existing behavior.

## Checks

Focused tests cover a blocked background builder with no overlapping work,
revision invalidation, deferred Remote startup with a real TCP catalog request,
cancelling pending startup, and search results that stay stable until explicit
refresh. Existing playlist order/duplicates, routing, waveform, drag-and-drop and
responsive-layout checks are retained. Workspace tests, local Lab workflow
fixtures and strict Clippy pass on Windows.

These code-path checks do not establish live performance on every machine or a
Linux device benchmark. Existing playback is left running while the verified
replacement is staged for the next application launch.
