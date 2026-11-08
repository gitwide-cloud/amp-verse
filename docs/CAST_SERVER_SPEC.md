# AMP⚡VERSE — AMP Cast Server, QNAP Edition & Google Cast
**Detailed Implementation Specification — Codex**

**Project:** AMP⚡VERSE<br>
**Platforms:** Windows, Linux, Android, QNAP Container Station, Docker<br>
**Primary language:** Rust<br>
**Distribution:** GitHub Releases / GitHub Container Registry<br>
**Design principles:** Lightweight, modular, LAN-first, offline-capable, KISS

---

# 1. Project Objective

Extend AMP⚡VERSE with a standalone, headless AMP Cast Server capable of:

1. Running continuously on compatible QNAP NAS hardware or other Linux/Windows systems.
2. Analyzing music libraries in the background.
3. Saving reusable BPM, beatgrid, musical key, energy, cue, phrase, waveform, and optional stem metadata.
4. Hosting multiple independent Auto DJ radio stations.
5. Broadcasting finished DJ mixes to AMP Player, Android, web browsers, and compatible receivers.
6. Supporting Google Cast devices, including compatible Chromecast, Google TV, and smart speakers.
7. Allowing authorized users to start, stop, and manage stations remotely.
8. Optionally playing a selected station through the server's own sound card.
9. Operating without a graphical desktop or an active DJ PC.

**Primary objective: the DJ PC must not need to remain powered on for background analysis or NAS-hosted radio playback.**

AMP Cast is an independent server component. It must not require the desktop AMP Player to be open.

---

# 2. Preserve the Existing AMP⚡VERSE Architecture

Do not restructure the existing application unnecessarily.

## Windows / Linux

**AMP Player — Base Application**

Includes:
- Classic Winamp-inspired player interface.
- AMP Library core.
- Shared audio playback engine.
- Basic playlists and playback.
- Network device discovery and remote control.
- AMP Cast radio listening.
- Google Cast sender support.

**Optional downloadable add-ons:**

| Add-on | Function |
|---|---|
| AMP Lab | Audio editor using existing MP3 Lab technology |
| AMP DJ | Multi-deck DJ workstation and intelligent Auto DJ |
| AMP Cast | Radio server, live audio and broadcasting |
| Additional plugins | Visualizers, effects, skins and extensions |

The shared audio engine remains part of AMP Library.

Add-ons should reuse that engine rather than including duplicate audio implementations.

## Android

Android remains an integrated lightweight application.

It must retain:
- Local music playback.
- Offline music library and saved playlists.
- Prepared Auto DJ playback.
- Precomputed stem support when downloaded.
- Manual local cue overrides.
- Lock-screen and background playback.
- Remote AMP Player and AMP Cast control.
- Radio station listening.
- Google Cast sender support.

**Do not introduce the desktop add-on system into Android.**

Android does not perform heavy BPM, key, or stem analysis.

## AMP Cast Server

AMP Cast Server uses the shared Rust library and analysis modules but has no dependency on the desktop GUI.

It must be deployable independently.

---

# 3. QNAP NAS Edition

## Deployment

Create an AMP Cast Docker image suitable for QNAP Container Station.

Target architectures:

- Linux AMD64
- Linux ARM64, where dependencies support it

Investigate ARMv7 only if the actual QNAP hardware requires it and all essential dependencies can support it.

Do not claim universal QNAP compatibility.

Inspect the hardware architecture, RAM, processor capabilities, and Container Station version before selecting a build.

## Packaging

Provide:

- Dockerfile
- Docker Compose configuration
- Persistent configuration volume
- Persistent application data volume
- Music-directory volume mapping
- Configurable network port
- Container health check
- Automatic restart policy
- Graceful shutdown
- GitHub-based release distribution

Publish container images through GitHub Container Registry.

Use versioned tags and a stable release channel.

## Installation

The deployment process should be straightforward:

