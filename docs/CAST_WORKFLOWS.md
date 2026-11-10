# Cast: live DJ, remote playlists, Google speakers

## Broadcast the actual DJ mix

In AMP Cast → Broadcast, connect to the Cast server on this computer (or your selected LAN server), then press **Start live DJ broadcast**. This deliberately replaces that station's current radio source. It captures the player master output after mixing, crossfader, channel processing, stems and left sampler, plus the separate right sampler output. Headphone cue stays private. Other players listen to the station's existing `/listen.mp3` URL. Stop my live DJ broadcast returns the station to stopped; starting its playlist again is explicit.

The audio callback only writes into bounded rings. FFmpeg encoding and authenticated upload run off the callback. Network failures cannot block local playback. Upload sessions have ownership tokens; old senders cannot replace new sessions or stop another session. A disconnected sender expires after eight seconds. Live output requires Player/DJ and its selected Cast server to remain running.

## Play a computer's saved playlists from another player

Host Smart Station, or connect to an existing server and press **Share playlists on connected station**. This is opt-in and persists on that server. On another desktop, discover/connect to that server and press **Play here** beside a saved normal or Smart Playlist. Android uses **Play on this phone**. The server plays only the selected source and sends its stream to the listening player. Smart selection uses the shared energy/key/BPM ranking and saved source rules; normal playlists preserve their ordering. Each song reads its prepared cue IN/OUT once before playback. This is prepared sequential station playback; a seamless live DJ mix is the separate Broadcast mode above.

**Save offline** still copies original audio together with full preparation/cues. Offline edits never automatically overwrite the source library.

## Google Home / Chromecast

Desktop: start a station playlist or live DJ broadcast, then use **Find Google speakers → Cast here**. A speaker IP can also be entered. Discovery includes speaker groups. The controller translates a local loopback station address into the computer's LAN address before sending the MP3 stream URL. It keeps the Cast session alive, reports receiver playback status/errors, and offers Stop Google Cast.

Android: AMP Cast → **Google Home / Chromecast**, choose a speaker with the standard Cast button, then press **Play station on speaker**. The phone uses Google's sender SDK; no analysis engine is installed. The speaker reads the active station's stream directly. Stop casting ends the selected Cast session.

The speaker must be able to reach both the controlling device and the station server. Discovery alone does not establish reachability across isolated subnets. Google speakers cannot read `localhost` on a different device. A Cast receiver buffers live audio, so speaker playback is delayed relative to the local DJ output; it is not suitable as a DJ headphone/beatmatching monitor.

Server capabilities now advertise `live_dj` and `playlist_playback`. Google output is a sender feature of desktop/Android, not an autonomous headless-server controller, so the server does not advertise native `google_cast` support. This update does not enable Internet port forwarding or change network isolation.
