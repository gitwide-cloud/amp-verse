# Tested Windows preview delivery

This installer consumes extracted CI artifacts. It does not build the app, change
an existing installation, stop playback, or launch the new app automatically.

Delivery preparation (maintainer): obtain **amp-verse-windows** and
**amp-analysis-engine-Windows** artifacts from the **same successful workflow run**.
Verify that run's head commit and test results first. Create a delivery manifest
from those exact extracted files, using SHA-256 for every file included:

```json
{
  "version": 1,
  "source_commit": "FULL_40_CHARACTER_LOWERCASE_COMMIT",
  "desktop_run_id": "GITHUB_RUN_ID",
  "engine_run_id": "GITHUB_RUN_ID",
  "desktop_files": [{"path": "amp-player.exe", "sha256": "SHA256"}],
  "engine_files": [{"path": "amp-engine-windows-x86_64.exe", "sha256": "SHA256"}]
}
```

Include **all** artifact license/notices/docs files in the manifest, not just the
executables illustrated above. Supply the manifest SHA-256 independently in the
delivery instructions. A manifest binds an inspected delivery to its files; it
is not a substitute for checking GitHub run provenance or a code signature.

Keep `install-tested-preview.ps1` and `launch-tested-preview.ps1` together. From
Windows PowerShell, invoke:

```powershell
.\install-tested-preview.ps1 -DesktopArtifact .\desktop -EngineArtifact .\engine `
  -Manifest .\delivery-manifest.json -ManifestSha256 MANIFEST_SHA256 `
  -SourceCommit FULL_COMMIT -RunId RUN_ID
```

The installer validates hashes, x64 PE headers, required files and same-run
manifest identity before installation. The destination and shortcut include the
commit. Already-existing destinations are refused. FFmpeg tools missing from the
artifact are reused from the existing AMP Verse installation **with its notices**;
installation stops if these runtime prerequisites are unavailable. The matching
engine is always taken from this delivery, never from the old installation.
The receipt records those runtime hashes and sources.

Close the current player and Cast host yourself, then use the new shortcut.
Settings, device identity, caches, home/profile paths, roaming and local app data
are isolated per full commit for both normal and acceptance launches. Existing
music-root preferences are not imported. Choose a music folder explicitly; the
installer itself performs no library scan. Show Control and Cast autostart are
disabled. Show Control uses loopback when enabled. The delivered 1c2945dd binary
has a separate known limitation: manually starting Native Cast overrides the
launcher bind and listens on all interfaces. Do not start Native Cast during
this isolated acceptance; the launcher does not enforce its bind address. Shared library metadata is still shared when you
select the same NAS music root. This is **not** a sandbox for editing that library.

To return to the previous version, close the preview and use the original AMP
Verse shortcut. Your original application and settings remain in place. Remove
only the commit-specific preview installation, shortcut and settings folder when
you no longer need it. No automatic cleanup is performed.

For isolated GUI acceptance, run the installed launcher with
`-AcceptanceFixture -MusicRoot C:\Path\To\DisposableFixture`. This uses a separate
acceptance profile. Both modes override inherited home/profile paths for the
child so neither imports the normal music-root preference. Use only disposable fixture
music here. Normal shortcuts do not select acceptance mode.

Verification status must be recorded for the actual delivery: script parser
validation alone does not establish successful installation or GUI acceptance.

Launcher follow-up to the 1c2945dd binary / 62195e6 installer delivery: fixes
normal-mode USERPROFILE isolation and PowerShell 5.1 UNC MusicRoot resolution.
This source patch does not change compiled binaries or an installed preview.
Before shipping it, validate with Windows PowerShell 5.1 for both launch modes
and a UNC fixture root; refresh delivery-manifest hashes and provenance for the
launcher and this document. Preserve the prior launcher for rollback. Never
reuse the prior manifest hash after changing these files.