1. Install or open QNAP Container Station.
2. Download the AMP Cast container.
3. Select the NAS music directory.
4. Select the persistent configuration directory.
5. Configure the web UI port.
6. Start the container.
7. Open the AMP Cast web interface.

Avoid requiring SSH access for ordinary configuration.

The same Docker deployment should also work on compatible Linux servers.

## Background operation

AMP Cast must:

- Run without an active desktop session.
- Automatically start after NAS reboot.
- Restore saved station configurations.
- Resume enabled persistent stations when configured.
- Resume incomplete analysis jobs.
- Retain metadata and settings across container updates.

---

# 4. Shared AMP Library Analysis Engine

The existing AMP Library audio engine becomes reusable by:

- AMP Player
- AMP DJ
- AMP Lab
- AMP Cast Server
- Headless analysis worker

The server must reuse the same analysis implementation as desktop AMP Library.

Do not build a separate, incompatible NAS analysis engine.

## Analysis capabilities

Analyze the music collection for:

| Data | Purpose |
|---|---|
| BPM | Tempo detection |
| Beatgrid | Beat-aligned mixing |
| Musical key | Harmonic compatibility |
| Camelot key | DJ-friendly harmonic notation |
| Overall energy | Song selection |
| Section energy | Dynamic mixing |
| Phrase boundaries | Musical transition timing |
| Intro/outro | Cue selection |
| Builds and drops | Hype mixing |
| Breakdowns | Dynamic variation |
| Suggested hot cues | Automatic DJ preparation |
| Waveform overview | Visual display |
| Stem references | Advanced prepared mixing |

Store confidence scores for uncertain results.

Do not treat detected BPM or section markers as automatically correct.

## Background analysis

Allow the user to point AMP Cast at a music directory.

The worker should:

1. Scan the directory and subdirectories.
2. Detect supported audio files.
3. Check for existing analysis metadata.
4. Analyze missing or outdated automatic data.
5. Save reusable results.
6. Continue with the next track.

Use incremental scanning so unchanged files are not repeatedly analyzed.

Support pause, resume, cancel, and configurable processing limits.

## Processing priority

Analysis must never interfere with active radio playback.

When a station is running, reduce analysis concurrency or temporarily pause heavy jobs when resources become constrained.

Provide settings:

- Analysis enabled/disabled
- Analyze new music automatically
- Maximum worker threads
- Pause heavy analysis during live playback
- Optional analysis schedule
- Stem generation enabled/disabled

Stem generation must not be required for ordinary music playback or Auto DJ.

---

# 5. Portable NAS Metadata

Preserve the existing `.ampverse` metadata design.

Example:

```text
Music/
├── Recognition/
├── Walk-In Music/
├── Everyday Music/
├── Background Music/
│
└── .ampverse/
    ├── library.json
    ├── playlists/
    ├── tracks/
    ├── beatgrids/
    ├── waveforms/
    ├── stems/
    ├── recommendations/
    └── analysis-state/
```

Use a documented, versioned metadata schema.

A local SQLite index may be used for performance, but the portable files must remain reusable across machines.

## Shared behavior

Desktop AMP Library and AMP Cast must recognize the same analysis data.

If a track has already been analyzed, another instance should reuse the saved results.

Support content identification so a renamed track does not necessarily require complete reanalysis.

Coordinate writes to avoid conflicting modifications from multiple computers.

## User-approved metadata

**Never automatically overwrite user-approved cues, beatgrids, or manual corrections.**

The analysis worker may propose changes.

Store suggestions separately.

Example:

```json
{
  "track_id": "example-track-id",
  "field": "cue_2",
  "existing_position_ms": 72000,
  "suggested_position_ms": 71840,
  "reason": "Detected phrase boundary",
  "status": "pending"
}
```

The user reviews these changes in AMP Library.

AMP DJ remains a performance workspace, not a metadata editor.

The AMP Cast Web UI should show analysis progress, but not provide full cue or beatgrid editing.

---

# 6. AMP Cast Radio Stations

A radio station is a **server-hosted Auto DJ playlist with one continuous playback timeline**.

