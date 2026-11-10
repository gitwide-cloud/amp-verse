# AMP Cast stations

The native Cast screen has three choices:

- **Find Stations:** search the LAN for AMP servers, or search broadcast radio by name (Radio Browser). Connect or Listen is the only required action. A manual address and optional access code are under the expandable connection section.
- **Broadcast:** start this computer's live MP3 station, or listen to a connected live station. Broadcasting and local listening are independent.
- **Host Smart Station:** explicitly publish saved normal playlists and saved Smart Playlist sources from the current library on the LAN. Starting sharing does not start broadcasting or analysis. No music is exposed until this option is started. It uses the existing shared server, not a second implementation.

Smart Stations provide a catalog and original audio downloads restricted to tracks that belong to shared playlists. Smart lists expose their eligible membership and saved rules; they are not pre-rendered mixes. The receiver saves originals and the complete portable preparation record: cue IN/OUT, numbered/manual hot cues, beatgrid, loops, key/BPM corrections, analysis and extension fields. Server, station and Library IDs travel with the copy. SHA-256 and length checks protect downloads; interrupted or changed-station downloads do not replace completed copies.

Desktop **Save offline** creates a new self-contained folder with audio, a playlist, and `.ampverse/tracks` metadata. Android **Save offline with my cues** uses separate app-private station storage, not the phone-folder library. Open **Offline playlists** to listen without a connection. Downloaded songs are also available to Android DJ; saved hot cues seed the local cue bank without overwriting existing local overrides.

Offline changes remain local. Nothing sends cue edits back to the server automatically. This update does not add remote metadata editing or automatic synchronization. Radio-directory streams are live listening sources, not downloadable media libraries.

Hosting uses LAN port 8787 and discovery port 18787. An existing server occupying that port is not stopped or replaced. Optional access codes use the existing server authorization. Server lifetime is independent of the player; the creating UI can Stop My Station. Host logs are in the device-local `cast-host/server.log` settings folder. The Cast server binary and audio tools are installed beside the player.

Protocol: `/api/smart/manifest`, `/api/smart/tracks/{id}/metadata`, and `/api/smart/tracks/{id}/audio`. Sharing is opt-in with `AMP_CAST_SMART_SHARING=1`; these routes reject access when sharing is off. Full analysis is disabled for UI-created stations. The existing minimal web listener remains separate from native library/DJ preparation.

Radio search API documentation: https://docs.radio-browser.info/.
