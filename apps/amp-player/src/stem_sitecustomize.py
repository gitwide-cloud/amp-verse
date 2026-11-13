"""AMP-owned stem runtime compatibility fix for workers embedded in older players.

Installed only in AMP's dedicated optional environment. It does nothing outside
the AMP stem-worker environment, including ordinary pip/maintenance commands.
New workers explicitly hide children themselves; the flags are idempotent.
"""
import os

if os.name == "nt" and os.environ.get("AMP_STEM_FFMPEG"):
    import subprocess

    class _AmpHiddenPopen(subprocess.Popen):
        def __init__(self, *args, **kwargs):
            kwargs["creationflags"] = kwargs.get("creationflags", 0) | subprocess.CREATE_NO_WINDOW
            startup = kwargs.get("startupinfo") or subprocess.STARTUPINFO()
            startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
            startup.wShowWindow = subprocess.SW_HIDE
            kwargs["startupinfo"] = startup
            super().__init__(*args, **kwargs)

    subprocess.Popen = _AmpHiddenPopen