A station does not restart every time a listener joins.

Each running station has:

- Playlist ID
- Station name
- Playback state
- Mixing style
- Energy target
- Current track
- Next prepared track
- Active decks
- Current transition
- Listener count
- Stream endpoint
- Optional local sound-card routing

## Station lifecycle

Support:

- Create station
- Save configuration
- Launch station
- Stop station
- Restart station
- Tune in
- Leave station
- Route to server sound card
- Remove server output
- View current playback state

Only active stations should consume audio-mixing resources.

## Multiple stations

Each station must maintain an independent:

- Auto DJ state
- Playlist
- Mixing timeline
- Track queue
- Energy progression
- Audio output stream

Multiple listeners of the same station share the same encoded broadcast.

Do not create an independent DJ engine for each listener.

Enforce configurable limits based on server resources.

---

# 7. Auto DJ Station Engine

Use the existing AMP DJ Smart Playlist specification.

The server's Auto DJ engine uses precomputed metadata to choose songs and transitions.

Support three styles.

## Hype

- Emphasize musical builds and drops.
- Use high-impact sections.
- Allow occasional intentional lulls.
- Prefer shorter, energetic song passages.
- Use musically appropriate high-impact transitions.

## Steady

- Maintain consistent musical energy.
- Avoid abrupt intensity changes.
- Favor compatible tempos and rhythmic structures.
- Use smooth, phrase-aligned transitions.

## Chill

- Play most or nearly all of each song.
- Preserve natural structure and breakdowns.
- Use subtle transitions.
- Avoid aggressive cue jumping.

## Progressive Energy

Allow gradual energy changes over a specified number of song transitions.

For example:

Energy 9 → 7 → 5 → 3.

The server selects music matching each target without abruptly interrupting playback.

## Smart selection

The engine handles:

- Song randomization
- BPM compatibility
- Harmonic compatibility
- Section selection
- Cue-in and cue-out
- Beatmatching
- Crossfading
- Recent-repeat avoidance
- Energy progression
- Fallback transitions

The web interface should not expose detailed DJ engineering controls.

Those belong in AMP Library and AMP DJ.

---

# 8. AMP Cast Web UI

The Web UI must be **modern, minimalist, responsive, and separate from the retro Winamp desktop interface**.

Follow the approved design direction:

- Modern light and dark themes.
- Clean typography.
- Minimal controls.
- Spacious layout.
- Mobile-friendly vertical arrangement.
- Wider landscape layout for desktop browsers.

Avoid dashboards full of analytics, effects, decks, and editing controls.

## Primary Station Screen

Display:

**AMP Cast**

Server status:
- Online / Offline
- Connected device count

Station selector:
- Choose an existing playlist/station.

Now Playing:
- Track name
- Artist
- Album artwork if available
- Playback state
- Mixing style
- Energy information

Primary controls:

| Control | Function |
|---|---|
| Select Playlist | Choose prepared Smart Playlist |
| Play | Launch/start the selected station |
| Stop | Stop station |
| Listen | Play the station in this browser |
| Play on Server | Route station to server sound card |
| Sync Across Devices | Manage supported receiver synchronization |
| Auto DJ | Enable/disable automatic mixing |
| Google Cast | Select a compatible Cast receiver |

Show station listeners and connected receivers in a compact secondary area.

## Editing restriction

Do not provide full playlist, cue, stem, or beatgrid editing in the Web UI.

Display a simple message:

"Manage playlists and music preparation in AMP Library."

## Admin Settings

Preserve the agreed simple password model.

- Optional admin password.
- Show/hide eye icon.
- Change password.
- Save.
- Local server administration can view the configured password.

Keep ordinary listening accessible without an admin login.

LAN-first operation.

External access through an independently configured tunnel.

Do not implement a complex account-management system.

---

# 9. Server Sound-Card Playback

Every running AMP Cast radio station can optionally be routed to the server's physical audio output.

## Behavior

