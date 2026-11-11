# Portable Smart Playlists

Library > Smart Playlists now offers **Export Smart Playlist** and **Import Smart Playlist**.

Export writes one small `.ampsmart` file for the selected editing draft. Put it on a NAS or copy it to another computer. It includes the name, folder rules (including subfolders), allowed/excluded/preferred songs, recognition assignment, mixing style, target energy, repeat behavior, progression and manual-cue preference. It includes the originating library identity and a music-root hint. It excludes audio, analysis, stem files and shared revision tokens.

Import creates a new playlist in the local editing draft. Existing playlists are never replaced. Click **Save Playlist** to publish it for future DJ loads; current playback keeps its loaded settings.

When the loaded library has the same identity, its sources are connected automatically, including when the NAS is mounted at a different path. Relative music paths must remain unchanged. Normal background scanning finds the eligible songs.

If a different library is loaded, the complete settings are retained but the imported playlist cannot play unrelated music. While playback is stopped, **Connect original library and keep import** opens the exported root if it is accessible and has the expected identity. Alternatively choose the correct mounted library through Sources, import there, or explicitly select **Use current library with these folder rules** / choose a replacement Music Source. Review the resulting song count before saving. Missing folders remain empty; they never expand to the whole library.

The file is versioned and limited to 4 MiB. Invalid settings, unsupported versions and folders escaping the library root are rejected. Song selections use existing portable relative-path song IDs; moving or renaming individual songs requires remapping those selections. This is a settings exchange, not an offline music download. Android's file-import interface is not part of this desktop update.

Validation covers full-rule round trips, distinct IDs for repeated imports, unavailable sources surviving save/reload, identity matching after relocation, malformed/future packages, path traversal and size limits.
