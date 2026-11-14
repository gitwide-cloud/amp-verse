# Saved desktop library catalog

AMP Verse saves each completed scan in a separate device-local catalog per music root. Startup loads the saved tracks, preparation/analysis, folder tree, Smart Playlist rules and regular playlists without contacting the NAS. A background refresh merges the current library and repeats two minutes after each completed attempt. Manual Rescan remains available. Smart AutoMix can start from the catalog while a refresh is running, and future selections can use newly discovered matching songs.

A refresh keeps the current catalog visible. Index remapping preserves loaded deck preparation snapshots, queues and pending/preloaded tracks. Failed/interrupted scans keep the last successful catalog. Catalog writes use validated temporary files and atomic replacement; malformed or mismatched catalogs fall back to a new scan. Catalogs are local startup indexes, not a replacement for the shared metadata revision/lease system.

The first run of this version needs one successful scan to create its catalog. Refresh currently walks the library in a background worker and publishes a completed snapshot; it is not a filesystem watcher. Cached entries do not make missing NAS audio available offline. Offline audio still requires a downloaded/cached copy.

## Durable startup library index

The desktop stores its main library path and per-library additional source choices in a device-local `ampdj/library-index.ampdb`. On Windows this is under the user’s normal Roaming/AMPVerse folder, independently of the preview profile. Boot reads the index before the legacy text preference and does not require the NAS to be online. Existing text preferences remain a migration fallback. Empty paths cannot replace a saved main library. Explicit source removal can save an empty source list; a background scan with no sources does not erase the previous source backup.

Stop Analysis saves the main-path preference and only signals the analysis queue to stop after current work. It does not clear the queue, completed results, manual edits, library cache, or source choices. Regression coverage reopens a real queue after stopping and verifies completed and pending rows, as well as saved folder/cue records.