Selecting **Play on Server** routes the already-running station to the designated sound card.

It must not:

- Restart the playlist.
- Create a second independent Auto DJ timeline.
- Interrupt existing radio listeners.
- Reset the current transition.

The server sound card becomes another output of the same station.

## Sound device configuration

Provide:

- Available audio devices
- Default output selection
- Master volume
- Output mute
- Station currently assigned to device

Initially, support one radio station per selected output.

Additional audio devices and independent outputs can be supported later.

## QNAP limitation

Some NAS models do not expose a usable audio output to containers.

If no supported sound device exists, disable **Play on Server** and show a clear explanation.

Normal radio broadcasting must continue to work.

---

# 10. Google Cast Integration

Add Google Cast capabilities to:

1. AMP Cast Web UI
2. AMP Player desktop
3. AMP Player Android

Google Cast is an **output destination**, not a replacement for AMP Cast radio streaming.

## Primary workflow

**QNAP → AMP Cast Radio → Google Cast Receiver**

The NAS hosts and mixes the music.

The receiver independently fetches and plays the radio stream.

The phone or browser acts as a remote controller.

Do not route the audio through the phone after starting a Cast session.

## Cast button

Add the standard Cast icon to relevant player and station interfaces.

When selected:

1. Discover available compatible Google Cast devices.
2. Show a device picker.
3. Allow selecting the target device.
4. Start playback of the chosen AMP Cast station.
5. Display the connected device.
6. Offer volume and disconnect controls.

## Supported destinations

Target compatible:

- Chromecast devices
- Google TV / Google Cast receivers
- Google Cast-enabled smart speakers
- Supported Google speaker groups

Use Google's existing speaker-group support rather than attempting independent speaker synchronization initially.

---

# 11. Google Cast Stream Delivery

Google Cast receivers should receive a conventional, supported media stream.

Do not require a proprietary AMP⚡VERSE receiver application for the first implementation.

## Preferred initial implementation

Use:

**HTTP Live Streaming (HLS) with AAC audio**

The AMP Cast engine produces the finished stereo master mix.

The streaming layer encodes that mix once per station and provides a compatible HLS output.

## Why HLS

- Supported by Google Cast receivers.
- Appropriate for continuously produced media.
- Standardized stream delivery.
- Can serve multiple listeners.
- Avoids requiring the Cast device to understand AMP-specific protocols.

Account for the buffering and latency introduced by HLS.

Google Cast is intended for convenient listening, not low-latency DJ headphone monitoring.

## Stream behavior

The stream must continue independently of the control interface.

Closing the web page or Android application must not stop the radio station.

Cast receivers should reconnect or report failures cleanly.

Include:

- Correct content types
- Valid HLS playlist responses
- Continuously updated media segments
- Configurable buffering
- Current-track metadata
- Appropriate CORS behavior
- Explicit stream error reporting

Use an authenticated or unguessable stream URL when a particular listening session requires access restriction.

## Receiver compatibility

Start with Google's Default Media Receiver when its capabilities are sufficient.

A custom receiver should be optional and deferred until genuinely needed.

Do not require a paid Cast developer registration merely to deliver the initial standard-media experience if the default receiver satisfies the requirements.

---

# 12. Google Cast From the Web UI

Use Google's official Web Sender SDK.

The Web UI must:

- Display the Cast button on supported browsers.
- Discover available compatible receivers.
- Allow connecting and disconnecting.
- Show current receiver status.
- Allow volume control.
- Start the selected AMP Cast station on the receiver.

## HTTPS requirement

The browser Cast sender must operate from a compatible secure web origin.

A plain HTTP LAN dashboard may still work for ordinary browsing and playback, but browser-based Google Cast may require a separate HTTPS origin.

Do not silently assume the Cast SDK works on every browser.

Implement capability detection.

If unavailable, hide or disable the Cast button with an understandable explanation.

## Server stream reachability

The Google Cast receiver must be able to reach the media URL independently.

