# Optional Windows server manager

This is a controller for an existing standalone amp-cast installation. The Rust
server remains headless and works without this app, .NET or WebView2. Closing a
window or exiting the tray never stops amp-cast or its receiver sessions.

The GUI is a WinForms tray plus the existing server web UI in Microsoft WebView2.
On a compatible server, the embedded window defaults to **Server speakers**.
Playback through the server or a server-owned Cast receiver continues after
closing the window. Choose **This device** for browser audio, which ends when
the window closes.
Publish self-contained .NET files for Windows x64; Microsoft Edge WebView2 Runtime
must be installed separately. Missing WebView2 shows an error without touching the
server. No runtime download, firewall mutation or service registration is implicit.

Create a manager-only JSON profile using the ACTUAL existing paths and listener
(do not guess production identities). Example placeholders:

```json
{
  "Executable": "C:\\existing-install\\amp-cast.exe",
  "MusicDirectory": "D:\\existing-music",
  "ConfigDirectory": "C:\\existing-private-server-config",
  "DeviceDirectory": "C:\\existing-device-identity",
  "Bind": "127.0.0.1:8787",
  "WebUrl": "http://127.0.0.1:8787/#cast"
}
```

For an installation that already has `Start AMP Verse Server.ps1` beside
`amp-cast.exe`, enable **Use the existing Windows server launcher**. The
manager delegates startup to that script so its runtime path, discovery,
binding and profile configuration take effect. Only the fixed script name
within the chosen installation is used.

Use the actual private IP listener and corresponding local IP URL if the existing
server does not bind loopback. The manager only opens a URL on this computer and
confines top-level navigation to that origin. It retains web sign-in cookies in
LOCALAPPDATA/AMPVerse/ServerManager/<installation-hash>/WebView, separate from
server identity. Keep this per-user directory private. Never embed access codes
in the URL or manager profile.

```
amp-server-manager --status C:\path\manager.json
amp-server-manager --start C:\path\manager.json
amp-server-manager --tray C:\path\manager.json
amp-server-manager --window C:\path\manager.json
```

Status is process evidence, not HTTP readiness or music availability. Start is
explicit, refuses any other/multiple/inaccessible amp-cast process, and never
kills or replaces an existing process. The existing executable, music and both
identity/config directories must exist. No source media scan is initiated by
opening the manager. Starting the server can perform its configured normal work.
Manager settings affect only the next explicit Start; no credentials are stored.
No stop-process action is exposed. Output Stop remains in the server web UI.

Validation: `dotnet run --project apps/amp-server-manager/Tests` on Windows/Linux;
`dotnet publish apps/amp-server-manager/Windows -c Release -r win-x64 --self-contained true`.
Stage the manager beside the matching server candidate without replacing live
files. Verify packaged hashes, retained settings and rollback before activation.
Real Windows acceptance must demonstrate tray exit/window close with an already
playing server, same server PID/IDs and continuous receiver output, as well as
WebView2-missing behavior, start races and noninteractive CLI operation. Unit tests
alone are not installed-runtime acceptance. Do not publish over Beta 1 artifacts.

First-time manager setup: `amp-server-manager --configure C:\path\manager.json`
opens the preferences form without launching a server. Supply existing paths;
Start requires existing server.json and device.json so a typo cannot silently
create new identities. The server package installer optionally verifies/copies
the bundled manager when its manifest source matches the server; an open manager
or server causes refusal before replacement. Rollback retains prior manager files.
