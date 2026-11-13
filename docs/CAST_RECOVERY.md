# Protected phone audio (Cast 1.0.3 / Android 0.2.7)

AMP Cast retains one encoded MP3 timeline in a bounded 2 MiB replay buffer.
`/api/audio/manifest` identifies the server, station, audio-session epoch and
starting sequence, approximately six seconds behind live output. Fixed 8192-byte
chunks are available at `/api/audio/chunks/{sequence}?epoch=...`; sequence,
epoch and SHA-256 headers allow the Android client to reject wrong or damaged
audio. Expired chunks return 410 and changed sessions return 409.

Android verifies advertised private server addresses against the selected
server, station and epoch before racing up to two links for each chunk. It
delivers each sequence once, in order. A failed link therefore does not require
restarting the decoder when the other can supply the same chunk in time.
The phone also uses a network wake lock during radio playback, a larger playback
buffer, and bounded reconnect backoff. Pausing or stopping cancels reconnect.

This is application-level recovery, not network bonding. It adds live delay.
The server needs reachable Ethernet and Wi-Fi addresses to offer two physical
paths. Both still share the phone's final Wi-Fi hop; outages exceeding available
buffering can interrupt playback. Audio endpoints retain existing public LAN
listening semantics; hashes detect corruption, not malicious servers.

Ordinary `/listen.mp3`, Google Home and other receivers remain compatible but
do not use this Android-specific recovery client. Android discovers support
when connecting to a server; reconnect after upgrading an already selected
server.

## Verification

- Rust workspace: 315 passed, 53 ignored; strict workspace Clippy passed.
- Pixel 7: corrupt/wrong sequence, identity/hash validation and backup chunk
  recovery tests passed.
- Real server MP3 through the actual Pixel playback service: primary proxy
  dropped after startup, 14 dropped requests and 53 backup requests, zero
  rebuffer events during the monitored interval. Test restores prior queue,
  volume and playback intent.
- Reproduce with `tools/test_cast_recovery.py`, `AMP_ADB`, `AMP_CAST_EXE`,
  `AMP_FFMPEG`, and matching installed app/test APKs. It uses isolated server
  state and ADB loopback forwarding, not physical dual-NIC failover.
- Android assembly passed. Lint completed, with existing Google Cast dependency
  Kotlin metadata compatibility warnings.
- Physical Ethernet/Wi-Fi failover remains unverified: Ethernet was disconnected
  on the test PC. Unattended overnight operation is also not yet verified.
