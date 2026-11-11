# Smart Playlist folder sources

Smart Playlist editing now shows the indexed music root and total separately from the playlist's eligible-song count. Browse Library Folders opens an expandable tree built from that completed index, with recursive song counts on each folder. Expanding an arrow navigates; selecting a folder includes its files and every descendant folder without per-song Allow selections. Save Playlist publishes the draft.

Windows folder comparisons are component-aware and ignore ASCII casing, so `Music` and `MUSIC` do not produce different selections. Matching never treats `MusicOld` as a child of `Music`. Canonical disk paths are used for native picks; stored relative spelling is normalized to the indexed folder tree for portable rules. A different share or an outside folder is rejected without deleting existing source rules, and the message identifies the current indexed root rather than suggesting the library lost its songs. Native Windows browsing remains available alongside the indexed tree.

Legacy folder drafts with a selected folder and an empty allowed-song list now include that folder's descendants; the old empty-source safety flag no longer forces users to Allow every song. Explicit exclusions and nonempty selected-song restrictions are preserved. An empty folder selection still includes no songs.

Selecting a source requests a background recursive refresh. The completed catalog remains usable during refresh and the existing periodic scan discovers newly added files. A new folder outside the indexed root still needs to be chosen in Library > Sources; selecting an unrelated NAS or drive does not silently replace the current library.

Verified with nested-folder and Windows-case regression tests, root selection, exclusion protection, outside-folder rejection and the user's actual NAS index: selecting its root includes all 4,405 songs recursively.
