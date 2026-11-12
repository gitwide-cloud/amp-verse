# Smart Playlist folder sources

Smart Playlist editing now shows the indexed music root and total separately from the playlist's eligible-song count. Browse Library Folders opens an expandable tree built from that completed index, with recursive song counts on each folder. Expanding an arrow navigates; selecting a folder includes its files and every descendant folder without per-song Allow selections. Save Playlist publishes the draft.

Windows folder comparisons are component-aware and ignore ASCII casing, so `Music` and `MUSIC` do not produce different selections. Matching never treats `MusicOld` as a child of `Music`. Canonical disk paths are used for native picks; stored relative spelling is normalized to the indexed folder tree for portable rules. A different share or an outside folder is rejected without deleting existing source rules, and the message identifies the current indexed root rather than suggesting the library lost its songs. Native Windows browsing remains available alongside the indexed tree.

Legacy folder drafts with a selected folder and an empty allowed-song list now include that folder's descendants; the old empty-source safety flag no longer forces users to Allow every song. Explicit exclusions and nonempty selected-song restrictions are preserved. An empty folder selection still includes no songs.

Selecting a source requests a background recursive refresh. The completed catalog remains usable during refresh and the existing periodic scan discovers newly added files. A new folder outside the indexed root still needs to be chosen in Library > Sources; selecting an unrelated NAS or drive does not silently replace the current library.

Verified with nested-folder and Windows-case regression tests, root selection, exclusion protection, outside-folder rejection and the user's actual NAS index: selecting its root includes all 4,405 songs recursively.

## Additional music roots

Library > Sources > Add music folder adds another root without replacing the main library. Scans include subfolders and combine tracks with the existing catalog; the previous catalog remains available during background refresh. Change main library remains a separate, explicitly named action. Remove source disconnects a root without deleting its audio or preparation.

Additional roots retain their own library identity, preparation, protected edits and analysis leases. The combined catalog uses stable source-prefixed paths and resolves playback and edits back to the original folder. Overlapping physical files are deduplicated. Missing sources remain configured and can be reconnected by adding their new location; device-specific locations preserve the shared default mapping.

The source configuration lives inside ampdj (in catalog.ampdb when compacted). Smart and regular playlists can include tracks from added roots; folder navigation shows them under Additional music sources. Each computer must have filesystem access to its configured source locations.

## DJ Smart Playlist browsing

DJ computes saved Smart Playlist membership in a background worker, reusing the completed result across redraws. New/edited rules and replacement library scans invalidate the result; analysis progress also invalidates it. Other metadata edits are picked up by a background refresh after roughly two seconds while that list is visible. Loading a new match shows a progress message rather than a false empty-source message. Old results are never reused for different rules or a different library generation.

This cache is for the visible DJ song table. Auto DJ still performs its existing current-rule eligibility and next-song selection checks. It does not rescan music or reanalyze tracks just to display a saved list. Validation includes a 14,000-track matching/reuse/rule-change/analysis-change test and the existing Player UI checks. Slow NAS access while discovering new files or preparing a loaded deck remains separate work.

## All music as a Smart Playlist source

New Smart Playlists start with all indexed music as their source. Genre and analysis filters then narrow that pool. The source editor has a visible **Use all indexed music** button, which preserves those filters and existing song exclusions. Selecting folders or importing playlists still creates a narrower source explicitly. Existing saved source restrictions are retained on upgrade.

An empty selected-folder source deliberately matches no songs. The editor now explains this beside the source controls and offers the all-music choice; it does not treat the indexed-library count as a selected source. Save Playlist publishes the chosen source and filters to DJ. Refresh playlists reads the saved rules without reanalyzing audio.

## Basic Library actions

Use **Add music folder…** to index another source, including subfolders. **Add folder to queue…** adds indexed songs to a regular playlist queue. New regular playlists need only a display name; **Create from queue** keeps the current song order and duplicates, and storage is handled inside ampdj. Existing playlists are protected from overwrite even after metadata is packed into the catalog.
