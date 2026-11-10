# Portable ampdj storage

Each music root now has one `ampdj` directory. Music files are never moved or rewritten.

- `ampdj/tracks/<relative music path>.json`: BPM, key, energy, cues, grids, loops, manual edits, analysis and revision data. Same-named songs in different albums stay distinct.
- `ampdj/library.json`: persistent shared library identity.
- `ampdj/smart-playlists.json`: all Smart Playlist rules and source selections for this library, in one JSON.
- `ampdj/playlists/`: ordinary playlists.
- `ampdj/legacy/`: original old metadata and recovery/conflict files, retained during conversion.

Desktop saves a copy of each library's Smart Playlist JSON to `Documents/ampdj/<Library ID>/smart-playlists.json`. Library IDs prevent one library from overwriting another's rules. `AMP_DOCUMENTS_DIR` can specify a redirected Documents directory. The music-root copy remains the shared, revision-protected authority; the Documents file is an export, never automatically uploaded on reconnection. Copy the music tree together with its `ampdj` folder to transport its metadata to another computer.

Rust desktop, the optional analysis add-on, headless Linux/Cast and Windows Cast share this storage implementation. Android offline station downloads write preparation into `ampdj/tracks`. Android Scan Folder reads preparation from `ampdj/tracks` when the selected folder is the music-library root and attaches it to imported audio. Android performs no analysis. Existing station manifests remain readable and preserve their embedded preparation.

For an existing library, close all old AMP installations and pause all analysis workers, then run `amp-prepare MUSIC_FOLDER --organize`. This command only reorganizes metadata; it does not analyze music. It refuses unexpired analysis claims. It retains the library ID, validates track records, does not overwrite an existing destination and archives old versions. If interrupted, it can be rerun. Old lock files remain in place to avoid splitting their lock domains; a small legacy identity fence stays at `.ampverse/library.json` to stop old versions from writing to the retired layout. Upgrade every writer before analysis resumes. Legacy adjacent sidecars and `.ampverse/tracks` remain readable before conversion.

Tests cover nested folders with duplicate names, unchanged audio, manual cue/BPM preservation, identity preservation, repeat conversion and active-worker rejection. Built Android APKs require installation on the phone; compiling an APK alone does not update an existing app.
