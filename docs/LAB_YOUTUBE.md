# AMP Lab — YouTube URL import

In desktop **AMP Lab**, paste a YouTube video URL and press **Download MP3**. Progress covers downloading and MP3 conversion; **Cancel** stops the stage and its child processes. **Retry** appears after errors. The completed MP3 automatically enters the existing waveform editor with preview, selection, fades, gain, normalize and export. There is no format selector.

The original download is a **256 kbps MP3**. Re-encoding does not improve the source quality. Lab exports suggest a separate `_clip.mp3` and reject existing destinations/source overwrites. If you have unexported edits, Lab asks before replacing them. Declining retains the editor and completed download; export first, then choose **Open downloaded MP3**. An active export also prevents replacement. This protection is shared by Open audio, From Library and URL imports.

Completed originals persist in:

- Windows: `%LOCALAPPDATA%\AMPVerse\AMP Lab\downloads`
- Linux: `$XDG_DATA_HOME/amp-verse/lab/downloads`, or `~/.local/share/amp-verse/lab/downloads`.

Titles are sanitized and byte-bounded, video IDs identify sources, and collisions get numbered filenames. Each action uses its own temporary directory. Failures/Cancel remove incomplete files; completed files are retained. Completed MP3s are published atomically without replacing existing files.

## Dependencies and recovery

Windows x86_64 and Linux x86_64 packages include the official **yt-dlp 2026.10.26** standalone binary and **Deno 2.9.8** under `tools/lab-youtube`. These run as separate tools, not a plugin/add-on. The official yt-dlp standalone includes matching EJS scripts. YouTube support requires a supported JavaScript runtime (Deno >=2.3.0); see [yt-dlp's official EJS guide](https://github.com/yt-dlp/yt-dlp/wiki/EJS). The standalone includes Python; users do not need to install Python for normal downloading. yt-dlp's current Python-package minimum is 3.11; see [the pinned upstream release](https://github.com/yt-dlp/yt-dlp/releases/tag/2026.10.26).

**FFmpeg, ffprobe and ffplay remain the existing AMP Lab audio dependencies**, located beside AMP Verse or on PATH. FFmpeg must include `libmp3lame`. This feature does not redistribute a new codec build. Linux: install your distribution's `ffmpeg` package (for example `sudo apt install ffmpeg`). Windows: use your existing AMP Verse audio tools, or obtain a Windows build linked from [FFmpeg's official download page](https://ffmpeg.org/download.html); place ffmpeg.exe, ffprobe.exe and ffplay.exe beside amp-player.exe. Retain that build's licenses and corresponding-source instructions.

To repair or update the downloader/runtime bundle, run the included setup script with Python 3.11+:

```text
python setup/setup_lab_youtube.py --dest tools/lab-youtube
```

From a source checkout:

```text
python tools/setup_lab_youtube.py --dest target/release/tools/lab-youtube
```

The script fetches pinned official GitHub releases, verifies release SHA256 digests (or official checksum manifests), stages writes, and retains upstream licenses, third-party notices and project source archives. It neither invokes a shell nor installs global packages. Maintain the pinned versions in the script and ship updates through normal AMP Verse packages. Missing tools produce a readable error with **Downloads / tools → Locate tools** and **Setup instructions**. A selected tools folder is remembered locally. The `AMP_LAB_TOOLS` environment variable supports developer/managed installs. Unsupported architectures can provide their own official matching yt-dlp/EJS, Deno and audio tools; automatic packaging currently follows the project's Windows/Linux x86_64 targets.

## Boundaries

Accepts ordinary YouTube watch, youtu.be, Shorts, live-video permalink and embed URLs containing a single valid video ID. Canonicalization removes playlist/tracking parameters, and yt-dlp also receives `--no-playlist`. Playlist-only/channel URLs and ongoing livestreams are rejected. Only completed single videos are imported. No cookies, browser sessions, user yt-dlp configuration, user plugins, DRM bypass or access-control bypass are used. Private/unavailable videos, network failures, disk errors and conversion errors leave the existing editor/source intact.

The UI uses bounded worker channels. Processes use structured argument arrays and isolated Windows Job Objects / Unix process groups; cancellation and timeouts terminate the whole tree. No URL-import screens are added to Player, DJ, Library or Android.

## Verification

Focused tests cover URL validation and playlist stripping, progress parsing, safe Unicode filenames, atomic filename collisions, missing tools, disk-path errors, private/network/invalid-audio failures, original preservation, and declined/blocked edit replacement. A locally compiled fake downloader and **real FFmpeg/Deno** exercise the entire download/conversion/publication/automatic-delivery path, including Cancel during download and conversion and a spawned grandchild heartbeat. A real editor-open test verifies waveform decoding and source preservation.

Live verification on Windows also succeeded against the public Big Buck Bunny test video (`aqz-KE-bpKQ`) using the official pinned yt-dlp/Deno and installed FFmpeg. The older upstream test ID `BaW_jenozKc` was unavailable and correctly reported as such; its fixtures remain deterministic local tests. Live YouTube access is intentionally excluded from routine CI, where it could fail due to video/network availability. Linux CI runs the local pipeline with real codecs and packages the official runtime bundle.
