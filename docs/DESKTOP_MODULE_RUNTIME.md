# Desktop module runtime — 1.0.18

The shared Windows/Linux Player constructs optional feature state on demand.
Lab, Cast, Podcasts, Transcribe, tag/artwork lookup, controller mappings and audio
output preferences remain uninitialized until their screen or an explicit command
needs them. Passive playback, recovery and remote status checks must use
`Module::get`/`get_mut`, never dereference a dormant module.

This changes runtime activity, not desktop package boundaries: these UI modules
are still compiled into the desktop executable. It is not a separate-DLL download
implementation, and no binary-size or measured RAM reduction is claimed.
Analysis, stem separation, transcription and URL processing continue to use their
existing separate tools only when requested.

## Lifecycle

- Player starts without opening an audio device. The shared playback engine opens
  on the first playback/load operation. DJ decks do not each get a startup engine.
- Merely opening Cast creates no connection worker. Connect or host/broadcast
  actions start the required worker/server. Explicit ongoing broadcasts and jobs
  are not cancelled by navigating away.
- Podcasts is not polled before use. Hidden Podcasts finishes existing work and
  records episode progress but starts no new automatic feed refresh/download;
  opening it resumes that scheduling. Hidden artwork textures are released.
- Lab preview stops when its window closes; its edits remain in memory, protecting
  unsaved work. Explicit exports/downloads and analysis jobs keep their existing
  lifecycle rather than being discarded by a tab switch.
- A stopped, idle UI requests a redraw at most once per second; audio and auxiliary
  playback retain their faster refresh, and input/workers can wake the UI sooner.
- Periodic source rescans require a visible Library or an explicitly enabled
  analysis queue. Startup cache loading and manual refresh remain available.

## Remote and launchers

Remote is off by default. Open **Remote / Web UI → Start remote service** when
needed. **Start Remote when AMP Verse opens** saves an opt-in preference. Existing
external deployments may explicitly use `AMP_CONTROL_AUTOSTART=1`;
`AMP_CONTROL_DISABLE` still prevents automatic startup.

The Windows launcher does not automatically spawn AMP Cast or set automatic Cast
connection. It applies a verified pending Player update only after the current
Player has closed. Installer shortcuts use the same portable launcher template.
An independently running headless Cast service remains independent.

## Verification

Regression coverage checks that idle polling and remote snapshots leave optional
modules and audio devices dormant, module switching retains state, and the first
on-demand Cast connection reaches the real local test server. Workspace tests,
local Lab conversion/cancellation fixtures and strict Clippy validate the shared
code on Windows. Linux uses the same runtime paths; no Linux device run was made.