A radio stream accessible only through the controlling browser's private tunnel may not be reachable by the Chromecast.

Resolve the stream endpoint from the receiver's network perspective.

Do not pass `localhost` or an inaccessible private address as the Cast media URL.

---

# 13. Google Cast From Android

Use the official Google Cast Android Sender SDK.

Integrate Cast controls into the existing AMP⚡VERSE Android application.

## Local Player

When listening to an AMP Cast station:

- Show Cast button.
- Select receiver.
- Transfer playback to the receiver.
- Continue controlling the station.
- Display receiver status.
- Disconnect or return to local playback.

## Offline music

Prepared offline music and local Auto DJ must remain fully functional without internet or a server connection.

Casting locally stored music requires a receiver-accessible stream source.

Do not assume the Cast device can directly read Android's private offline music files.

For the initial milestone, prioritize casting NAS-hosted AMP Cast stations.

Local offline Auto DJ casting can be implemented later through a supported local streaming bridge.

## Android compatibility

Retain:

- Background playback
- MediaSession controls
- Lock-screen controls
- Notification mini-player
- Offline playlist storage
- Manual cue editing
- Lightweight prepared Auto DJ

Adding Google Cast must not require heavyweight music-analysis code in the APK.

---

# 14. Google Cast From Desktop AMP Player

Provide Cast support in the Windows/Linux player.

The interface should include:

- Device discovery
- Cast device selection
- Current connection status
- Volume control
- Disconnect

Primary milestone: cast an AMP Cast radio stream.

Investigate native sender implementations or an appropriate supported integration rather than assuming the Android or Web Sender SDK can be embedded unchanged in Rust.

If native desktop sender integration proves too complex, allow an explicit browser-based Cast workflow as an initial fallback.

Do not introduce a heavyweight browser runtime solely for Google Cast without evaluating its resource cost.

---

# 15. Cast Handoff

Support transferring listening from AMP Player or the Web UI to a Google Cast device.

Example:

1. User is listening to Home Radio on Android.
2. User taps Cast.
3. Google Cast receiver is selected.
4. Receiver loads the current live radio stream.
5. Phone becomes the remote.
6. The server continues mixing without restarting.

The radio itself must continue uninterrupted.

Receiver startup may involve a short buffer delay; do not promise instantaneous or sample-accurate handoff.

## Return to local playback

When the user disconnects Cast:

- Stop or release the receiver session as appropriate.
- Allow the phone or browser to resume listening to the current live station.
- Do not restart the Auto DJ playlist.

---

# 16. Sync Across Devices

Keep synchronized playback separate from the normal Google Cast button.

## Initial behavior

Provide a **Sync Across Devices** control for supported AMP Cast receivers.

Users can:

- View connected AMP receivers.
- Select receivers that should play the same station.
- Join the existing station timeline.
- Remove receivers from the group.

## Important distinction

Ordinary radio listeners share the same program timeline but may hear it at different moments due to buffering.

Actual synchronized audio output requires clock alignment and coordinated buffering.

Implement the two concepts separately.

### AMP Receiver Sync

For AMP-to-AMP receivers, design:

- Shared station timestamps
- Receiver clock-offset estimation
- Target playback latency
- Buffer alignment
- Drift correction
- Receiver health monitoring

### Google Cast Sync

For Google Cast devices, prefer supported Google speaker-group functionality.

Do not promise sample-accurate synchronization between arbitrary AMP receivers and Google Cast devices without testing and a dedicated synchronization mechanism.

## Graceful fallback

If accurate synchronization is unavailable, offer ordinary radio listening rather than presenting unsynchronized playback as synchronized.

---

# 17. Live Audio and Live Cast

Preserve AMP Cast's existing Live Cast concept.

The server may receive:

- AMP DJ master mix
- Compatible external audio input
- Network audio stream
- Designated mixer output

The server can then broadcast that source to listeners or Google Cast devices.

## Station modes

**Auto DJ:** Server automatically mixes a prepared playlist.

**Live:** Server broadcasts an external audio source.

