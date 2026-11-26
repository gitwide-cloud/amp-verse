# Headless AMP Cast deployment — current milestone

**Scope update:** QNAP app/deployment work is cancelled. LAG and AUHUBNAS plans below are historical, not active instructions. Focus is Sager Linux and the Windows/Linux player with optional analysis. NAS shares remain storage. See OPTIONAL_ANALYSIS.md.

One shared Rust server and analysis engine serve Windows, Linux and ARM candidates. Device-specific choices live in configuration/environment files. The complete requested server plan is in CAST_SERVER_SPEC.md; the revised roles below supersede its earlier home-QNAP production role.

| Device | Role / gate |
|---|---|
| LAG — TS-431P | ARM32 development/test only. Isolated generated-music test, spare port and private state directory. Preserve storage/torrent/software services and network isolation. |
| AUHUBNAS — TS-231P3 | Office production analysis, optionally lightweight radio. Deploy only after actual LAG acceptance passes. |
| Sager — Linux | Intended primary home server. Full profile uses the same code and local persistent configuration. Advanced stations and Google Cast remain later milestones. |

## Implemented foundation

Headless executable, existing public MP3 radio listener, independent server/station IDs, private persistent server configuration, shared coordinated incremental background analysis, progress/pause/resume endpoints, optional admin password, SIGTERM shutdown, systemd/Compose examples and candidate image/native ARM32 build pipelines. The server runs without AMP Player or a desktop login.

The current radio implementation still hosts one station per process, with one shared encoder broadcast per station and a possible gap between tracks. Multiple processes/servers are independent. Saved multi-station definitions, continuous PCM Smart DJ mixing, HLS/AAC, Google Cast sender/receiver integration, soundcard routing, live takeover, SSD audio caching and synchronized receivers are not implemented here. Capability feedback reports HLS and Google Cast as false. No Android APK was changed.

## Private configuration

`AMP_CAST_CONFIG_DIR` holds `server.json` with version, persistent server/station IDs, station name, analysis enabled, worker count, scan interval and pause-during-radio preference. `AMP_DEVICE_CONFIG_DIR` holds installation identity and local outbox. Keep both private and unique per installation. They must persist across container replacement and stay separate from the shared music directory.

Default analysis is disabled, one worker, 60-second interval, and pause while broadcasting. Configure `server.json` while stopped, or use authenticated `/api/analysis/pause` and `/api/analysis/resume`; these persist enabled state. `AMP_ANALYSIS_ENABLED=true` bootstraps the first start only, so an operator's saved Pause survives restart. `AMP_ANALYSIS_WORKERS` can override worker count. In-flight jobs finish or retain expiring leases; pausing prevents new jobs.

`/api/status` exposes actual progress, IDs and capabilities. `/health` checks the HTTP process, not media/NAS availability. Existing `/api/start`, `/api/stop` and `/listen.mp3` remain. Optional `x-amp-server` / `x-amp-station` command headers prevent controls intended for a different selected instance from being applied. Public listening does not require an admin password. The optional password protects controls; there are no cloud accounts.

CLI analysis: `amp-prepare MUSIC_FOLDER --jobs 1 --watch 300`. Completed tracks are reused. `--force` is explicit reanalysis, normally for review/debugging rather than an unattended repeating job.

## Windows standalone package

Run `install-windows-server.ps1` from an extracted Windows engine artifact. The installer verifies the binaries against `build.json`, preserves the previous installation in a rollback folder, and leaves the server configuration and music alone. It does not start or stop the server.

For LAN use, run the installer as administrator with `-PrivateLan`. It installs inbound TCP 8787 and UDP discovery 18787 rules for the installed server executable on one Private interface and its local IPv4 subnet. Use `-InterfaceAlias` if more than one Private interface is active. Start the server with an explicit private bind and a control password. A loopback install requires no firewall rules. Phones and laptops need no inbound firewall rule to connect.

## Linux / Docker

Build `cargo build --locked --release -p amp-cast`. This builds server and analyzer without GUI/audio-device dependencies. FFmpeg must be installed for analysis and MP3 broadcasting. The Dockerfile supplies FFmpeg; native packages do not bundle a QNAP FFmpeg binary.

Use `deploy/compose.yaml` with a private `.env` based on `deploy/.env.example`. Create the data directory with the selected user's ownership; music must already be mounted. Compose refuses to silently create a missing bind source. The process must be able to create/update `.ampverse` records, but application code never rewrites source songs. Bind localhost initially; explicitly select a LAN interface after testing. No privileged container, host networking or storage-management access is required.

Build candidate: `docker buildx build --platform linux/amd64 -f deploy/Dockerfile --load -t amp-cast-test .`. The image pipeline builds/tests headless Linux first, then AMD64/ARM64/ARMv7 images. Each image must pass its container health check before an immutable commit tag and `edge-<architecture>` tag are pushed to GHCR. Edge is a test channel, not production approval. Do not deploy a tag until its actual CI result and image digest are verified. First GHCR package visibility/access may need account configuration.

