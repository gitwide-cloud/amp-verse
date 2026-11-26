# Server-owned Google Cast

The existing AMP Verse server owns speaker discovery and the Cast V2 TLS controller. A browser, phone or laptop only requests discovery/start/status/stop. Closing that client does not stop the server controller or its station source. This is a standalone-server update, not a second application.

## Workflow

In the server web player's Cast module, choose Find Google speakers, select a friendly speaker name, then Cast current server program. The server must already have an active playlist or broadcast. This action attaches an additional speaker output and preserves the current program (including the existing R&B station) and physical server speakers.

Stop Google speaker only detaches the selected Google output. It does not call station Stop or replace the server's saved playlists, library, prepared cues, or other outputs. To change the broadcast source, use the existing server playlist controls explicitly.

The selected speaker gets a fresh DefaultMediaReceiver session: any previous DMR is stopped and its absence confirmed before relaunch. A reused old session ID is rejected so a stale laptop controller cannot stop the new session by its cached ID.

The controller advertises a numeric private IPv4 address selected by the server's route to the discovered receiver. It rejects a loopback-only listener or an incompatible listener family. Discovery currently supports Google Cast IPv4 devices on the private LAN; multicast restrictions, host firewall rules and audio-format compatibility may prevent a real receiver from working. This feature changes no firewall rules and adds no listener.

## API and authority

- POST /api/google-cast/discover: bounded five-second mDNS discovery; returns opaque device IDs and friendly names. At most 64 devices are retained, for two minutes.
- POST /api/google-cast/start with {device_id}: only a currently discovered device can be selected. Arbitrary host/port fields are rejected.
- GET /api/google-cast/status: server_owned plus session state, receiver-reported player state, controller_active and confirmed_playing/confirmed_stopped.
- POST /api/google-cast/stop: requests stop for the owned receiver session; confirmation is reported separately. A new start waits until the previous controller has ended.

Control routes use the existing server authorization, optional configured access code/remembered browser session, server identity and cross-origin checks. Open trusted-LAN mode requires no code. No code, cookie or private-library URL is sent to Google speakers.

Google receivers fetch only the current station MP3 stream through a random, revocable capability URL. It grants no library, preparation, queue or control access. It is invalidated on stop, controller failure or replacement; an open response checks revocation while streaming. URLs/tokens and receiver IP addresses are excluded from ordinary API status and UI labels. The existing public station endpoint's behavior is unchanged.

Cast TLS uses the same self-signed receiver-certificate compatibility verifier as the native Cast implementation. TCP connect, TLS handshake, writes, launch, heartbeat and unconfirmed-stop waits are bounded. Message framing retains fragmented packets and rejects oversized input. Receiver PLAYING is reported only after a media-status response; it does not claim physical audibility. Media and receiver status are polled every five seconds. Stale media confirmation is cleared after 15 seconds; 30 seconds without fresh media state or with prolonged buffering fails actionably. Minor MP3 subscriber lag skips old chunks instead of closing the stream.

A server process restart does not automatically reconnect the speaker. This first implementation controls one receiver at a time; speaker groups are available only if advertised by Google as one Cast device. Native desktop/Android Cast controls still own their local controller until a separate client update; they are not automatically delegated to this server. Use the server web Cast controls for server-owned playback.

## Validation and installation handoff

Focused Linux/Windows CI builds the existing amp-cast binary and packages versioned archives with SHA256 files. Packages contain the binary, build.json with source/binary SHA256 and platform/version, and this document: no credentials, live configuration, music or test data. Install by replacing the existing server binary through the normal owner-managed update procedure, preserving its configuration and library identity. Do not launch a duplicate server installation.

Tests include a generated real TLS Cast receiver exercising LAUNCH/LOAD, fragmented frames, heartbeat and receiver-confirmed stop; authorization/open-LAN guards; numeric-address selection; revoked-stream denial; station/source/library noninterference; and browser controls that issue only server lifecycle requests. The OFFICECOMP FFplay PCM-demuxer option correction e226fa5 is included.

Generated fixtures are not Google hardware acceptance. Actual Google speaker audibility, Windows service-account multicast/device access, and playback after closing the real laptop require owner device QA with the matching binary. This task does not install or restart OFFICECOMP, change its running R&B program, or deploy another application.
