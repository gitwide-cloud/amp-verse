# Local file databases

No database server, login or network service is required. These are standard embedded SQLite files, readable locally by AMP Verse and SQLite tools.

`music/ampdj/catalog.ampdb` holds song preparation, beat information, cues, default stem mixes, normal playlists and Smart Playlist rules. The `records` table preserves each document's field revisions and origin information. For example, `SELECT key, json_extract(document, '$.bpm') FROM records WHERE key LIKE 'tracks/%';` reads saved tempos.

In Library / Analyze Music / Portable catalog, **Consolidate catalog into one file** converts an existing portable metadata folder. Close other AMP versions first and stop playback and preparation. Active analysis claims refuse conversion; migration and publication locks coordinate upgraded workers. Imported values are preserved in the immutable `recovery_records` table before loose JSON is retired. Do not let older clients write to a converted library: they do not understand the database. Conversion does not alter source audio or an active loaded deck.

The headless equivalent is `amp-prepare MUSIC_FOLDER --compact`. Metadata updates keep their revision comparison and conflict detection. SQLite edits occur on a local temporary copy under the shared publication lock; the validated, synced snapshot replaces the shared file. Writable SQLite connections and WAL files are never opened on the NAS. Library scans read one database snapshot for the entire scan.

The desktop prepared-stems cache has a separate `prepared-stems/stems.ampdb` beside the player's local settings. New preparation stores the three audio parts and their checksums in that one archive, using bounded streaming and transactions. WAVs beside it are derived playback caches. Copy the archive to another installation's prepared-stems folder; Library / Edit song / **Restore stems from portable archive** restores matching audio by source checksum without running separation. Preparing the library again packs older cached stems while reusing their completed separation. This archive can be large; no stems are automatically uploaded or downloaded.

Android stores new local library preparation in `ampdj/catalog.ampdb`; each offline station download has a catalog containing its playlist and cues. Legacy JSON imports/downloads remain readable. Folder import also reads a desktop catalog database, so prepared BPM, keys, cues and analysis can travel with the music. Offline overrides stay local; reconnecting does not publish them.

Identity, transient leases/locks, explicit exports and recovery backups remain separate. The database conversion does not delete those protections or legacy backups automatically.
