# Visual direction from supplied references

The supplied AMP Verse mockups establish two related finishes: black/gold station-console and folder-browser chrome, and purple/silver classic-player chrome with green displays and orange/cyan DJ decks. The later AMP Lab mockup adds silver/graphite editor chrome; the Plugin Manager mockup uses the black/gold finish.

Implemented in the native Rust shell and embedded Cast UI, with drawn geometry and CSS gradients rather than screenshots pasted over controls. The subsequently supplied A/lightning/V icon is preserved in `assets/amp-verse-icon.png` and embedded in desktop window icons and Cast's browser/touch icon. No web frameworks, remote fonts or runtime downloads are required.

- **Base player:** labeled **AMP VERSE**, with a module dropdown in the topmost custom title bar. The dropdown opens/focuses AMP Verse, AMP Library or AMP DJ, and links to AMP Lab downloads and the local Cast UI. The bar includes drag, minimize, maximize and close controls, replacing the separate system decoration. Library and DJ retain their own visible labels and separate native windows. The compact purple chassis, green display and transport controls remain.
- **Library:** black/gold folder tree at left, searchable file table above the playlist panel at right, real preparation fields and file type. The user's folder/playlist requirement takes precedence over the older mockups' Crates entries. Library now expands beneath the compact Player transport; Detach restores a native window. Touching AMP windows can explicitly merge into one stacked layout.
- **DJ:** purple bezel, orange Deck A and cyan Deck B vinyl decoration, green loaded-track text, actual prepared cue data and working play/stop controls. Platters move only while the corresponding playback process is active. No invented waveform, output meter, sync, pitch or crossfader controls are shown.
- **Cast:** black/gold console, station rail, broadcast toolbar, current track panel, real station playlist and listener count. This milestone has one configured station; the reference's multiple named stations, artwork, sample songs, 42 listeners, meters and schedules are not fabricated. The tower emblem is a drawn station icon, not album artwork. Output format reflects the real 128 kbps encoder.
- **Android:** the compact integrated mobile reference is retained as design direction in the architecture. No Android implementation is implied by this desktop/web styling work.

- **Plugin Manager:** a real offline component catalog follows the black/gold reference with sidebar filtering, search, a component list, details and included-component summary. Included versions come from the compiled package. Cast executable presence is checked separately; it is never equated with a running station or installed plugin. AMP Lab connects an explicitly selected external editor, with manual GitHub download links. No fictitious release sizes, available-update counts, pack listings or installed badges are copied from the mockup.
- **AMP Lab target:** preserve the silver/graphite editor layout, visible AMP Lab label, green decoded waveform, original/processed preview, cue/trim bounds, processing controls and export panel. The multichannel section must eventually show real loaded audio channels/stems, with functional mute/solo/gain and rendered-audio validation. The existing MP3 Lab editor supplies a separate-process starting point, not proof that the new target controls, analysis, configurable normalization or multichannel engine exist. Its repository stays read-only in this project. No new AMP Lab editor window is shipped by this change.

`AMP_SCREENSHOT` captures the real native UI. Set `AMP_SCREENSHOT_VIEW=player`, `library`, `dj`, or `addons` to render one surface as the root window for deterministic visual QA; ordinary launches use independent module viewports. CI captures all four surfaces. Screenshot mode never starts audio or fabricates a loaded song.

## Selectable appearance

Classic, Modern Light and Modern Dark are implemented across the native Player, Library, DJ and Plugin Manager, and the Cast browser console. Classic preserves the module finishes described above; modern themes use flat light or charcoal panels with readable text and gold accents. The top-bar dropdown includes Appearance, and the base player also exposes a visible selector. See [theme preferences and Android target](THEMES.md). CI captures all four native surfaces in all three themes using `AMP_THEME`.

## Cast playback web references

The supplied light/mobile and dark/desktop Cast references now guide the web UI: a prominent now-broadcasting card, playlist source, playback and synchronization cards, Auto DJ status, and playlist-editing guidance. Modern Light/Dark use purple accents; Classic retains retro purple/gold/green. Appearance and the optional admin password are under the top-right menu. The layout stacks cards on narrow screens.

The page uses the actual station icon and live server/track/listener state. Broadcast Start/Stop and this-device listen/pause/disconnect are functional. Listening and station transport are separate. No fabricated album artwork, spectrum, track duration, timeline, paired-device count or synchronized badge is displayed. The source is explicitly the single configured folder; named playlist switching, server soundcard routing and track-time sync remain future milestones. App setup links to the repository guide; no unsupported app-launch URL is invented.


