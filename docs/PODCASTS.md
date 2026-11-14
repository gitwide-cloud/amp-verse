# AMP Podcasts

AMP Podcasts is an optional host-backed source-provider plugin. Its versioned `.ampplugin` package enables the podcast workspace in the AMP Player host; it is not a DLL, a separate audio player, or a second audio engine. The UI and host API are compiled into AMP Verse. New host behavior requires an AMP Verse update; package-only updates do not replace compiled Rust code.

## Everyday workflow

Open Podcasts from the application menu or workspace tab. Home shows listening history, managed downloads, local retention and subscriptions. Browse categories or search a title, publisher or topic. Open a show to inspect its description and publisher RSS episodes before subscribing. Listen to latest episode or Play / Resume streams publisher audio into AMP Player without subscribing or saving an offline copy. Player supplies transport, seek, volume, EQ and routing. Return to music queue is explicit.

Download saves an episode separately for offline playback. Downloaded episodes remain available after unsubscribing, including downloads from shows you never followed. Listening history and queue order are saved in the local media catalog. Podcast queues do not use Auto DJ fades, tempo changes or beatmatching. A completed episode advances to the next queued episode; previous/next also use the existing remote-control actions.

Show pages offer optional hourly latest-episode downloads. Subscriptions refresh one feed at a time, with cached details retained if a publisher fails. Browsing an already cached feed reuses it for five minutes; Refresh forces a request. Up to 200 available audio episodes are retained per fetched feed, and the details cache holds 20 shows. Long lists use the existing virtualized row support.

Settings contains download destination, themes, retention and advanced direct RSS browsing. Changing destination leaves previous downloads where they are; their individual paths are retained. Automatic cleanup is opt-in and runs while playback is idle. Pinned, loaded, changed and unmanaged files are protected. Retention dates are local cleanup policy, not publisher expiration. Ordinary RSS feeds usually do not supply reliable expiration dates.

## Discovery and dependencies

Search uses the public [Apple Search API](https://performance-partners.apple.com/search-api), behind a provider interface, and shows supported [Apple podcast categories](https://podcasters.apple.com/support/1691-apple-podcasts-categories). No credentials or self-hosted server are required. Requests are bounded and explicit searches are paced to at most one per three seconds. This integration does not invent charts, rankings or episode-search results. Search results are shows; RSS supplies episode descriptions, dates, duration and public audio enclosures. Catalog availability depends on Apple and each publisher.

The existing FFmpeg is reused. Because the bundled build lacks HTTP protocols, Rust transfers validated HTTP/HTTPS publisher audio into FFmpeg through a pipe. There is no second decoder or audio output. Seeking a remote pipe may need to fetch/discard earlier audio; downloaded episodes seek locally. Artwork is size-limited, decoded with image allocation limits, and cached on disk. Failed artwork is not retried each frame.

## Installation and updates

Plugin Manager lists AMP Podcasts with install/update, enable/disable, open and uninstall. GitHub downloads resolve one release and verify SHA-256 before atomically replacing the package. Manual `.ampplugin` installation also validates the package ID and host API. Uninstall removes only the installed package, preserving the local media catalog, subscriptions, progress and audio. The release workflow runs `tools/package_podcasts.py`, producing a versioned package and checksum for Windows/Linux hosts.

Data remains in the existing `media.ampdb` alongside AMP Verse preferences, with artwork in `PodcastArtwork` and a default `Podcasts` audio directory. No existing user subscription record is migrated destructively. Music sources are not changed or rescanned when a podcast starts.

## Verification and practical limits

Focused tests cover RSS metadata, independent show inspection, persisted listening/queue/download records, package validation, existing cancellation/collision/incomplete-download handling and safe retention. The opt-in `live_podcast_discovery_and_category` test checks real Apple discovery and publisher RSS. The opt-in `podcast_http_stream_uses_existing_decoder` test generates audio, serves it over local HTTP and verifies the existing muted engine advances playback.

Download cancellation removes incomplete files. HTTP range-resume/pause across application restarts is not implemented; retry restarts that episode. Provider-specific preview clips, ratings, reliable provider expiration and publisher archive pagination are not supplied by this integration. Google Cast depends on the existing output route; this plugin does not introduce another casting implementation. Native Android podcast browsing is not added by the desktop package.

## Build verification record (2026-11-13)

Windows host version: 1.0.17 Preview. Workspace tests and strict Clippy pass. Live search and category tests pass. Native muted HTTP-stream decoding passes with the installed FFmpeg that has no network protocols. The actual UI was exercised with live Woodworking and Technology directory results, publisher show/episode pages, a local TED Tech subscription, a completed 101.4 MB episode download, offline resume, listening history and a persisted podcast queue. These checks used an isolated profile and did not add the test subscriptions or downloads to the user's profile.

The desktop remote API returned the podcast title and position, accepted pause, seek +5 seconds and play, and reported playback advancing. Recorded test positions were 186.164 seconds before seeking and 191.164 afterwards. Native Android podcast browsing and a physical-phone remote session were not tested in this task.

Actual frame captures are under `test-output/podcasts-ui/screenshots/`. Classic Home, search, categories, show, episode, subscriptions, downloads and history were inspected; Modern Light and Modern Dark were checked during navigation and Home/settings rendering. A full automated 24-screen/theme matrix and Linux native rendering have not been completed. The existing source theme is reused; category illustrations from the supplied bitmap mockup are not copied into the app.

Download cancellation/collision/truncation and safe retention are covered by the existing local tests. HTTP range-resume, restartable partial downloads, a provider expiration integration and deep archive pagination remain limitations rather than placeholder buttons.

## Changed areas

- `apps/amp-player/src/podcasts.rs`: directory provider, RSS details, navigation, artwork, subscriptions, queues, downloads and listening persistence.
- `apps/amp-player/src/podcast_plugin.rs`, `addons.rs`: optional package validation, install/update, enable/disable and uninstall.
- `mixer.rs`, `main.rs`, `playback_owner.rs`, `player_sequence.rs`, `performance.rs`: existing-engine streaming, direct file handoff, progress flush, spoken-word isolation and remote feedback.
- `podcast_cache.rs`: managed download deletion, destination protection and local retention display.
- `workspace.rs`, `skin.rs`, `organized_library.rs`: module launch, dedicated workspace, compact Player seek controls and consistent theme background.
- `plugins/amp-podcasts/amp-podcasts.ampplugin`, `tools/package_podcasts.py`, `.github/workflows/build.yml`: versioned source-provider package and release assets.
- Root `Cargo.toml` / `Cargo.lock`: host version 1.0.17. No separate MP3 Lab repository was modified.
