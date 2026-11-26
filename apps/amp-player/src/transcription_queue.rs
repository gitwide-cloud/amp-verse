//! Sequential, restartable folder transcription; never rewrites source recordings.
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub root: PathBuf,
    pub relative: PathBuf,
    pub source: PathBuf,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Item {
    pub input: Input,
    pub state: String,
    pub error: String,
    #[serde(default)]
    fingerprint: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Settings {
    pub vocals: bool,
    pub gpu: bool,
    pub language: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Queue {
    #[serde(default)]
    pub settings: Option<Settings>,
    pub items: Vec<Item>,
}
fn fingerprint(source: &Path) -> String {
    fs::metadata(source)
        .ok()
        .and_then(|m| {
            Some(format!(
                "{}:{}",
                m.len(),
                m.modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?
                    .as_nanos()
            ))
        })
        .unwrap_or_default()
}
fn queue_file() -> Result<PathBuf, String> {
    Ok(local_store()
        .ok_or("Cannot locate Library settings")?
        .with_file_name("transcription-queue.ampdb"))
}
impl Queue {
    pub fn load() -> Result<Self, String> {
        Self::load_at(&queue_file()?)
    }
    fn load_at(file: &Path) -> Result<Self, String> {
        let mut queue: Self = amp_core::catalog_db::local_record(file, "folder-queue")?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        for item in &mut queue.items {
            if item.state == "Transcribing" {
                item.state = "Queued".into();
            }
        }
        Ok(queue)
    }
    pub fn save(&self) -> Result<(), String> {
        self.save_at(&queue_file()?)
    }
    fn save_at(&self, file: &Path) -> Result<(), String> {
        amp_core::catalog_db::save_local_record(
            file,
            "folder-queue",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )
    }
    pub fn add(&mut self, inputs: Vec<Input>) -> usize {
        let mut count = 0;
        let mut known: std::collections::HashMap<PathBuf, usize> = self
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| (item.input.source.clone(), i))
            .collect();
        for input in inputs {
            let fingerprint = fingerprint(&input.source);
            if let Some(index) = known.get(&input.source) {
                let item = &mut self.items[*index];
                if item.fingerprint != fingerprint {
                    item.state = "Queued".into();
                    item.error.clear();
                    item.fingerprint = fingerprint;
                    count += 1;
                }
                continue;
            }
            known.insert(input.source.clone(), self.items.len());
            self.items.push(Item {
                input,
                state: "Queued".into(),
                error: String::new(),
                fingerprint,
            });
            count += 1;
        }
        count
    }
    pub fn pending(&self) -> usize {
        self.items.iter().filter(|i| i.state == "Queued").count()
    }
    pub fn failed(&self) -> usize {
        self.items.iter().filter(|i| i.state == "Failed").count()
    }
    pub fn retry(&mut self) {
        for item in &mut self.items {
            if item.state == "Failed" {
                item.state = "Queued".into();
                item.error.clear();
            }
        }
    }
    pub fn summary(&self) -> String {
        format!(
            "{} recordings · {} queued · {} saved · {} already transcribed · {} failed",
            self.items.len(),
            self.pending(),
            self.items.iter().filter(|i| i.state == "Saved").count(),
            self.items
                .iter()
                .filter(|i| i.state == "Already transcribed")
                .count(),
            self.failed()
        )
    }
}
pub(super) fn scan(
    folder: &Path,
    library: &Path,
    cancel: &AtomicBool,
) -> Result<Vec<Input>, String> {
    let folder = folder.canonicalize().map_err(|e| e.to_string())?;
    let mut roots = vec![library.canonicalize().map_err(|e| e.to_string())?];
    for source in amp_core::music_sources::list(library)? {
        if let Ok(root) = source.root.canonicalize() {
            roots.push(root);
        }
    }
    roots.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    let fallback = folder.clone();
    let mut dirs = vec![folder];
    let mut inputs = Vec::new();
    while let Some(dir) = dirs.pop() {
        if cancel.load(Relaxed) {
            return Err("Folder scan cancelled; existing queue kept".into());
        }
        for entry in
            fs::read_dir(&dir).map_err(|e| format!("Cannot read {}: {e}", dir.display()))?
        {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if ![
                    "ampdj",
                    ".ampverse",
                    ".git",
                    "$recycle.bin",
                    "system volume information",
                ]
                .contains(&name.as_str())
                {
                    dirs.push(entry.path());
                }
            } else if kind.is_file() && supported(&entry.path()) {
                let source = entry.path();
                let owner = roots
                    .iter()
                    .find(|root| source.starts_with(root))
                    .unwrap_or(&fallback);
                let relative = source
                    .strip_prefix(owner)
                    .map_err(|e| e.to_string())?
                    .to_path_buf();
                inputs.push(Input {
                    root: owner.clone(),
                    relative,
                    source,
                });
            }
        }
    }
    inputs.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(inputs)
}
fn supported(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        [
            "mp3", "wav", "flac", "m4a", "m4b", "ogg", "opus", "aac", "wma", "aif", "aiff",
        ]
        .contains(&s.to_ascii_lowercase().as_str())
    })
}
fn one(
    input: &Input,
    options: &Options,
    cancel: &AtomicBool,
    tx: &mpsc::SyncSender<Event>,
) -> Result<(Transcript, bool), String> {
    let existing = amp_core::transcripts::load(&input.root, &input.relative)?;
    if let Some(doc) = &existing
        && doc.source_sha256 == sha(&input.source, cancel)?
    {
        return Ok((doc.clone(), true));
    }
    let doc = recognize(input, options, cancel, tx)?;
    if cancel.load(Relaxed) {
        return Err("Transcription queue paused".into());
    }
    let saved = amp_core::transcripts::save(
        &input.root,
        &input.relative,
        &doc,
        existing.map_or(0, |d| d.revision),
    )?;
    Ok((saved, false))
}
pub(super) fn run(
    queue: Queue,
    options: Options,
    cancel: Arc<AtomicBool>,
    tx: mpsc::SyncSender<Event>,
) {
    match queue_file() {
        Ok(file) => run_at(queue, options, cancel, tx, &file),
        Err(error) => {
            let _ = tx.send(Event::BatchFinished(error));
        }
    }
}
fn run_at(
    mut queue: Queue,
    options: Options,
    cancel: Arc<AtomicBool>,
    tx: mpsc::SyncSender<Event>,
    file: &Path,
) {
    let result = (|| {
        for index in 0..queue.items.len() {
            if cancel.load(Relaxed) {
                break;
            }
            if queue.items[index].state != "Queued" {
                continue;
            }
            queue.items[index].state = "Transcribing".into();
            queue.save_at(file)?;
            let _ = tx.send(Event::Queue(Box::new(queue.clone())));
            let _ = tx.send(Event::Progress(0.));
            let _ = tx.send(Event::Status(format!(
                "Transcribing {}",
                queue.items[index].input.relative.display()
            )));
            let outcome = one(&queue.items[index].input, &options, &cancel, &tx);
            finish(&mut queue.items[index], &outcome, cancel.load(Relaxed));
            if let Ok((doc, _)) = outcome {
                let _ = tx.send(Event::BatchReady(queue.items[index].input.clone(), doc));
            }
            queue.save_at(file)?;
            let _ = tx.send(Event::Queue(Box::new(queue.clone())));
        }
        Ok::<_, String>(())
    })();
    let status = match result {
        Ok(()) => format!(
            "{} · {}",
            if cancel.load(Relaxed) {
                "Queue paused; resume when ready"
            } else {
                "Queue finished"
            },
            queue.summary()
        ),
        Err(e) => format!(
            "Queue stopped safely: {e}. Completed transcripts and local drafts were retained."
        ),
    };
    let _ = tx.send(Event::BatchFinished(status));
}
fn finish(item: &mut Item, outcome: &Result<(Transcript, bool), String>, cancelled: bool) {
    match outcome {
        Ok((_, skipped)) => {
            item.state = if *skipped {
                "Already transcribed"
            } else {
                "Saved"
            }
            .into();
            item.error.clear();
        }
        Err(_) if cancelled => {
            item.state = "Queued".into();
            item.error.clear();
        }
        Err(error) => {
            item.state = "Failed".into();
            item.error = error.clone();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input(root: &Path, name: &str) -> Input {
        Input {
            root: root.into(),
            relative: name.into(),
            source: root.join(name),
        }
    }
    #[test]
    fn nested_scan_excludes_managed_data_and_non_audio() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("episodes/ampdj")).unwrap();
        for name in [
            "talk.MP3",
            "episodes/book.m4b",
            "episodes/notes.txt",
            "episodes/ampdj/vocals.wav",
        ] {
            fs::write(temp.path().join(name), b"audio").unwrap();
        }
        let found = scan(
            &temp.path().join("episodes"),
            temp.path(),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].root, temp.path().canonicalize().unwrap());
        assert_eq!(found[0].relative, Path::new("episodes/book.m4b"));
        assert!(scan(temp.path(), temp.path(), &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn duplicate_folders_do_not_duplicate_jobs_and_running_job_recovers() {
        let temp = tempfile::tempdir().unwrap();
        let mut q = Queue::default();
        assert_eq!(q.add(vec![input(temp.path(), "one.wav")]), 1);
        assert_eq!(q.add(vec![input(temp.path(), "one.wav")]), 0);
        q.items[0].state = "Transcribing".into();
        let file = temp.path().join("queue.ampdb");
        q.save_at(&file).unwrap();
        assert_eq!(Queue::load_at(&file).unwrap().pending(), 1);
    }
    #[test]
    fn failure_does_not_remove_following_jobs_and_cancel_keeps_pending() {
        let temp = tempfile::tempdir().unwrap();
        let mut q = Queue::default();
        q.add(vec![
            input(temp.path(), "bad.wav"),
            input(temp.path(), "next.wav"),
        ]);
        finish(&mut q.items[0], &Err("bad recording".into()), false);
        assert_eq!(q.failed(), 1);
        assert_eq!(q.pending(), 1);
        finish(&mut q.items[1], &Err("cancelled".into()), true);
        assert_eq!(q.items[1].state, "Queued");
        q.retry();
        assert_eq!(q.pending(), 2);
    }
    #[test]
    fn queue_continues_after_failure_and_reuses_completed_transcript() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("bad.wav"), b"invalid audio").unwrap();
        fs::write(temp.path().join("done.wav"), b"existing audio").unwrap();
        let checksum = sha(&temp.path().join("done.wav"), &AtomicBool::new(false)).unwrap();
        let doc = Transcript {
            version: 1,
            source_sha256: checksum,
            model: "base".into(),
            language: "en".into(),
            duration: 2.,
            input: "original".into(),
            segments: vec![amp_core::transcripts::Segment {
                start: 0.,
                end: 1.,
                text: "Already saved".into(),
            }],
            revision: 0,
            device_id: String::new(),
        };
        amp_core::storage::register(temp.path());
        amp_core::transcripts::save(temp.path(), Path::new("done.wav"), &doc, 0).unwrap();
        let mut q = Queue::default();
        q.add(vec![
            input(temp.path(), "bad.wav"),
            input(temp.path(), "done.wav"),
        ]);
        let options = Options {
            python: temp.path().join("not-installed-python"),
            addon: temp.path().join("addons"),
            language: String::new(),
            vocals: false,
            gpu: false,
        };
        let (tx, rx) = mpsc::sync_channel(8);
        let drain = std::thread::spawn(move || while rx.recv().is_ok() {});
        let file = temp.path().join("queue.ampdb");
        run_at(q, options, Arc::new(AtomicBool::new(false)), tx, &file);
        drain.join().unwrap();
        let saved = Queue::load_at(&file).unwrap();
        assert_eq!(saved.items[0].state, "Failed");
        assert_eq!(saved.items[1].state, "Already transcribed");
        assert_eq!(saved.pending(), 0);
        assert_eq!(
            amp_core::transcripts::load(temp.path(), Path::new("done.wav"))
                .unwrap()
                .unwrap()
                .revision,
            1
        );
    }
    #[test]
    fn rescan_requeues_changed_audio_only() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("talk.wav"), b"old").unwrap();
        let mut q = Queue::default();
        q.add(vec![input(temp.path(), "talk.wav")]);
        q.items[0].state = "Saved".into();
        assert_eq!(q.add(vec![input(temp.path(), "talk.wav")]), 0);
        fs::write(temp.path().join("talk.wav"), b"longer new recording").unwrap();
        assert_eq!(q.add(vec![input(temp.path(), "talk.wav")]), 1);
        assert_eq!(q.pending(), 1);
    }
    #[test]
    fn scanning_parent_preserves_known_library_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let library = temp.path().join("library");
        fs::create_dir_all(&library).unwrap();
        fs::create_dir_all(temp.path().join("external")).unwrap();
        fs::write(library.join("talk.mp3"), b"audio").unwrap();
        fs::write(temp.path().join("external/speech.mp3"), b"audio").unwrap();
        let found = scan(temp.path(), &library, &AtomicBool::new(false)).unwrap();
        assert_eq!(found.len(), 2);
        let known = found
            .iter()
            .find(|input| input.relative == Path::new("talk.mp3"))
            .unwrap();
        assert_eq!(known.root, library.canonicalize().unwrap());
        let external = found
            .iter()
            .find(|input| input.relative == Path::new("external/speech.mp3"))
            .unwrap();
        assert_eq!(external.root, temp.path().canonicalize().unwrap());
    }
}
