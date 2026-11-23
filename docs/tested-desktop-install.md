# Update the canonical Windows desktop installation

Use `tools/install-tested-desktop.ps1` for the existing
`%LOCALAPPDATA%\Programs\AMP Verse` installation. It updates that directory and
keeps the active launcher, shortcuts and profile in place. It creates no preview
installation, shortcut or profile. `install-tested-preview.ps1` is deprecated and
retained only for compatibility with historical deliveries.

The existing player, Cast, updater, analysis and native stem workers must be closed by the
user. The installer refuses running processes; it never kills playback and never
launches the app. It also refuses a pending `amp-player.next.exe`, which the
existing launcher would otherwise install over the new delivery.

Provide extracted Windows desktop and Windows analysis-engine artifacts from the
same successful CI run, their full source commit, and a delivery manifest whose
SHA-256 was supplied independently. The manifest format is the existing version-1
format documented in `tested-preview-install.md`: `source_commit`,
`desktop_run_id`, `engine_run_id`, `desktop_files` and `engine_files`; every file
entry has `path` and `sha256`. A manifest is an integrity record, not proof by
itself that CI passed or that artifacts came from the claimed run.

```powershell
.\install-tested-desktop.ps1 -DesktopArtifact .\desktop -EngineArtifact .\engine `
  -Manifest .\delivery-manifest.json -ManifestSha256 ACTUAL_MANIFEST_SHA256 `
  -SourceCommit ACTUAL_FULL_COMMIT -RunId ACTUAL_RUN_ID
```

Before changing installed files, the installer verifies the artifacts, stages
selected application files and the matching engine, and verifies backup copies
of every file it will overwrite. Existing FFmpeg tools/notices are required and
left unchanged. Artifact launchers, shortcuts and profile files are excluded
from the update plan. Existing add-ons are retained; addons/amp-engine.exe is replaced with this delivery's matching engine. A desktop manifest may additionally include the fixed addons/stems-live native module. Its worker, runtime, model, module manifest, notices and four named license texts must all be present together and individually verified by the delivery manifest. Only exact filenames accepted by Get-StemLiveFiles enter the plan; Python environments, arbitrary DLLs and other add-ons cannot enter through this exception. The optional exact onnxruntime_providers_shared.dll is accepted when supplied. Existing same-name module files receive the normal verified backup; rollback restores them and removes only newly introduced module files. Owner files elsewhere in add-ons are untouched. Documentation and packaged
Lab dependencies may be refreshed. Shared music files/library metadata are not
included in the update or settings backups.

Backups are in `%LOCALAPPDATA%\AMPVerseInstallBackups\<timestamp-random>`;
they are recovery data, not another runnable installation. The active launcher's
bytes are backed up and checked unchanged. Default settings snapshots cover:

- `%APPDATA%\amp-verse` and `%APPDATA%\AMPVerse`;
- `%LOCALAPPDATA%\AMPVerse\V1Preview\settings`;
- `%LOCALAPPDATA%\AMPVerse\V1Preview\AppData`;
- `%LOCALAPPDATA%\AMPVerse\V1Preview\Documents`.

A custom launcher may select other folders. Supply them with
`-SettingsDirectory @('C:\Actual\Settings', 'C:\Actual\Documents')`.
The backup manifest records each selected source, whether it existed, and hashes
of copied files. The installer never snapshots the entire `LOCALAPPDATA\AMPVerse`
tree, which can contain large caches/work directories. Paths overlapping the
installation or backup directory, volume roots, and linked paths are refused.
No profile or settings data is rewritten by the installer.

A shared exclusive update lock prevents overlapping install/rollback transactions.
Each application-file replacement is staged, verified and published atomically.
An installation copy failure attempts to restore changed files from the verified
backup. If files remain locked or another app starts during recovery, the error
reports the retained backup and says recovery needs attention. Nothing is
deleted recursively. Keep the printed **backup manifest SHA-256** for rollback.
A successful update records its source/run in `tested-desktop-receipt.json`.

To roll back, close AMP Verse and its workers, then run the delivered rollback
script with the printed backup path and checksum:

```powershell
.\restore-tested-desktop.ps1 -BackupDirectory 'C:\...\AMPVerseInstallBackups\...' `
  -BackupManifestSha256 ACTUAL_BACKUP_MANIFEST_SHA256
```

Rollback verifies all old-file checksums first and refuses to overwrite files
changed after this update. It restores overwritten files and removes only files
introduced by this update. It leaves launcher, shortcuts and settings in place.
The separate settings snapshots are available for deliberate recovery if later
application use changed settings; they are not silently restored over newer work.
An interrupted rollback can be retried: files already matching their backed-up
hash are recognized as restored, while unrelated later modifications are refused.

Verification must state the exact delivery commit/run and actual install result.
Parser/function checks do not establish a successful installation or GUI test.

## Bounded checks

`tools/test_install-tested-desktop.ps1` checks PE/hash/path boundaries by extracting
functions without invoking installation. `tools/test_install-tested-desktop-transaction.ps1`
uses synthetic x64 fixture files and temporary LOCALAPPDATA/APPDATA to exercise
installation and resumable rollback, manifest rejection, launcher/profile
preservation, receipt/backup verification and later-file-change refusal. It
retains its temporary files for inspection, never launches fixture binaries,
and refuses to run while any AMP process is open. It is suitable for the
Windows CI runner. These checks do not verify actual audio playback or GUI use.