The systemd example runs as a dedicated `ampverse` user and requires the music mount before startup. Install verified binaries into `/usr/local/bin`, create/configure the service account/data/music mount, review `deploy/amp-cast.service`, and enable it only on the designated server. It does not require a graphical login.

## ARM32 compatibility gate

QNAP documents the TS-431P and TS-x31P3 family as ARM32. This does not establish their installed kernel, firmware, Container Station version, free RAM or compatible floating-point ABI. Read those from LAG before selecting a runtime. Do not install Docker/Container Station or alter existing NAS services automatically.

The headless candidate uses Rust 1.89 (shared source, separate build toolchain) to retain that release's documented ARMv7 Linux baseline of kernel 3.2/glibc 2.17. Current Rust releases have newer ARM requirements. Dependencies are locked and checked against 1.89. Shared coordination additionally requires Linux 3.15+ OFD byte-range locking; the Rust compiler baseline alone is not the complete runtime requirement. The native candidate is static `armv7-unknown-linux-musleabihf`, avoiding a QNAP glibc dependency; its actual kernel/CPU/runtime compatibility still needs testing. Docker candidates use Debian Bookworm userspace and GNU ARMv7 binaries. A container does not replace the host kernel.

Native ARM32 artifact contains `amp-cast` and `amp-prepare`; set their executable permission after extracting a ZIP (`chmod +x amp-cast amp-prepare`). FFmpeg must separately be compatible on the NAS. ARM emulation and a successful cross-build do not prove native FFmpeg, Container Station, SMB locking or long-running radio performance. If LAG cannot safely support either runtime, document that blocker rather than changing its production services.

Read-only preflight: architecture, kernel, free RAM, free disk, existing container runtime/version, occupied port and effective UID/GID. Test only in a new dedicated folder with generated audio, never by moving or replacing the real music directory. Measure idle/server/analysis/radio CPU and RSS, restart recovery, persistent identities/results, two-host claim exclusion, lease takeover, network interruption and preservation of all existing NAS services. AUHUBNAS remains gated until those checks pass.

## Sources and verification boundaries

- [QNAP TS-431P specifications](https://www.qnap.com/en/product/ts-431p/specs/hardware/software)
- [QNAP TS-x31P3 official guide](https://download.qnap.com/TechnicalDocument/Storage/Home%20NAS/TS-x31P3/TS-x31P3-UG-01-en.pdf)
- [Rust 1.89 platform requirements](https://github.com/rust-lang/rust/blob/1.89.0/src/doc/rustc/src/platform-support.md)
- [Rust file-lock API source/stability](https://github.com/rust-lang/rust/blob/1.89.0/library/std/src/fs.rs)

This workstation has neither Docker nor WSL installed. Local verification uses Windows processes, real FFmpeg, the shared unit/native tests and an ARM32 cross-build. Linux/container/emulated ARM checks run in GitHub CI; actual LAG/AUHUBNAS/Sager deployments require management connectivity and are not claimed as completed. Hardware resource benchmarks remain unmeasured.

### LAN discovery

A Cast HTTP listener on a non-loopback IPv4 address also listens on UDP 18787. Clients send AMPVERSE_CAST_DISCOVER_V1 and receive versioned server/station identity, station name and HTTP port. Discovery never joins a session or starts playback/analysis. Loopback HTTP listeners are not advertised. AMP_CAST_DISCOVERY_PORT=0 disables discovery; AMP_CAST_ADVERTISE_PORT overrides the HTTP port in replies for mapped deployments. The default is the actual HTTP listener port. Replies are limited to private/link-local/loopback IPv4 sources and rate limited.

Allow UDP 18787 and the configured HTTP port on the trusted LAN. Docker Compose exposes discovery on the same configured bind address and advertises AMP_PORT. Broadcast traversal across Docker/NAT, VLANs and firewalls is not guaranteed; use a manual server URL when discovery cannot reach the server. No physical LAN or NAS deployment is implied by loopback smoke tests.

### Existing Windows server access code

The standalone Windows package includes **Show Server Access Code.cmd**. Run it on
the server PC, signed in as the Windows user who originally configured the server.
It opens the existing CurrentUser-DPAPI-protected code in a local dialog; it does
not reset the password, copy it to the clipboard, write plaintext, or print it to
the terminal. Enter the code only in your own Studio sign-in screen. The helper
uses the private LOCALAPPDATA/AMPVerse/StandaloneServer folder by default. Custom
deployments can run show-windows-server-access.ps1 with its ConfigDirectory
parameter. An absent or unreadable code gives an explanation and changes nothing.
This is initial pairing/recovery, not a background tray manager or account creation.

The Windows installer reuses a single existing Programs/AMP Verse Server folder,
including historical version-suffixed folders. Multiple candidates require an
explicit existing InstallDirectory instead of silently adding another copy.
Running Cast/server processes and linked destinations are refused before copying.
No process or playback is stopped automatically.