**Hybrid:** Server can switch between Auto DJ and a live source.

## Hybrid behavior

When Live Cast begins:

1. Prepare the incoming live audio stream.
2. Verify the stream is available.
3. Switch using a controlled transition.
4. Continue broadcasting through the same station identity.
5. Preserve connected listeners.

When the live source ends, resume Auto DJ when configured.

Do not treat ordinary microphone silence as proof that a live performance has ended.

## Compatibility

Preserve the existing optional AMP Cast DualPath concept for redundant Wi-Fi/Ethernet audio transport.

Do not introduce network bonding or virtual network adapters.

DualPath belongs to the AMP Cast audio protocol only.

---

# 18. Network Discovery and Remote Control

All AMP⚡VERSE editions should use the existing shared network-control model.

## Discovery

Allow AMP Player and Android to discover available LAN instances.

Display:

- Device name
- Host type
- IP address
- Server/player version
- Available capabilities
- Connection status

## Connection

Workflow:

1. Search network.
2. Select AMP instance.
3. Enter optional password.
4. Connect.
5. Receive current state.
6. Display supported controls.

Remote clients must not assume every instance has the same add-ons installed.

## API

Reuse existing control transport where possible:

- HTTP for commands
- WebSocket for live status
- OSC for compatible show-control workflows
- MIDI mapping for hardware
- Companion integration

Avoid creating a separate incompatible API solely for QNAP.

Provide clear capabilities and version information.

---

# 19. Companion Integration

AMP Cast must remain controllable through Bitfocus Companion.

Required station actions:

| Action | Function |
|---|---|
| Launch Station | Start a saved radio station |
| Stop Station | End the station broadcast |
| Load Smart Playlist | Select prepared music |
| Start Auto DJ | Activate automatic mixing |
| Stop Auto DJ | Disable automatic mixing |
| Set Mixing Style | Hype / Steady / Chill |
| Set Energy | Change target energy |
| Progressive Energy | Start a gradual energy change |
| Play on Server | Route to physical sound output |
| Remove Server Output | Disable physical output |
| Start Live Cast | Switch to incoming live audio |
| Return to Auto DJ | Resume server-generated music |
| Fade In / Fade Out | Control broadcast or local output level |
| Duck / Restore | Lower and recover audio level |

Expose status feedback:

- Server connection
- Station running
- Current track
- Next track
- Playlist
- Mixing style
- Energy
- Listener count
- Local audio routing
- Live Cast state
- Cast receiver status

Do not require Companion to handle beatmatching, cue calculations, or music selection.

The engine handles those internally.

Preserve the established office architecture:

**Node-RED → OnTime → Companion → AMP⚡VERSE**

Node-RED remains the meeting secretary, not a DJ controller.

---

# 20. Web UI Information Architecture

Use a minimal navigation model.

## Radio

Default landing page.

Contains:
- Station selector
- Now Playing
- Play / Stop
- Listen
- Play on Server
- Google Cast
- Auto DJ toggle

## Devices

Contains:
- Connected AMP receivers
- Google Cast receiver selection where supported
- Sync controls
- Receiver status

## Library

Contains:
- Music directory
- Scan status
- Analysis progress
- Pending recommendations count
- Worker status
- Pause / Resume Analysis

Do not put permanent cue editing here.

Use AMP Library desktop for review and approval.

## Settings

Contains:
- Optional admin password with show/hide eye
- Music path
- Audio output
- Analysis worker limits
- Station behavior
- Network port
- Theme
- Server version
- Update status

Keep this interface functional and uncluttered.

---

# 21. Two NAS Deployments

Support separate AMP Cast installations.

## Office QNAP

Primary responsibilities:

- Background library analysis.
- Shared metadata creation.
- Optional office radio stations.
- Hosting prepared Smart Playlists.
- Supplying audio to an eligible office receiver.

The office DJ PC can be powered down when not required for its other AV responsibilities.

## Home QNAP

Primary responsibilities:

