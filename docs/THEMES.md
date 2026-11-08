# Selectable themes

The current desktop and Cast interfaces offer exactly three themes:

| Name | Portable ID | Appearance |
| --- | --- | --- |
| Classic | `classic` | Metallic grey chrome, beveled buttons, black/green displays and gold accents |
| Modern Light | `modern-light` | White/light-gray panels, dark text, dark gold accents and readable green status text |
| Modern Dark | `modern-dark` | Flat charcoal panels, light text, gold accents and subdued green status text |

Desktop: choose the theme from the main player's visible selector or **top-bar dropdown → Appearance** in any module. The choice updates the current desktop windows without restarting playback. It is saved to `%APPDATA%/AMPVerse/theme.txt` on Windows or `$XDG_CONFIG_HOME/amp-verse/theme.txt` (otherwise `~/.config/amp-verse/theme.txt`) on Linux. The setting is separate from playlists and audio preparation. Failed saves are reported in the selector while the chosen theme remains active for the session.

Cast: open the **top-right menu → Theme**. Modern web themes use purple accents from the supplied Cast references; Classic retains retro gold accents. The browser saves the choice locally under `amp-verse-theme`. It is a per-browser preference, independent of the server and desktop settings. No admin password is needed to change your appearance. Storage-unavailable browsers still allow session-only changes.

Theme changes affect appearance only, including custom panels, title bars, buttons, table labels and deck status text. They do not change the transport, station state, playlist or preparation data. Classic is the fallback for missing/unknown preferences.

For QA, `AMP_THEME=classic|modern-light|modern-dark` overrides the desktop startup preference; `AMP_VERSE_CONFIG_DIR` can point to an isolated settings folder. CI captures all five desktop surfaces in every theme. A preferences test checks saving/replacing/reading all three values and rejecting unknown IDs.

The supplied Android mockups define the integrated mobile direction. Android must expose these same three names/IDs and save its choice locally when that app is implemented. An Android APK has not been created by this theme change. Notification and lock-screen controls must follow platform media-session requirements; the mockup is not a claim that arbitrary system chrome is already customizable. When an APK is built, install it on the Pixel and leave it installed as requested.


All three themes are shared appearance settings over one interface implementation per platform, not separate products/screens. Keep functions and layouts the same wherever practical. Theme code may change palettes, bevels and contrast; it must not decide feature availability. The Android preview now builds with these same theme IDs; Pixel installation/testing is pending reconnection. Earlier Android target text above describes the original theme milestone, not the current APK build status.
