# Android Cast paths — candidate, not device accepted

Baseline c0e1a5d (feature/browser-server-preview). Branch codex/sag-47-android-cast. Coordinate with SAG-26; this is separate from desktop current-master candidate cc2f8b7.

The panel separates destinations receiving the host program from controls changing the host source. Opening, discovering and connecting must not start/stop audio. Source changes affect all listeners. Phone playback is not sent by the host-broadcast action. Android phone-source sending remains unimplemented; its disabled control says so.

Host status now advertises host_master_launch and server_output_control as command support, not hardware readiness. Missing flags disable those actions. Request-time errors still report missing desktop control endpoint, wrong password or output process failure. Server speaker command acknowledgement is not audible confirmation. Lost status/auth/identity verification disables mutations until a successful refresh or reconnect. Shared playlist mutations are disabled too. Chromecast forwards the host URL and does not imply server-native Google Cast support.

## Fixture regression matrix (not executed on Android)

Use generated audio and an authenticated private LAN test host with a distinct port/settings directory for destructive source/start/stop regression tests. Never run CastDeviceTest against OFFICECOMP: its fixture test intentionally starts/stops the station. The owner now requests a separate preservation-only Pixel 7 check against OFFICECOMP AMP Radio (below); this does not authorize rebinding/restarting the live server.

| Path | Required checks |
| --- | --- |
| Discovery | Dedicated test host advertised; select correct identity; no playback change; empty scan gives manual-address/binding guidance |
| Manual address | Valid LAN host; malformed/loopback URL rejected; unreachable/timeout/401 explicit; credentials not persisted/logged; wrong identity disables controls |
| Station to phone | Running host stream enables Listen; stopped/preparing source disabled; phone timeline is separate; stop phone does not stop station |
| Host desktop master | New flag permits request; old host flag absent disables; missing Player/control password failure preserved; accepted request waits for actual running stream; phone source unchanged |
| Server speakers | New flag required; stop output available independently from station running; missing ffplay/device not presented as audible success |
| Chromecast | Host stream only, running required; receiver must reach host LAN address; buffering and failed receiver connection checked separately |
| Phone-source sending | Disabled, explicit gap; no capture or fabricated stream; Player/DJ phone audio never silently replaced by host source |
| Stale host | Status failure disables source/destination mutations and shared-playlist controls; successful identity-checked refresh recovers |

Validation pending: Android full-flavor compilation and tests, mock endpoint regression execution, screenshots and real Android fixture acceptance, installable preview. No build/device pass claimed by this document. The server contract regression has passed as recorded below. Desktop cc2f8b7's 13 passes do not validate this Android patch.

## Review correction scope

Background polls preserve the last action notice and cannot unlock failed authentication/identity checks. Explicit reconnect or Refresh verifies the authenticated playlist endpoint before enabling commands. Discovery failures do not invalidate an unrelated host. Source replacement and delayed-dialog actions recheck the selected client; shared playlists no longer start phone playback. Stream destinations require on_air; preparing remains cancellable. User entry rejects loopback while direct CastClient mock fixtures remain supported. Discovery selection pins advertised identity. TV stop-listening only pauses the amp-radio item.

Added/updated Android instrumentation auth-persistence and address tests plus Rust command-auth/capability regression. Android tests have not yet executed. Full and standalone Cast now use the same CastPanel. FullCastPanel retains Full storage; standalone delegates phone playback/downloads to AMP Player. Source-level parity is not device acceptance. Chromecast receiver callback updates need SDK compilation and real-device verification. Builds must use CI or a permitted server toolchain, not the Windows PC. OFFICECOMP remains untouched.

Server regression executed after review corrections: `CARGO_BUILD_JOBS=2 cargo test -p amp-cast --lib --locked --offline android_host_commands_require_auth_and_advertise_support` passed (1 test, 0 failures; compile 1m52s). It checks anonymous mutation rejection and advertised command flags only. Android instrumentation, APK compilation, physical output and discovery on LAN remain unexecuted.

## Unified Full / standalone flow (latest owner direction)

1. Find nearby servers and choose a name. The selection dialog accepts the server password. Manual addresses stay under Advanced.
2. Read Server connection separately from the station on-air/preparing/idle state. In standalone, the independent AMP Player IPC status is above the panel. Missing/mismatched Player disables phone listening but leaves server controls available. The matching Player must advertise safe radio-stop support.
3. Select Listen on this phone, Play on server speakers, or Google Cast. Idle stations cannot send a nonexistent stream; missing command capabilities explain unavailable server output. Server speaker command support still does not prove a working physical device.
4. Source actions are collapsed separately. Desktop host Player/DJ master requests do not capture phone audio. Source-changing actions affect all listeners and are not part of the preservation-only OFFICECOMP check.
5. Disconnect drops the app's server selection and polling only. It leaves station and outputs running. Stop phone listening remains accessible after disconnect and only stops the radio media item, preserving unrelated music. Google Cast disconnect remains in receiver controls.

## Pixel 7 / OFFICECOMP acceptance record — pending

Capture before screenshots from the currently installed Full and Cast apps before replacement. Record their APK version and known commit (unknown if unavailable). Install matched signed Full/Player and standalone Cast artifacts built from the candidate commit; record CI URL, commit and APK hashes. No Windows PC builds.

With Pochiko / SAG-26, record OFFICECOMP station identity, source, running/on-air state, queue, output and listener baseline. If still bound to 127.0.0.1, record the LAN reachability failure; do not restart/rebind or interrupt library analysis to obtain a pass.

For each edition: discover and select named OFFICECOMP; verify wrong-password failure then correct-password access; compare station state and separate IPC status; listen on Pixel 7; stop phone listening; disconnect and reconnect. Confirm the station source, queue/session and existing outputs were preserved (timeline may advance normally). Do not start/stop the live station, choose another playlist, launch desktop broadcast, or replace existing receiver/server-speaker output as part of this preservation check. Test those mutations on the isolated fixture.

Capture after screenshots: named server selection, protected/failed access, connected station with three outputs, idle station (fixture), collapsed/expanded source actions, Player IPC unavailable while server remains reachable, and disconnected server. Report results separately for Full and standalone. No screenshots, tested APK commit or Pixel 7 pass are available yet.
