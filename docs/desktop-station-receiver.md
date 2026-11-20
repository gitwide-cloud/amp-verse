# Desktop station receiver

Desktop Listen fetches HTTP(S) in Rust and gives FFplay only `pipe:0`, with a
`file,pipe` protocol whitelist. This supports the installed network-disabled
FFplay; it does not change the bundled FFmpeg distribution or its protocols.
No selected station code is forwarded to arbitrary radio URLs. Redirects are
refused; a station must provide its final HTTP(S) stream address.

The supervisor owns the decoder process. The HTTP pump owns only its reader and
stdin, uses 8 KiB chunks and no application backlog, and has bounded network
phase/read timeouts. A DNS timeout is terminal (not retried): Ureq bounds
caller waiting, but its internal OS getaddrinfo helper cannot be canceled or
joined by this receiver. A discovered literal LAN address avoids that helper. Decoder death, ended streams and stalled pipe progress
trigger a fresh HTTP connection and decoder, rather than joining unrelated MP3
streams inside one decoder. Five consecutive failures end the session and show
the bounded decoder diagnostic. A healthy session resets the failure budget.

Stop requests cancellation without waiting on the UI thread. The receiver role
and its per-Player audio lease remain reserved until the worker has killed and
reaped the decoder and joined its HTTP/stderr pumps. Closing the window requests
the same finite cleanup; it never leaves a detached decoder process running.
Liveness is polled from the main Player update even with the Cast view closed.

For this transport fix, pause local Player/DJ, previews, sampler and cue playback
before choosing Listen. Sending and receiving are mutually exclusive. While
receiving, new local playback requests report Stop Listening first. The shared
per-Player mixer gate freezes source advancement, pauses local play flags and
rejects stale queued play commands. Stopping the receiver does not resume local
playback, discard a queue, or rewrite saved playlists or unsaved editor drafts.
Independent offline/test mixers do not share that gate.

## Verification

Ordinary tests cover real owned subprocess death, bounded diagnostics, drop and
idle/stalled-input cancellation, receiver restart, output lease/source-position
freeze, stale admission rejection and local/remote denial. Hosted CI additionally
streams a generated MP3 over a loopback HTTP fixture into FFplay with SDL dummy
audio. On Windows it downloads the exact installed FFplay from the pinned MP3
Lab release archive and verifies both archive and executable SHA256 before use.
Original notices remain in that CI archive and a provenance receipt is retained.
Linux uses the installed system FFplay with an explicit file/pipe whitelist;
that test does not claim to be the Windows executable. No physical audio device,
microphone, owner library or owner installation is required.
