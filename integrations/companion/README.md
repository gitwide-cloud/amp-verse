# AMP Verse Companion module

Install the packaged `ampverse-0.2.0.tgz` into a current Companion host supporting the Node 26 module runtime and API 2.1.3. This is a dedicated SDK module, not a generic HTTP preset collection.

1. In AMP Library, mark each desired regular or Smart Playlist **Available in Companion**, then save it. Sharing is off by default.
2. Add the AMP Verse connection in Companion. Choose the discovered Player/DJ instance and Cast server; manual addresses are supported. Passwords use the host secret field.
3. Use **Refresh available playlists**. On **Call playlist song / cue**, select a playlist and optional search/page, then press **Learn**. Choose the song and Learn again for named cues. Pages contain 50 songs. The normal picker never downloads the complete library.
4. Choose Load Only, Play Now, or Auto Fade and a specific deck or LIVE A/B. Load Only refuses to replace a playing deck.
5. Import the 24 deck transport/LIVE presets. Amber identifies A, cyan identifies B. Green feedback reflects actual playback, not a command being sent.

The six physical decks are A1/A2/A3/B1/B2/B3. LIVE focus does not change audio routing. Crossfade reassignment requires both the old and new pair paused and Auto DJ stopped. Background layers are excluded from Auto DJ transitions.

Actions also cover hot cues, loops, synchronization, Key Lock, gain, stem levels, filter presets, individual fades, emergency stops, energy/progression, Smart Playlists, requests, samplers, live broadcast, independent Cast stations, and Google Cast. Advanced output features require the corresponding host capability and prepared media. Unavailable media returns an error.

For Cast, refresh station playlists before selecting one. Runtime station energy/style changes apply on the next song selection without restarting current audio. Cast uses station/server identity fences; concurrent mix edits use revisions. Google Cast feedback reports receiver-confirmed state. Discover receivers and refresh action definitions before selecting one.

Network reconnect never starts/stops playback. Commands have unique request IDs and retries reuse the same ID. Old configuration sockets cannot update new connection state. Do not expose the control API directly to the Internet.

Development: `npm ci`, `npm test`, `npm run package`. Official SDK packaging and automated action/preset contract checks are included. A real Companion host button test is still required before claiming host-level acceptance.

Separate Cast passwords are supported. Recognition accepts recipient/team/milestone and a prepared song/cue. Music scenes retain local mix settings. Auto DJ Pause/Resume keeps its current song position and background layers; finish an active handoff first. Broadcast selects processed DJ master or Player/A1. Live variables cover all six decks, both pad banks, clock, energy/progression, receiver, broadcast and polled station state. Disconnection clears green playback feedback.
