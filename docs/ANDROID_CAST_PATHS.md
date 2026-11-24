# Android Cast paths — candidate, not device accepted

Baseline c0e1a5d (feature/browser-server-preview). Branch codex/sag-47-android-cast. Coordinate with SAG-26; this is separate from desktop current-master candidate cc2f8b7.

The panel separates destinations receiving the host program from controls changing the host source. Opening, discovering and connecting must not start/stop audio. Source changes affect all listeners. Phone playback is not sent by the host-broadcast action. Android phone-source sending remains unimplemented; its disabled control says so.

Host status now advertises host_master_launch and server_output_control as command support, not hardware readiness. Missing flags disable those actions. Request-time errors still report missing desktop control endpoint, wrong password or output process failure. Server speaker command acknowledgement is not audible confirmation. Lost status/identity verification disables mutations until a successful refresh or reconnect. Control/download authentication rejection opens password entry without disabling public listening. Shared playlist mutations are disabled too. Chromecast forwards the host URL and does not imply server-native Google Cast support.

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

## SAG-76 continuation

The five paths remain visible before connecting, with explicit headings and a connection prerequisite. Background refresh updates host state without replacing the last foreground action result; background failures still appear. A stopped host explains why phone listening and Google Cast are disabled. These changes need full-flavor Android compilation and device review.

### Reachability decision

Do not rebind or restart the installed OFFICECOMP instance while its analysis/playback is active. Its loopback listener is a deliberate safe default in `apps/amp-cast/src/main.rs`; LAN discovery is explicitly suppressed for loopback listeners. This explains the current installation, but is not proof of a working LAN path. Android reports that limitation during initial connection, failed discovery and connection failure.

The isolated fixture must use a new generated-audio directory, new `AMP_DEVICE_CONFIG_DIR` and `AMP_CAST_CONFIG_DIR`, `AMP_ANALYSIS_ENABLED=false`, a nonempty privately supplied `AMP_CAST_PASSWORD`, and `AMP_CAST_BIND=<private-LAN-IP>:<unused-port>`. Do not reuse production config, identities or credentials. Use default discovery UDP 18787 only if available; otherwise verify manual addressing independently and report discovery as untested. Test authenticated controls with both valid and invalid passwords. Never include secrets in reports.

### Reverse direction: concrete missing implementation

Android currently cannot transmit its own Player/DJ master to the server. `CastPanel` invokes `/api/live/launch`, which asks the remote desktop to send audio. `CastClient` supplies host listening URLs; it contains no live audio upload/session implementation. `AudioDsp.queueInput` processes PCM for local output and spectrum analysis, with no network sender. The server already has authenticated `/api/live/start`, `/api/live/audio`, `/api/live/stop` and an `x-amp-live-session` ownership token (`apps/amp-cast/src/live.rs`). The missing Android components are a current-master PCM tap including DJ routing, an encoder and bounded upload worker, and session ownership/cancellation/error handling that preserve local playback. Merely adding a button or posting `/api/live/launch` does not fill that gap.

### Evidence this continuation

- Source inspection and `git diff --check`: PASS.
- Java compiler / adb discovery: unavailable in this runtime; no APK compiled or installed.
- Second LAN device, authenticated fixture, real Android, physical output and preservation across device testing: NOT RUN.
- No live service, analysis queue, playlist, credential or music data was modified. Preservation during real testing remains unverified.
- Installable preview for this candidate: unavailable. Older artifacts are not evidence for this head.


## Review correction candidate (after 30ecdad)

Addressed source findings from SAG-79/SAG-80; this is a new uncompiled candidate, not device acceptance.

