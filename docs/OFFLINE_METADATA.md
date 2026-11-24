# Shared audio metadata and optional DJ preparation

The opt-in sharing/authentication boundary and stable server, station, library and track keys are unchanged. No source files are modified by these endpoints.

| Endpoint | Contract |
| --- | --- |
| `GET /api/smart/tracks/{id}/basic` | Version1, existing server/station/library/track identity, title, original-file bytes and SHA-256, and a whitelist of ordinary catalog tags. Does not read a preparation record or its revision and does not return cues, analysis, models or stem data. |
| `GET /api/smart/tracks/{id}/preparation` | Explicit DJ fetch. Same response and revision/currentness semantics as the legacy bundled endpoint, including audio integrity and preparation. It does not download audio. |
| `GET /api/smart/tracks/{id}/metadata` | Legacy bundled contract remains unchanged for existing clients. |

Basic tags use the current server catalog snapshot (the same source used by the library): title, artist, album, album_artist, genre/genres, year/date, track/disc, comment, content_advisory and media_kind when present. This is an explicit whitelist, not the whole preparation/extensions document. Manual ordinary tag extensions take precedence (including intentional empty values). Otherwise known scanner `embedded_tags` Lofty keys provide artist/album/title/date/track/disc/comment; multi-values remain in order, joined with semicolons for display. Genres use the existing shared genre resolver, including `embedded_genres` and manual genre priority. No unknown tags or DJ-specific Lofty keys are projected. Snapshot freshness remains the normal library-refresh contract. Playlist membership and order remain in the playlist manifest; no new client `media_id` is minted by metadata fetches. `revision` still means the DJ preparation revision and is deliberately absent from `/basic`.

Android ordinary offline downloads fetch `/basic`. A compatible enabled embedded or signed AMP DJ adds an explicit `/preparation` request. The module download action accepts `preparation:false` for basic-only catch-up; asking for preparation without a compatible DJ stays basic-only. Servers predating these additive routes use the old `/metadata` only on HTTP404, not on malformed data, authorization failure or a server error. Such legacy servers necessarily still send their bundled response.

Offline integrity now lives in the item's top-level `sha256`, with read-only fallback to legacy `preparation.source_sha256`. Basic refreshes retain optional local preparation, its remote three-way-merge baseline and revision. Tags keep a separate `remote_tags` baseline. Later DJ preparation is merged using the existing conflict policy: local manual edits stay active and desktop proposals remain reviewable. Existing media IDs, playlist order, source identities and managed audio URIs are retained; unchanged local bytes are verified and reused. Removed membership remains retained by the existing non-destructive offline sync policy.

The actual `RadioOffline.save` path accepts explicit preparation mode, so saving the same source/playlist after adding AMP DJ catches up against existing verified audio. Ordinary `RadioOffline.items` does not require preparation and only attaches DJ extras when present. Private-library song downloads retain their existing compatibility behavior.

## Verification

- Rust regression drives actual authenticated sharing routes with a corrupt optional preparation record: `/basic` still returns integrity and no DJ fields; explicit preparation rejects corrupt data. Legacy and explicit DJ endpoint responses are compared, and basic identities match both.
- Pure Java `OfflineSongMetadataTest` covers legacy integrity migration, basic-only records, retained manual preparation and later three-way preparation merge.
- `RadioOfflineTest` exercises basic download, later preparation with zero extra audio requests, stable media ID and manual cue, then another basic refresh retaining the preparation baseline. `IncrementalSyncDeviceTest` retains the existing reindex/removal/conflict/failure/cancel coverage. Both are included in the emulator CI class list.
- A passing unit test or emulator result is not a claim that a physical phone was tested. Release evidence records CI and actual device acceptance separately.