- Personal Auto DJ radio stations.
- Background music analysis.
- Browser/Android radio listening.
- Google Cast output to compatible home devices.

Home and office servers must remain independently configurable.

Do not automatically bridge the office and home networks.

If the home QNAP remains isolated for other software workloads, preserve that isolation; do not grant it unnecessary access to personal NAS storage or office systems.

Expose only deliberately configured music sources and network endpoints.

---

# 22. Performance and Resource Efficiency

The server should be lightweight when idle.

## Design rules

1. Rust-based core services.
2. No desktop GUI dependencies.
3. Shared decoded-audio and metadata implementations.
4. Load mixing engines only for active stations.
5. Encode once per station where practical.
6. Reuse broadcast output for multiple listeners.
7. Background analysis has configurable limits.
8. Heavy stem generation is optional.
9. Avoid unnecessary periodic scanning.
10. Close unused decoders and release buffers.

Do not claim a fixed memory or CPU target until real benchmarks are available.

## Resource monitoring

Provide minimal diagnostics:

- CPU usage
- Memory usage
- Active stations
- Active listeners
- Analysis worker activity
- Stream health
- Encoder failures

Keep detailed diagnostics in a secondary status or logs view rather than filling the radio dashboard.

---

# 23. GitHub Updates and Distribution

GitHub remains the official distribution source.

Do not create a proprietary add-on marketplace backend.

## Release artifacts

Publish suitable assets for:

- Windows AMP Player
- Linux AMP Player
- Android APK
- AMP Lab add-on
- AMP DJ add-on
- AMP Cast add-on
- AMP Cast Docker images

Use GitHub Releases for versioned downloads and GitHub Container Registry for Docker images.

## QNAP updates

The server should be able to check release information.

The web UI may display:

- Current version
- Available version
- Release notes
- Update instructions

Updates must preserve:

- Music directory mappings
- Metadata
- Playlist configurations
- Station definitions
- Credentials/settings
- Persistent application data

Do not restart containers or stations automatically in the middle of active broadcasts.

Use a controlled maintenance or update procedure.

## Repository separation

Maintain the existing project boundaries.

- AMP⚡VERSE remains its own GitHub project.
- MP3 Lab remains independent.
- Existing MP3 Lab technology may be reused for AMP Lab.
- Do not alter the original MP3 Lab repository without a separate explicit request.

---

# 24. Development Phases

## Phase 1 — Headless Server Foundation

Implement:

- Shared Rust engine integration
- Headless AMP Cast executable
- Docker packaging
- Persistent config
- Music directory mapping
- Basic web UI
- Library discovery
- Background BPM analysis
- Portable metadata persistence

Verify on standard Linux first.

Then validate on available QNAP hardware.

## Phase 2 — Radio Stations

Implement:

- Station definitions
- Saved playlist loading
- Independent Auto DJ sessions
- Broadcast output
- Browser listening
- AMP Player listening
- Station start/stop
- Server sound-card routing

Use safe crossfade behavior before adding advanced intelligent mixing.

## Phase 3 — Smart Radio

Implement:

- Hype / Steady / Chill
- Section-aware cueing
- Beat-aligned transitions
- Harmonic compatibility
- Energy-aware song selection
- Recent-repeat avoidance
- Progressive Energy
- Metadata suggestion workflow

## Phase 4 — Google Cast

Implement:

- Cast-compatible streaming endpoint
- Android sender integration
- Web Sender integration
- Receiver discovery
- Cast connection status
- Handoff from local listening
- Playback control and disconnect
- Desktop sender or browser fallback

Test actual receiver compatibility.

## Phase 5 — Advanced Network Audio

Implement:

- Live Cast
- Hybrid Auto DJ/live takeover
- AMP receiver synchronization
- Optional DualPath
- Multi-output routing
- Extended Companion control

Keep advanced networking separate from the core station engine.

---

# 25. Required Testing

## QNAP