- H1: background polling has a separate flag and never sets foreground busy. Its responses are discarded when a newer user operation or host selection supersedes it. Stable playlist manifests no longer recreate buttons every poll. Late dialog actions recheck host identity, verification and foreground busy, with visible feedback rather than silent rejection.
- H2: desktop sender callbacks and server-speaker URLs use the actual bound interface; unspecified IPv4/IPv6 listeners use the matching loopback family. Added five address regression cases in `station_control.rs`; they have not run under a build lease.
- H3: shared-manifest 401/403 produces an admin-password prompt while public status/listening stay available. Command auth rejection no longer invalidates public status. Discovered hosts are selected for explicit Connect and password entry; credentials are not persisted.
- H4: destination readiness uses `on_air`, while `preparing` keeps Stop available and Start disabled.
- H5/H6: each path has adjacent prerequisites and feedback. Successful recovery clears stale connection errors; host error and server-output-off transitions are reported. Control POSTs refresh status immediately. Source and output-process state are displayed without claiming physical audibility. Phone/TV listen and radio-scoped stop give feedback, and phone player errors appear in the Cast path.
- H7: reconnect clears previous host state/lists. Google Cast registers receiver status callbacks, distinguishes buffering/playing/error, shows the station URL and direct host-to-speaker path, and detaches callbacks on pause/end. The picker uses the AMP light/dark palette's corresponding AppCompat theme. Actual icon contrast still needs screenshots.
- M1–M5/M8: disabled styling includes phone-send/shared controls; saved address settings remain visible; wildcard addresses are rejected; Internet radio results have a separate container; queue visibility follows sharing state; empty station lists are explained; starting a host playlist no longer starts phone playback before on-air readiness; Help explains all paths.
- M7 scope: these changes target the integrated full-flavor `CastPanel` and shared client/activity code. The standalone `android/modules/cast/FeatureScreen.java` companion retains its earlier UI and is not covered by this acceptance or an integrated APK. Do not publish or describe that companion as having these fixes.

Google Cast callback implementation follows the official RemoteMediaClient API: https://developers.google.com/android/reference/com/google/android/gms/cast/framework/media/RemoteMediaClient . Receiver-reported playback is not proof of audibility.

Validation: all changed/new Java files parse using javalang 0.13.0; `rustfmt --edition 2021 --check` and `git diff --check` pass. These are syntax/format checks only, not Java type checking or Android/Rust compilation. Address validation instrumentation was added. Existing playlist/offline mock fixtures were adjusted to a private IPv4 interface because production rejects loopback destinations; these remain same-device mock tests, not LAN acceptance. No automated tests were executed. Exact-head APK, authenticated isolated LAN fixture, physical Android/screenshots and preservation testing remain unavailable. A saved question to Pochiko requests the current serialized lease and authorized build/device access.


## Targeted follow-up to SAG-84

- R1/R2: HTTP command rejection has its own exception category; only typed host transport/identity/status-verification failures invalidate reachability. Internet radio, discovery and download failures do not invalidate the selected host. Foreground reachability failures record the affected feedback view, and background recovery updates that view as well as global status.
- R3/R4: shared actions use the admin-control gate. An auth prompt does not hide the public queue; search and queue visibility agree. A passwordless host returning sharing-disabled 403 is treated as unavailable sharing rather than a password prompt.
- R5: each foreground operation captures its feedback target. Output transitions use the server-speaker feedback, host program errors use source/desktop feedback, and sharing actions have an adjacent feedback view. Background status failures do not overwrite the last-clicked unrelated path.
- R6: accepted Google loads wait for asynchronous receiver status; they no longer immediately render stale cached state. The receiver URL guard also rejects wildcard/loopback forms accepted by neither the client nor a remote speaker.
- R7: the device flow uses the automatically visible saved-address controls, current labels, scroll-to-control helpers and explicit on-air readiness before Listen. Added a generated HTTP fixture regression separating 409/503 command rejection from closed-server reachability failure. These tests are source additions only and have not executed.

The existing build-access interaction remains pending; no duplicate access request, build, APK, device run or live-service change was made for this pass.


## Selective restoration on current integration (2026-11-20)

Restored the four interrupted SAG-76 fixes (`06aac8f`, `30ecdad`, `5a39c7e`, `1f44b49`) onto integration `bfbf886` without replacing its newer adaptive HLS playback, friendly discovery labels, discovery identity verification, saved identities, or standalone Cast connection fixes. Manual address/password fields remain behind Advanced connection settings, and the station playlist selector retains its Station controls disclosure. All five path headings and prerequisites remain visible before connection; unsupported or unverified actions remain disabled.

The restoration also prevents a second connection click from clearing the selected client while another operation is finishing. Tests for current disclosures were adjusted to open Advanced explicitly. Added a disconnected-path UI regression, and extended the generated HTTP fixture to cover authorization codes, stale host identity and retained HLS stream selection.

Validation of this restoration: `cargo test -p amp-cast --lib --locked` passed 29 tests with eight existing ignored tests; the bound-interface/IPv6 callback regression passed. All 14 changed/new Java sources parsed using javalang 0.13.0. Rust station-control formatting and `git diff --check` passed. Android assembly and instrumentation execution remain pending CI/device verification; parsing is not Android type checking.

