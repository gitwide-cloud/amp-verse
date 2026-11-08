# Visual direction from supplied references

The six user-supplied images establish two related finishes: black/gold station-console and folder-browser chrome, and purple/silver classic-player chrome with green displays and orange/cyan DJ decks.

Implemented in the native Rust shell and embedded Cast UI, with drawn geometry and CSS gradients rather than screenshots pasted over controls. The subsequently supplied A/lightning/V icon is preserved in `assets/amp-verse-icon.png` and embedded in desktop window icons and Cast's browser/touch icon. No web frameworks, remote fonts or runtime downloads are required.

- **Base player:** labeled **AMP VERSE**, with a module dropdown in the topmost custom title bar. The dropdown opens/focuses AMP Verse, AMP Library or AMP DJ, and links to AMP Lab downloads and the local Cast UI. The bar includes drag, minimize, maximize and close controls, replacing the separate system decoration. Library and DJ retain their own visible labels and separate native windows. The compact purple chassis, green display and transport controls remain.
- **Library:** black/gold folder tree at left, searchable file table above the playlist panel at right, real preparation fields and file type. The user's folder/playlist requirement takes precedence over the older mockups' Crates entries. No integrated player strip in this window.
- **DJ:** purple bezel, orange Deck A and cyan Deck B vinyl decoration, green loaded-track text, actual prepared cue data and working play/stop controls. Platters move only while the corresponding playback process is active. No invented waveform, output meter, sync, pitch or crossfader controls are shown.
- **Cast:** black/gold console, station rail, broadcast toolbar, current track panel, real station playlist and listener count. This milestone has one configured station; the reference's multiple named stations, artwork, sample songs, 42 listeners, meters and schedules are not fabricated. The tower emblem is a drawn station icon, not album artwork. Output format reflects the real 128 kbps encoder.
- **Android:** the compact integrated mobile reference is retained as design direction in the architecture. No Android implementation is implied by this desktop/web styling work.

`AMP_SCREENSHOT` captures the real native UI. Set `AMP_SCREENSHOT_VIEW=player`, `library`, or `dj` to render one surface as the root window for deterministic visual QA; ordinary launches use independent module viewports. CI captures all three surfaces. Screenshot mode never starts audio or fabricates a loaded song.
