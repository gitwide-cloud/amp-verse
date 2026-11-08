//! Offline-first library, preparation data and process audio adapter. No GUI or network dependency.
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Preparation {
    pub cue_in: f64,
    pub cue_out: Option<f64>,
    pub bpm: Option<f64>,
    pub beat_offset: Option<f64>,
    #[serde(default)]
    pub stems: Vec<String>,
}
impl Preparation {
    pub fn validate(&self) -> Result<(), String> {
        if !self.cue_in.is_finite() || self.cue_in < 0.0 {
            return Err("Cue IN must be finite and nonnegative".into());
        }
        if self
            .cue_out
            .is_some_and(|x| !x.is_finite() || x <= self.cue_in)
        {
            return Err("Cue OUT must follow IN".into());
        }
        if self.bpm.is_some_and(|x| !x.is_finite() || x <= 0.0) {
            return Err("BPM must be positive".into());
        }
        if self.beat_offset.is_some_and(|x| !x.is_finite() || x < 0.0) {
            return Err("Beat offset must be nonnegative".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    /// Relative to the configured music root; never a remote executable or URL.
    pub path: PathBuf,
    pub title: String,
    pub preparation: Preparation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub name: String,
    pub folders: Vec<Folder>,
    pub tracks: Vec<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub tracks: Vec<Track>,
    pub tree: Folder,
}

pub fn sidecar(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(".amp.json");
    p.into()
}

pub fn scan(root: &Path) -> io::Result<Library> {
    let root = root.canonicalize()?;
    let mut tracks = Vec::new();
    fn visit(dir: &Path, root: &Path, tracks: &mut Vec<Track>) -> io::Result<Folder> {
        let mut folder = Folder {
            name: dir.file_name().unwrap_or_default().to_string_lossy().into(),
            folders: vec![],
            tracks: vec![],
        };
        let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let ty = entry.file_type()?;
            // Do not follow symlinks, including links outside the root or cyclic directories.
            if ty.is_symlink() {
                continue;
            }
            let path = entry.path();
            if ty.is_dir() {
                folder.folders.push(visit(&path, root, tracks)?);
            } else if ty.is_file() && supported(&path) {
                let preparation = match fs::read(sidecar(&path)) {
                    Ok(bytes) => {
                        serde_json::from_slice::<Preparation>(&bytes).map_err(io::Error::other)?
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Preparation::default(),
                    Err(e) => return Err(e),
                };
                preparation.validate().map_err(io::Error::other)?;
                folder.tracks.push(tracks.len());
                tracks.push(Track {
                    path: path.strip_prefix(root).unwrap().into(),
                    title: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    preparation,
                });
            }
        }
        Ok(folder)
    }
    let tree = visit(&root, &root, &mut tracks)?;
    Ok(Library { tracks, tree })
}
pub fn supported(p: &Path) -> bool {
    p.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_lowercase().as_str(),
            "mp3" | "wav" | "flac" | "ogg" | "opus" | "m4a" | "aac"
        )
    })
}

/// Resolve an untrusted playlist entry without escaping the library root.
pub fn resolve(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(io::Error::other("Expected a relative library path"));
    }
    let root = root.canonicalize()?;
    let file = root.join(relative).canonicalize()?;
    if !file.starts_with(root) || !file.is_file() {
        return Err(io::Error::other("Track outside library"));
    }
    Ok(file)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Playlist {
    pub version: u32,
    pub name: String,
    pub tracks: Vec<PathBuf>,
}
impl Playlist {
    pub fn load(path: &Path, root: &Path) -> io::Result<Self> {
        let list: Self = serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
        if list.version != 1 || list.tracks.is_empty() {
            return Err(io::Error::other("Expected nonempty version 1 playlist"));
        }
        for p in &list.tracks {
            resolve(root, p)?;
        }
        Ok(list)
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        fs::write(
            path,
            serde_json::to_vec_pretty(self).map_err(io::Error::other)?,
        )
    }
}

pub fn command(name: &str) -> Command {
    let mut cmd = Command::new(name);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}
pub fn cue_args(prep: &Preparation) -> Result<Vec<String>, String> {
    prep.validate()?;
    let mut args = vec!["-ss".into(), prep.cue_in.to_string()];
    if let Some(end) = prep.cue_out {
        args.extend(["-t".into(), (end - prep.cue_in).to_string()]);
    }
    Ok(args)
}

/// Small-footprint prototype adapter inspired by MP3 Lab's process lifecycle.
#[derive(Default)]
pub struct Deck {
    child: Option<Child>,
}
impl Deck {
    pub fn play(&mut self, root: &Path, track: &Track, volume: u8) -> Result<(), String> {
        self.stop();
        let path = resolve(root, &track.path).map_err(|e| e.to_string())?;
        let child = command("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "error", "-volume"])
            .arg(volume.min(100).to_string())
            .args(cue_args(&track.preparation)?)
            .arg("-i")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Install FFmpeg/ffplay and add it to PATH: {e}"))?;
        self.child = Some(child);
        Ok(())
    }
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    pub fn playing(&mut self) -> bool {
        match self.child.as_mut().map(Child::try_wait) {
            Some(Ok(None)) => true,
            _ => {
                self.stop();
                false
            }
        }
    }
}
impl Drop for Deck {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cues_reject_bad_ranges_and_numbers() {
        for p in [
            Preparation {
                cue_in: -1.,
                ..Default::default()
            },
            Preparation {
                cue_out: Some(0.),
                ..Default::default()
            },
            Preparation {
                bpm: Some(f64::NAN),
                ..Default::default()
            },
        ] {
            assert!(p.validate().is_err());
        }
        assert_eq!(
            cue_args(&Preparation {
                cue_in: 2.,
                cue_out: Some(5.),
                ..Default::default()
            })
            .unwrap(),
            ["-ss", "2", "-t", "3"]
        );
    }
    #[test]
    fn tree_playlist_and_sidecars_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("Albums")).unwrap();
        fs::write(root.join("Albums/a.MP3"), []).unwrap();
        fs::write(root.join("ignore.txt"), []).unwrap();
        let prep = Preparation {
            cue_in: 1.,
            cue_out: Some(3.),
            ..Default::default()
        };
        fs::write(
            sidecar(&root.join("Albums/a.MP3")),
            serde_json::to_vec(&prep).unwrap(),
        )
        .unwrap();
        let library = scan(root).unwrap();
        assert_eq!(library.tracks.len(), 1);
        assert_eq!(library.tree.folders[0].tracks, [0]);
        assert_eq!(library.tracks[0].preparation, prep);
        let list = Playlist {
            version: 1,
            name: "Evening".into(),
            tracks: vec!["Albums/a.MP3".into()],
        };
        list.save(&root.join("evening.json")).unwrap();
        assert_eq!(
            Playlist::load(&root.join("evening.json"), root)
                .unwrap()
                .tracks,
            list.tracks
        );
        assert!(resolve(root, Path::new("../outside.mp3")).is_err());
        assert!(resolve(root, &root.join("Albums/a.MP3")).is_err());
    }
    #[test]
    fn malformed_preparation_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.wav"), []).unwrap();
        fs::write(tmp.path().join("a.wav.amp.json"), "broken").unwrap();
        assert!(scan(tmp.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_escape_or_recurse() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.mp3"), []).unwrap();
        symlink(
            outside.path().join("secret.mp3"),
            root.path().join("link.mp3"),
        )
        .unwrap();
        symlink(root.path(), root.path().join("cycle")).unwrap();
        assert!(scan(root.path()).unwrap().tracks.is_empty());
        assert!(resolve(root.path(), Path::new("link.mp3")).is_err());
    }
}