Safe isolated Pixel checks: `CastAddressDeviceTest` uses no network; `CastFailureDeviceTest` creates a same-device private-IPv4 HTTP fixture (skips when no private interface exists); `CastPathsDeviceTest` opens only the Full Cast UI and checks disconnected controls/Advanced disclosure without network or playback commands. None needs `castUrl`. `CastDeviceTest` requires an explicitly supplied isolated generated-audio station and must not target an owner's live station.


### Friendly-label review follow-up

Removed the restored raw stream URL from the Google Cast source summary and removed the literal loopback address from the default disconnected status. The source summary now uses only the selected station's friendly title (with a fallback for a missing title). Actual stream URLs remain transport data. Manual address fields and explicit connection validation retain their relevant technical detail behind Advanced. The disconnected-path regression also rejects URLs and loopback addresses in visible default labels. No receiver load or playback command was added.

## Owned Android master core (first-release candidate)

The pure-Java `OwnedPcmMixer` is the proposed common output seam for Player, DJ A/B, scratch and sampler voices. Cue is explicitly rejected. It renders one bounded 48 kHz stereo PCM clock before local output and the optional encoder fanout. Decoder generations reject old media after seek/flush; presented-frame markers distinguish actual media from pauses and generated silence. Export congestion disconnects export without blocking local rendering. The owned voice and PCM queue bounds do not scale with the music library.

Ten targeted JUnit tests currently pass on a checksum-verified Temurin 17 toolchain with a 128 MiB Java heap. They cover ownership/cue exclusion, crossfade and scratch precedence, identical local/encoder frames, network backpressure failure, stale decoder generations, pause/seek/EOS, underflow, capacity bounds, discontinuity rejection and stale presentation clocks.

This core is not yet wired into Media3, SoundPool replacement or the existing scratch output, and does not by itself enable phone broadcasting. Full-master acceptance still requires those sources to share the owned render clock, the real MP3/HTTP ingestion path, Android build checks and device playback/latency evidence. No phone or physical speaker pass is claimed.

### Owned master and sender core evidence (2026-11-23)

The pure Java master now uses bounded SPSC rings; render never takes the control, decoder or encoder monitor and calls no external callback. A stalled consumer regression holds all three monitors while local render completes. The render output and exported encoder-input frames are identical after ownership, gain, crossfade, scratch substitution and sampler summing. Hard saturation is explicitly observable with a clipping counter; applications must provide headroom, rather than claiming a transparent limiter.

The identity-bound live sender and HTTP adapter use the existing sequenced MP3 contract: bounded packets, exact lost-ack retries, guarded resume, cancellation and owner-conflict rejection. The 23 pure Java tests passed with a 128 MiB heap. These components are not yet wired into Media3, SoundPool or the scratch output; this is not Android playback or real-device broadcast acceptance.

### Candidate common sink (not enabled for users)

The service can now construct the owned Media3 sink behind the internal, default-off `owned-pcm-master-candidate` setting. Player and A/B use one stereo 48 kHz hardware output; cue retains its original private sink. Hardware initialization failure chooses the original sink before cutover. The new sink handles mono/stereo PCM16, gapless delay/padding, Sonic resampling/speed/pitch, bounded queue backpressure, EOS versus physically presented output, independent pause, flush generations and explicit timestamp discontinuities. Offload/passthrough/tunneling cannot bypass the candidate master. Unsupported formats remain explicit.

32 Java tests passed against pinned actual Media3 1.8.1 classes and official Android API35 compile interfaces, using a fake hardware output. The speed-checkpoint regression verifies held encoder-padding frames are released at a tempo change and only genuine final padding is discarded. `tools/test_android_owned_pcm.py` reproduces these tests with supplied dependency directories and records dependency/source hashes.

A separate generated-audio native FFmpeg probe passed actual isolated current-server ingestion: 65 sequenced uploads, 6.024 seconds decoded, owned local/export frame parity including sampler/scratch, and stale-owner upload/stop rejection preserving a replacement. This proves protocol and native codec behavior, not Android FFmpegKit, AudioTrack drivers, network latency or speaker acceptance.

Still required before enabling: move SoundPool sampler and main scratch AudioTrack onto the common bus; bind service-owned encoder/uploader and explicit receive/send ownership (including same-station feedback prevention); full Android CI and real-device acceptance. Existing normal playback remains the default. No automatic broadcast or whole-library stem preparation was added.
