# Offline Windows desktop installer

Build a verified Desktop folder package with `tools/package_windows_installers.py`, then wrap it with `tools/package_windows_exe.py --package PACKAGE --output AMP-Verse-Desktop-Setup.exe` on Windows. The wrapper uses the Windows .NET Framework compiler and embeds the complete verified package, codecs, engine, app-local runtimes and licenses. No developer tools are required on the destination laptop.

Run the EXE, choose an existing music folder or network share, and click Install. Installation is per-user and preserves the existing supported desktop installation/profile through the transactional installer. A new installation receives a Start menu shortcut. No administrator access, system runtime installation or persistent execution-policy change is required.

The EXE verifies its embedded archive and installer before execution; the inner installer verifies every payload file. This checksum chain is integrity checking, not publisher authentication: the current setup EXE is not code signed.

For disposable acceptance tests, `--verify-only` verifies extraction without installing. `--fixture-root D:\CodexBuildCache\NEW-CHILD --music EXISTING-MUSIC-FOLDER` installs without launching or creating shortcuts, using the existing strict fixture containment checks. Normal installation should be tested separately on a clean Windows machine before a general release.
