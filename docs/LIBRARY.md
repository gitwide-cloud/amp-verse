# Player, Library and Smart Lists

The desktop starts with a compact transport bar. Expand Library attaches the folder browser, song table, playlist editor and cue waveform below that bar. The module menu remains in the AMP Verse title. Collapse Library returns to the compact player.

## Merge and detach

Native windows still snap within 16 logical pixels. Touching windows expose a lit MAGNET button; pressing it moves the selected module into the neighboring window. The player can receive other modules through its own MAGNET button. Each attached module has a Detach button. Nested modules stay with their host when it detaches; cycles are rejected. Only the outer native header controls the operating-system window. Embedded headers cannot close or minimize the host. Merged groups use vertical scrolling if they exceed the available desktop height. Layout persistence and edge-specific split orientations are not implemented; sections currently stack vertically. Wayland compositors that withhold global window positions cannot support edge detection reliably.

Supported modules: Player (root host), Library, DJ, native Lab arrangement, Cast and Plugin Manager. The external MP3 Lab editor runs in a separate process and is not mergeable.

## Folder and playlist workflow

Import a local or mounted NAS music root. The left side is the actual folder tree, never crates. Select a folder to filter the table, search by title, double-click to play, or use Mix checkboxes to build a playlist. Add folder includes descendants without adding duplicate tracks. Playlist Load/Save uses the existing root-relative JSON format. Up/Down and Remove edit the order. Smart Lists are regular playlists after ordering, so they remain portable and editable.

## Editing

Select a song and open Edit cues / BPM / key / rating / energy. Review and save IN/OUT, manual cues, tempo, beat-grid offset, musical and harmonic key labels, rating 0–5, energy 0–1, playback rate 0.84–1.16 and optional title/artist/album/genre/comment text. All are saved beside audio in the existing versioned `.amp.json` sidecar. Audio bytes and embedded ID3/Vorbis tags are not rewritten. Correcting a key label does not transpose audio. Saved speed is applied when loaded and changes audible pitch; key lock is not implemented. Trimming, destructive processing and rendered mix export belong to Lab.

## Shared recommendation engine

`amp_core::smart::order` takes preparation records, keeps the first song fixed, and greedily chooses the next song with the lowest weighted tempo, harmonic and energy-continuity cost. The first song's energy adds a penalty against energy drift. Camelot neighbors on the same ring and relative major/minor pairs are preferred. Unknown dimensions receive an explicit penalty. Ordering preserves every input occurrence, including repeats, and is deterministic for tied scores. The desktop caps ordering at 1,000 entries.

`amp_core::smart::curate` expands a seed set from the imported library. Candidates need known energy, tempo and harmonic code, lie within ±6% tempo and ±0.2 energy of the first seed, and take compatible harmonic steps from the preceding song. It adds at most 20 songs per desktop request, rejects unknown anchors and candidates, and stops when no suitable neighbor remains. It then orders the result using the same engine. Seeds are retained; repeated seeds are deduplicated during expansion.

New analysis estimates energy from aggregate RMS level using a bounded -48 to -6 dBFS mapping. This is a rough relative level proxy, not an assessment of genre, arrangement, perceived intensity or musical taste. Existing energy values are preserved, including manual corrections. Older analysis sidecars may have no energy until reviewed or reanalyzed. The current background analyzer process is not replaced during a Player-only installation.

No audio is beat-matched or rendered by Smart List creation itself. Prepared BPM/key estimates and chosen transitions still need review and listening.

## Normal playlist builder

New Playlist creates a blank saved playlist without capturing or replacing the playback queue. Use Add music to browse the indexed folders, including their descendants, or search by partial title, artist, album, genre, filename, or other displayed metadata. The general search and track search intersect; the folder search filters the folder navigation. Clear returns to all music.

Checkboxes select songs independently of playback. Add checked to queue and Add checked to playlist are explicit destinations; adding a folder includes its descendants. Repeated additions keep existing membership once, preserve order and save through the normal playlist store/sync path. The Library page also offers a saved-playlist destination menu. Originals stay in place.

Find Similar produces up to twenty review-only candidates using prepared mixing neighbors, then artist/genre matches where needed. Preview auditions a short excerpt without adding it; Add is required for membership. Preview is blocked while decks are playing so it cannot interrupt a live mix. Missing sources are reported explicitly and saving is blocked until reconnected; their saved paths are preserved.

## Deck Dislike

Dislike sits next to Stem settings, including when the EQ tab is selected. It targets the current deck and the playback playlist, never a list merely being browsed. Smart Playlists save an exclusion; normal playlists remove every occurrence of that song path while keeping other songs in order. Music files stay in place. Saving must succeed before playback changes. A successful click starts Mix Next immediately; if there is no playable replacement, the rejected deck stops. A valid incoming transition already in progress finishes without restarting. The deck clear X uses a red glyph at four times its normal button font size.