- Container installs on supported CPU architecture.
- Server starts automatically.
- Music directory mounts correctly.
- Analysis survives restart.
- Metadata survives container replacement.
- Multiple stations can run within measured resource limits.
- No GUI is required.

## Analysis

- Existing metadata is reused.
- Changed tracks are detected.
- Manual cues remain protected.
- Suggestions require approval before replacing user values.
- Background analysis does not interrupt playback.

## Radio

- Launching a station begins Auto DJ.
- Second listener joins the current station timeline.
- Leaving a station does not stop it.
- Closing browser does not stop the broadcast.
- Separate stations retain independent playback state.
- Disconnected clients can reconnect.

## Google Cast

- Compatible device is discovered.
- Receiver loads the stream directly.
- Phone/browser can control playback.
- Closing the control interface does not terminate the radio.
- Cast disconnection does not stop the server station.
- Receiver stream URL is reachable.
- HTTPS Web Sender requirements are met.
- Unsupported browsers show appropriate fallback behavior.
- Receiver interruptions are reported accurately.

## Audio output

- Play on Server uses the intended sound card.
- Routing does not restart the station.
- Removing local output does not disconnect listeners.
- Unsupported sound cards are handled gracefully.

## Companion

- Station controls execute correctly.
- Feedback reports actual state.
- Network reconnect does not duplicate commands.
- Existing OnTime and Node-RED roles remain unchanged.

## Performance

Benchmark:

- Idle server
- Library scan
- Background BPM analysis
- One active station
- Multiple active stations
- Multiple listeners
- Google Cast active
- Analysis while broadcasting

Publish measured results and hardware specifications.

---

# 26. Final Acceptance Criteria

The completed system should support this workflow:

**Scenario A — Home Radio**

1. QNAP starts AMP Cast automatically.
2. User opens the Web UI.
3. User selects a prepared Smart Playlist.
4. User presses Play.
5. AMP Cast begins mixing.
6. User opens AMP Player on Android.
7. User tunes into the same station.
8. User selects a Google Cast speaker.
9. Speaker plays the radio directly from the NAS.
10. Phone becomes the remote.
11. Station continues after the phone screen locks.

**Scenario B — Office Analysis**

1. Office QNAP starts the analysis service.
2. User points it to the music folder.
3. New music is discovered.
4. BPM, key, energy and recommended cues are generated.
5. Metadata is saved alongside the music.
6. DJ PC starts later.
7. AMP Library immediately recognizes existing analysis.
8. User reviews any proposed changes.
9. Approved metadata becomes available to AMP DJ.
10. No unnecessary reanalysis occurs.

**Scenario C — Office Radio**

1. AMP Cast launches a prepared Smart Playlist.
2. Auto DJ produces a continuous mixed broadcast.
3. Companion can change the energy or mixing style.
4. An AMP receiver joins the station.
5. The station can optionally be routed through an available server sound card.
6. Closing the web dashboard does not stop playback.

---

# 27. Instructions to Codex

Inspect the existing AMP⚡VERSE repository before making architectural changes.

Reuse working shared Rust audio and control components wherever possible.

Do not rewrite functional modules unnecessarily.

Maintain separation between AMP Player, AMP Library, AMP DJ, AMP Lab, and AMP Cast.

Prioritize working code and passing tests over decorative interfaces.

Implement independent milestones, commit changes to the AMP⚡VERSE GitHub project, and provide build artifacts through the established GitHub pipeline.

Document:

- Implemented functionality
- Remaining functionality
- Test results
- Known limitations
- Hardware compatibility
- Resource benchmarks
- Blockers

Do not present unimplemented features as complete.

Do not modify the separate MP3 Lab project.

## Guiding Principle

**AMP Library prepares the music.**

**AMP DJ performs the music.**

**AMP Cast hosts and distributes the music.**

**AMP Player listens and controls.**

**Google Cast delivers the music to compatible speakers and TVs.**

Keep the entire ecosystem lightweight, modular, network-first, and usable without mandatory cloud accounts or subscriptions.
