# Windows Desktop and Server packages

These candidate packages provide a fresh per-user installation or update the
single existing installation. Extract locally and open `Install AMP Verse.cmd`,
then choose the existing music folder. Server means the music folder to serve to
your AMP devices. No Python or Rust is needed on the receiving PC.

Desktop uses `%LOCALAPPDATA%\Programs\AMP Verse`; Server uses
`%LOCALAPPDATA%\Programs\AMP Verse Server`. A unique legacy `AMP Verse Server*`
installation is updated instead. Existing launchers, profiles, identities and
runtime behavior are preserved by the existing transactional updater. Ambiguous
installations are refused; no extra preview installation is created.

Fresh Server is code-free on the trusted LAN, with music sharing enabled and
analysis initially disabled. `-ProtectedMode` explicitly selects an account-bound
protected code. The local Show Access Code helper explains code-free mode. No
startup entry, service or firewall rule is installed. The optional ordinary
Start-menu shortcut opens the app. Server's installed folder also provides
`Open AMP Verse Server.cmd`, `Server Tray.cmd` and `Run Server Console.cmd`.
Closing the manager leaves the server running. The embedded web view requires
WebView2; the console and browser controls work independently of it.

For unattended installation:

```powershell
.\install-windows-package.ps1 -MusicDirectory 'D:\Music' -NoStart
```

## Producing the unpublished candidate packages

Use extracted, checksum-verified Desktop, Engine and Server artifacts from the
same reviewed source build. Keep their original `build.json` records. On an
official Visual Studio Windows build runner, export the installed toolchain's
redistributable CRT; this never installs the global redistributable:

```powershell
.\tools\export_windows_crt.ps1 -Destination D:\staging\crt
python tools/package_windows_installers.py --role Desktop --application D:\staging\desktop --engine D:\staging\engine --codec-archive D:\staging\ffmpeg-9.1-essentials_build.zip --crt-directory D:\staging\crt --output D:\staging\desktop-installer --run-id BUILD_RUN_ID
python tools/package_windows_installers.py --role Server --application D:\staging\server --codec-archive D:\staging\ffmpeg-9.1-essentials_build.zip --crt-directory D:\staging\crt --output D:\staging\server-installer --run-id SERVER_RUN_ID
Compress-Archive -Path D:\staging\desktop-installer\* -DestinationPath D:\staging\AMP-Verse-Windows-Desktop.zip
Compress-Archive -Path D:\staging\server-installer\* -DestinationPath D:\staging\AMP-Verse-Windows-Server.zip
Get-FileHash D:\staging\AMP-Verse-Windows-*.zip -Algorithm SHA256
```

The codec archive uses the existing pinned `setup_ci_ffmpeg.py` policy. Bundled
executables, original FFmpeg license and builder README, Microsoft-signed
app-local CRT files, original Microsoft redist/license container and provenance
are inventoried. The Microsoft installer container is retained, never executed.
No DLL is taken from System32. Installer script hashes are recorded separately
from application source; packaging does not confer audio/hardware qualification.

Before uploading candidate archives, run both PowerShell editions against
`tools/test_windows_first_install.ps1`, the Python packager unit tests, and the
isolated real-binary fresh-start/restart smoke. Retain fixture and component
receipts. The fixture switch accepts only disposable D-drive cache children and
refuses shortcuts; it is not a second household installation.