## Native Cast reference

The latest dashboard reference is explicitly assigned to the built-in desktop AMP Cast module, regardless of the image's embedded "Web UI" caption. It is a native egui window opened through the topmost module dropdown, with left navigation, Now Broadcasting and Streaming Status panels, real server queue/cue data and Auto DJ status. It shares the three desktop themes. The separate browser playback page remains a different surface. Detailed listener geography, CPU/memory/uptime, track waveforms/artwork, seek, crossfade/rules, editable station profiles and native soundcard playback are unbuilt and are not fabricated.


## Interface ownership

The web UI is intentionally basic: live station/track status, listening on this device, basic broadcast Start/Stop, a collapsed read-only queue and local theme selection. It is not the full Cast management dashboard. Playlist creation/reordering, library management, cue editing, Auto DJ rules/mixing, schedules, routing and detailed station controls belong in AMP Verse desktop and the integrated Android app as implemented. The server owns execution and prepared state so it remains independent of both clients. Android is still unbuilt; this ownership does not imply its controls already exist. Do not mirror advanced desktop mockups or placeholder future-feature panels into the simple browser page.


## Current product decisions

- Application-wide themes share screens and behavior: Classic is metallic grey with beveled controls and green displays; Modern Light is bright/minimal; Modern Dark is charcoal with subtle accents. Earlier purple-first references are superseded by this Classic direction.
- The browser Cast surface stays minimal. Its Open App / Downloads action opens `https://github.com/gitwide-cloud/amp-verse/releases`.
- Desktop Plugin Manager is the home for downloading/managing add-ons. Current implementation is an offline catalog, manual download links and external-editor connection; automatic download/install/update management is still unbuilt. Android has an integrated feature set with no plugin loader.
- AMP Lab must extend/reuse the existing MP3 Lab editor foundation. Do not create a second independent editor implementation. Current integration launches an explicitly selected MP3 Lab installation; deeper editor integration/branding is future work. This does not authorize modifying the separate mp3-lab repository in this AMP Verse task.
- Use the Pixel 7 for real-device Android development/testing and leave AMP Verse installed once installation succeeds. A reported commit/local pass is not proof that CI or device tests passed; report actual results separately.


## Android compact reference follow-up

Keep AMP VERSE as the topmost dropdown brand. List AMP Player, AMP Library, AMP DJ, AMP Remote, AMP Lab and AMP Cast by name, with Settings retained. Version 0.1.2 restyles the integrated app with shared Classic bevels/black displays/green text and compact transport/queue panels. Modern themes retain the same screen structure/functions. Unbuilt module entry pages must identify their preview status rather than show fabricated working control sets. Android still has no desktop plugin loader.


## AMP Lab integrated editor status

Android 0.1.6 replaces the Lab placeholder with the existing MP3 Lab Android editor foundation, not a second newly created editor. Its proven waveform/trim/fades/gain/normalize/preview/export/session behavior is retained. The AMP VERSE dropdown and shared app theme preference integrate navigation/appearance. Desktop Lab still launches the existing independent installation. Earlier Android Lab-unbuilt notes are superseded for these editor features; multichannel/stems/analysis remain future work.


Android Player uses the compact reference layout as of 0.1.7. Module navigation and Settings live in the AMP VERSE title dropdown. Playback occupies one six-button transport row and one EQ / PL / add / radio row; the remaining space belongs to the queue. Theme selection stays in Settings and changes the shared views rather than creating separate screens.


Desktop Classic uses recessed black audio displays, purple gunmetal surfaces, double beveled frames and gold title rails. The AMP VERSE brand remains the module dropdown at the left; the module title is centered in the draggable header. Modern themes use the same windows and layouts with their own palette. DJ retains its two platters and loadable library; no dummy crossfader or meters are shown for unimplemented audio features.

All native AMP Verse module windows share magnetic title-bar dragging. Nearby edges attract within 16 logical desktop points; touching windows form a connected group that moves together. Hold Shift while starting a drag to detach one window; Shift also suppresses edge attraction. Closed and minimized windows are excluded. The separately launched MP3 Lab editor is outside this native viewport group. Global positioning is supported on Windows and X11; Wayland uses compositor dragging because it does not expose absolute window positions.


## Attached Library milestone

The supplied transport-and-library reference now drives the compact horizontal Player and attached folder Library. The shared layout retains Classic, Modern Light and Modern Dark. Every Rust module supports explicit magnet merge and detach; embedded title bars retain module names without duplicate native window controls. See LIBRARY.md for supported edits, shared Smart List recommendations and current layout limits.
