# AMP Verse 1.0 Windows preview

The first V1 preview brings Player, Library, DJ, Lab, Cast/Remote and add-ons into one workspace. The transport at the bottom controls the current Player/DJ playback owner. Changing workspaces does not start playback or take ownership. The classic compact player and separate dockable windows remain available through **Classic windows**. Use the lightning/menu button and **Version 1 workspace** to return.

## Install and return to version 0

Build the release player, then run `tools/install-v1-preview.ps1`. It installs into **AMP Verse 1 Preview**, alongside the existing **AMP Verse** installation. It creates separate **AMP Verse 1 Preview** and **AMP Verse v0** shortcuts. Close the preview before starting version 0. No uninstall or library restoration is necessary.

The preview keeps preferences, cached local state, session recovery and local documents under `%LOCALAPPDATA%/AMPVerse/V1Preview`. Only the initial library path, theme and completed startup catalog snapshots are copied from V0, once. Catalog snapshots are validated on load, then refreshed in the background. The original application and its preferences stay untouched. Development/test runs may select an explicit isolated profile with `AMP_VERSE_CONFIG_DIR` and enable the workspace with `AMP_V1_MODE=1`.

This is a UI preview, with no new music catalog format or automatic data migration. Both versions can read the existing music library. Deliberate edits to shared playlists, cues, tags and preparation remain in that shared library when returning to V0. Reverting the application does not undo music edits. Do not run both players simultaneously against the same output.

Version 0 is preserved at Git branch `archive/v0-before-v1-20261112`. V1 development is on `feature/version-1-preview`; it does not replace the stable GitHub release. Android remains on its own current build.

## Verification

Windows workspace tests pass at 760×540, 1280×900 and 1920×1080: transport remains in bounds, navigation keeps the playback owner, and navigation does not issue native window-size commands. Native screenshots of Library, DJ and Player were inspected. Library preparation starts collapsed to give the music browser space. The DJ view fits inside the workspace; the classic view remains available for compact use. Very small windows reduce the DJ control size; use a larger window for performance.

Workspace tests and the existing Windows test suite passed (308 passed, 53 hardware/network-dependent tests skipped). Workspace-wide Clippy passed with warnings treated as errors. The installed V0 executable checksum was unchanged after installing V1. This UI update does not claim new audio DSP, Android changes, or fresh hardware receiver/controller validation.
