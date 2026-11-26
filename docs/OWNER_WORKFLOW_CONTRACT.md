# AMP Verse: agreed owner workflows

This reconciles Pedro's explicit requirements across the original Jarvis and OFFICECOMP chats. It is a product contract, not a declaration that these features are complete. Use it with the actual installation/test receipts. Keep engine work first, functionality next, and visual redesign last.

## One system
- One standalone server for Windows or Linux, usable headlessly with the full web UI.
- On Windows, a lightweight tray/settings agent and a self-contained window around that same web UI are requested. Closing the window keeps server playback running. Tray functionality is still pending; do not claim it exists.
- Desktop is the main manual DJ workstation, inspired by VirtualDJ/Mixxx; mobile is a lightweight player/DJ client. Library, Cast, DJ, Lab, Podcasts, and Bitfocus Companion remain modules, not separate unrelated products.
- Use one canonical installed app per role/device, preserving its data, signer, shortcuts, library identity, and rollback. Test updates replace that installation. Publish matching verified packages for the updater only after real acceptance.

## Server and library
- Server ingests/imports and organizes music, performs heavyweight analysis/stems, refines metadata with appropriate online sources, and maintains the shared analysis database.
- Desktop can analyze/write preparation data; mobile consumes analysis. Client playlist edits are supported and sync to the server; that is separate from writing analysis.
- Physical folder organization belongs to Library. Playlist editing never rearranges source files.
- Keep annotations for instrumental, a cappella, AI-generated, clips, and other relevant media kinds; do not treat uncertain identification as fact.

## Playlists and stations
- Normal and Smart playlists sync between server, desktop, and Android and replace DJ crates. Library browsing retains folders and filters across metadata.
- Normal playlists preserve saved order and repeated occurrences unless Shuffle is explicitly enabled. Missing references stay in saved data and have actionable handling.
- Smart playlists use analysis-based next-track selection for AutoMix. Only the server hosts persistent Smart stations.
- Desktop/mobile may send a live station. Each client sends or receives one audio program at a time; output ownership prevents competing playback and surprise resume.

## Casting is a server feature too
- The server must discover/manage Google Cast receivers and own the receiver sessions, controlled through its web UI and clients.
- Selecting server music or a server station and a speaker must work without keeping the desktop or a browser open. Closing a controller does not stop server-owned playback.
- Desktop/mobile can send or receive live audio; server can receive a laptop live DJ mix and output locally or through supported receivers.
- Add supported Google Home/Chromecast, DLNA and other transports honestly, with actual capability reporting. Do not promise unsupported formats/devices.
- Desktop-controlled Google Cast success DOES NOT satisfy server-managed casting. Current standalone server google_cast:false is a real incomplete requirement.

## Simple owner flows
- Discover named servers and speakers automatically; technical addresses stay in Advanced settings.
- OFFICECOMP uses the owner's explicit code-free private-LAN mode, retaining the scoped firewall. Preserve optional protected modes and unrelated installations' authentication.
- Select AMP Radio, open its Library, tap a song, and play immediately on the selected output. Streaming never silently creates an offline download or takes over another output.
- Distinguish This phone/browser, Server speakers, and a named Cast receiver. Stop applies to the selected output. Explicit Download and offline playback are separate actions.
- Bitfocus Companion can control music, clips, playlists, DJ and Cast on desktop/server. HA/Node-RED automation bridges belong on the server under Cast/Remote.

## Acceptance before calling it ready
1. Real Windows/Linux server and desktop builds pass relevant tests; installed source and hashes are recorded.
2. Pixel streams a selected server song immediately without an offline copy; explicit downloads and offline playback are separately tested.
3. Server output continues after controller/browser closes; Stop works. Google Cast is tested from the SERVER, with desktop closed, not merely from desktop UI.
4. Normal/Smart create, rename and edits sync across clients; saved order, duplicates, missing refs, opaque metadata and unsaved DJ state are preserved.
5. LAN disconnect/reconnect and decoder cleanup are tested; stable listeners and non-silent output are distinguished from human-confirmed audibility.
6. Publish/update one matched usable build set; keep incomplete capabilities visible and never label one-path success as completion of the entire build.

## Coordination and reports
Use Pochiko, Codex, Claude, and Gemini names in owner-facing updates. Codex does most implementation, Claude reviews, Gemini handles notes/coordination as requested. Give simple completed / still missing / needs owner action reports. Do not send Pedro between chats for routine decisions already authorized. If app delivery itself fails, record the exact blocker and keep progress possible through the shared handoff. Neither chat should invent a reduced architecture to close a ticket.
