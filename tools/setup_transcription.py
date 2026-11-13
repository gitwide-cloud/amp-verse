"""Install the optional local speech runtime. No audio is uploaded."""
import argparse, os, subprocess, sys
from pathlib import Path
VERSION = "1.2.1"
def setup(destination):
    destination=Path(destination);python=destination/("Scripts/python.exe" if os.name=="nt" else "bin/python")
    flags=subprocess.CREATE_NO_WINDOW if os.name=="nt" else 0
    if not python.is_file():
        print("Creating optional speech runtime",flush=True)
        subprocess.run([sys.executable,"-m","venv",str(destination)],check=True,creationflags=flags)
    print("Installing local speech recognition tools from PyPI",flush=True)
    subprocess.run([str(python),"-m","pip","install","--disable-pip-version-check","--index-url","https://pypi.org/simple","faster-whisper=="+VERSION],check=True,creationflags=flags)
    print("Speech tools ready. The first transcription downloads the base model.",flush=True)
if __name__=="__main__":
    parser=argparse.ArgumentParser();parser.add_argument("--dest",required=True);setup(parser.parse_args().dest)
