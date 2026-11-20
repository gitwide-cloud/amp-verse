# Android Cast paths — candidate, not device accepted

Baseline c0e1a5d (feature/browser-server-preview). Branch codex/sag-47-android-cast. Coordinate with SAG-26; this is separate from desktop current-master candidate cc2f8b7.

The panel separates destinations receiving the host program from controls changing the host source. Opening, discovering and connecting must not start/stop audio. Source changes affect all listeners. Phone playback is not sent by the host-broadcast action. Android phone-source sending remains unimplemented; its disabled control says so.

Host status now advertises host_master_launch and server_output_control as command support, not hardware readiness. Missing flags disable those actions. Request-time errors still report missing desktop control endpoint, wrong password or output process failure. Server speaker command acknowledgement is not audible confirmation. Lost status/auth/identity verification disables mutations until a successful refresh or reconnect. Shared playlist mutations are disabled too. Chromecast forwards the host URL and does not imply server-native Google Cast support.

## Isolated acceptance matrix (not executed)

Use generated audio and an authenticated private LAN test host with a distinct port/settings directory, not OFFICECOMP or production data. OFFICECOMP is loopback-only and analyzing; do not rebind/restart it. Keep real receiver output gated by the existing explicit test-window requirement.

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

Validation pending: Android full-flavor compilation and tests, mock endpoint regression execution, screenshots and real Android fixture acceptance, installable preview. No build/device pass claimed by this document. Server contract change has only formatting/diff validation until the serialized build lease is used. Desktop cc2f8b7's 13 passes do not validate this Android patch.

## Review correction scope

Background polls preserve the last action notice and cannot unlock failed authentication/identity checks. Explicit reconnect or Refresh verifies the authenticated playlist endpoint before enabling commands. Discovery failures do not invalidate an unrelated host. Source replacement and delayed-dialog actions recheck the selected client; shared playlists no longer start phone playback. Stream destinations require on_air; preparing remains cancellable. User entry rejects loopback while direct CastClient mock fixtures remain supported. Discovery selection pins advertised identity. TV stop-listening only pauses the amp-radio item.

Added/updated Android instrumentation auth-persistence and address tests plus Rust command-auth/capability regression. These tests have not yet executed. Lite + Cast add-on FeatureScreen remains outside this embedded full-flavor patch and does not yet have parity; no all-editions acceptance claim. Chromecast receiver callback updates need SDK compilation and real-device verification. Builds must use CI or a permitted server toolchain, not the Windows PC. OFFICECOMP remains untouched.

Server regression executed after review corrections: `CARGO_BUILD_JOBS=2 cargo test -p amp-cast --lib --locked --offline android_host_commands_require_auth_and_advertise_support` passed (1 test, 0 failures; compile 1m52s). It checks anonymous mutation rejection and advertised command flags only. Android instrumentation, APK compilation, physical output and discovery on LAN remain unexecuted.
