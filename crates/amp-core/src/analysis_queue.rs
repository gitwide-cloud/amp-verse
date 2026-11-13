//! Durable shared FIFO in the small catalog index. Shared analysis leases still fence publication.
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone)]
pub struct Queue {
    file: PathBuf,
    root: PathBuf,
    shared: bool,
    _owner: Arc<crate::coordination::Lock>,
}
pub struct Job {
    pub path: PathBuf,
    token: String,
}
#[derive(Default, Debug, Clone, Copy)]
pub struct Stats {
    pub total: usize,
    pub pending: usize,
    pub done: usize,
    pub failed: usize,
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(crate) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS analysis_jobs(seq INTEGER PRIMARY KEY AUTOINCREMENT,path TEXT NOT NULL UNIQUE,stamp TEXT NOT NULL,state TEXT NOT NULL,token TEXT,error TEXT,attempts INTEGER NOT NULL DEFAULT 0,expires INTEGER NOT NULL DEFAULT 0,device TEXT NOT NULL DEFAULT ''); CREATE INDEX IF NOT EXISTS analysis_fifo ON analysis_jobs(state,seq);";
impl Queue {
    /// Seed trusted catalog/cache entries without stat-ing every file first.
    /// Discovery verifies changes independently; existing shared work is authoritative.
    pub fn seed_known(&self, tracks: &[crate::Track]) -> Result<usize, String> {
        let mut rows = Vec::new();
        for track in tracks {
            if !crate::media_kind::is_music(&track.path, &track.preparation) {
                continue;
            }
            let path = track
                .path
                .to_str()
                .ok_or("Song path is not UTF-8")?
                .replace('\\', "/");
            if Path::new(&path).is_absolute()
                || path.contains(':')
                || Path::new(&path)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("Queue song references must be relative".into());
            }
            let stamp = track
                .preparation
                .extensions
                .get("analysis_source_stat")
                .and_then(|s| {
                    Some(format!(
                        "{}:{}",
                        s["size"].as_u64()?,
                        s["modified_ns"].as_str()?
                    ))
                })
                .unwrap_or_default();
            let complete = track.preparation.analysis.is_some()
                && track.preparation.source_sha256.is_some()
                && !stamp.is_empty();
            rows.push((path, stamp, complete));
        }
        let mut added = 0;
        for chunk in rows.chunks(500) {
            let missing = self.access(false, |db| {
                for (path, _, _) in chunk {
                    let found: bool = db
                        .query_row(
                            "SELECT EXISTS(SELECT 1 FROM analysis_jobs WHERE path=?1)",
                            [path],
                            |r| r.get(0),
                        )
                        .map_err(err)?;
                    if !found {
                        return Ok(true);
                    }
                }
                Ok(false)
            })?;
            if !missing {
                continue;
            }
            added+=self.access(true,|db|{let tx=db.transaction().map_err(err)?;let mut n=0;for(path,stamp,complete)in chunk{n+=tx.execute("INSERT OR IGNORE INTO analysis_jobs(path,stamp,state) VALUES(?1,?2,?3)",params![path,stamp,if *complete{"done"}else{"pending"}]).map_err(err)?;}tx.commit().map_err(err)?;Ok(n)})?;
        }
        Ok(added)
    }
    pub fn for_library(root: &Path, role: &str) -> Result<Self, String> {
        use sha2::Digest;
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_DATA_HOME"))
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
            .ok_or("Local queue storage unavailable")?;
        let id = sha2::Sha256::digest(format!("{role}:{}", root.display()).as_bytes());
        let mut q = Self::open(
            &base
                .join("AMPVerse/work")
                .join(format!("analysis-{id:x}.ampdb")),
            root,
        )?;
        if !crate::catalog_db::path(root).is_file() {
            crate::catalog_db::compact(root)?;
        }
        if crate::catalog_db::path(root).is_file() {
            q.shared = true;
            let initialized = q.access(false, |db| {
                db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='analysis_jobs')",
                    [],
                    |r| r.get::<_, bool>(0),
                )
                .map_err(err)
            })?;
            if !initialized {
                q.access(true, |db| {
                    db.execute_batch(SCHEMA).map_err(err)?;
                    Ok(())
                })?;
            }
        }
        Ok(q)
    }
    pub fn open(file: &Path, root: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(file.parent().ok_or("Queue folder missing")?).map_err(err)?;
        let owner = Arc::new(crate::coordination::Lock::acquire(
            &file.with_extension("owner"),
        )?);
        let q = Self {
            file: file.into(),
            root: root.into(),
            shared: false,
            _owner: owner,
        };
        q.access(true,|db|{db.execute_batch(SCHEMA).map_err(err)?;db.execute("UPDATE analysis_jobs SET state='pending',token=NULL,expires=0 WHERE state='running'",[]).map_err(err)?;Ok(())})?;
        Ok(q)
    }
    fn access<T>(
        &self,
        write: bool,
        work: impl FnOnce(&mut Connection) -> Result<T, String>,
    ) -> Result<T, String> {
        if self.shared {
            return crate::catalog_db::queue_access(&self.root, write, work);
        }
        let mut db = Connection::open(&self.file).map_err(err)?;
        db.busy_timeout(std::time::Duration::from_secs(10))
            .map_err(err)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")
            .map_err(err)?;
        work(&mut db)
    }
    /// Filesystem discovery happens outside the index lock. Only changed/new songs append.
    pub fn enqueue(
        &self,
        paths: impl IntoIterator<Item = (PathBuf, bool)>,
    ) -> Result<usize, String> {
        let mut prepared = Vec::new();
        for (path, complete) in paths {
            let path = PathBuf::from(
                path.to_str()
                    .ok_or("Song path is not UTF-8")?
                    .replace('\\', "/"),
            );
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
                || path.to_string_lossy().contains(':')
            {
                return Err("Queue song references must be relative to the library root".into());
            }
            let Ok(meta) = std::fs::metadata(crate::music_sources::audio_path(&self.root, &path))
            else {
                continue;
            };
            let stamp = format!(
                "{}:{}",
                meta.len(),
                meta.modified()
                    .map_err(err)?
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(err)?
                    .as_nanos()
            );
            prepared.push((
                path.to_str().ok_or("Song path is not UTF-8")?.to_owned(),
                stamp,
                complete,
            ));
        }
        self.enqueue_rows(prepared)
    }
    /// Use directory metadata instead of a second per-file NAS stat pass.
    pub fn enqueue_tracks(&self, tracks: &[crate::Track]) -> Result<usize, String> {
        let mut rows = Vec::new();
        let mut fallback = Vec::new();
        for track in tracks {
            if !crate::media_kind::is_music(&track.path, &track.preparation) {
                continue;
            }
            let key = track
                .path
                .to_str()
                .ok_or("Song path is not UTF-8")?
                .replace('\\', "/");
            if Path::new(&key).is_absolute()
                || key.contains(':')
                || Path::new(&key)
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("Queue references must be relative".into());
            }
            let stamp = track
                .preparation
                .extensions
                .get("scan_source_stat")
                .and_then(|s| {
                    Some(format!(
                        "{}:{}",
                        s["size"].as_u64()?,
                        s["modified_ns"].as_str()?
                    ))
                });
            if let Some(stamp) = stamp {
                rows.push((key, stamp, false));
            } else {
                fallback.push((track.path.clone(), false));
            }
        }
        Ok(self.enqueue_rows(rows)? + self.enqueue(fallback)?)
    }
    fn enqueue_rows(&self, prepared: Vec<(String, String, bool)>) -> Result<usize, String> {
        let mut added = 0;
        for chunk in prepared.chunks(500) {
            let changed = self.access(false, |db| {
                for (key, stamp, _) in chunk {
                    let old: Option<(String, String)> = db
                        .query_row(
                            "SELECT stamp,state FROM analysis_jobs WHERE path=?1",
                            [key],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()
                        .map_err(err)?;
                    if old.is_none_or(|(old, state)| old != *stamp && state != "running") {
                        return Ok(true);
                    }
                }
                Ok(false)
            })?;
            if !changed {
                continue;
            }
            added += self.access(true, |db| {
                let tx = db.transaction().map_err(err)?;
                let mut added = 0;
                for (key, stamp, complete) in chunk {
                    let old: Option<(String, String)> = tx
                        .query_row(
                            "SELECT stamp,state FROM analysis_jobs WHERE path=?1",
                            [key],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()
                        .map_err(err)?;
                    let mut complete = *complete;
                    if let Some((old, state)) = old {
                        if old == *stamp || state == "running" {
                            continue;
                        }
                        if old.is_empty() && state == "pending" {
                            tx.execute(
                                "UPDATE analysis_jobs SET stamp=?2 WHERE path=?1",
                                params![key, stamp],
                            )
                            .map_err(err)?;
                            continue;
                        }
                        complete = false;
                        tx.execute("DELETE FROM analysis_jobs WHERE path=?1", [key])
                            .map_err(err)?;
                    }
                    tx.execute(
                        "INSERT INTO analysis_jobs(path,stamp,state) VALUES(?1,?2,?3)",
                        params![key, stamp, if complete { "done" } else { "pending" }],
                    )
                    .map_err(err)?;
                    added += 1;
                }
                tx.commit().map_err(err)?;
                Ok(added)
            })?;
        }
        Ok(added)
    }
    pub fn cached_complete(root: &Path, track: &crate::Track) -> bool {
        let p = &track.preparation;
        if !crate::media_kind::is_music(&track.path, p) {
            return true;
        }
        if p.analysis.is_none() || p.source_sha256.is_none() {
            return false;
        }
        let Ok(meta) = std::fs::metadata(crate::music_sources::audio_path(root, &track.path))
        else {
            return false;
        };
        let ns = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos().to_string());
        p.extensions.get("analysis_source_stat").is_some_and(|s| {
            s["size"].as_u64() == Some(meta.len()) && s["modified_ns"].as_str() == ns.as_deref()
        })
    }

    pub fn next(&self) -> Result<Option<Job>, String> {
        let waiting=self.access(false,|db|db.query_row("SELECT EXISTS(SELECT 1 FROM analysis_jobs WHERE state='pending' OR (state='running' AND expires<?1))",[crate::coordination::now()? as i64],|r|r.get::<_,bool>(0)).map_err(err))?;
        if !waiting {
            return Ok(None);
        }
        self.access(true,|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(err)?;let now=crate::coordination::now()? as i64;
        tx.execute("UPDATE analysis_jobs SET state='pending',token=NULL WHERE state='running' AND expires<?1",[now]).map_err(err)?;
        let path:Option<String>=tx.query_row("SELECT path FROM analysis_jobs WHERE state='pending' ORDER BY seq LIMIT 1",[],|r|r.get(0)).optional().map_err(err)?;
        let job=if let Some(path)=path{let token=crate::coordination::token();tx.execute("UPDATE analysis_jobs SET state='running',token=?2,expires=?3,device=?4,attempts=attempts+1 WHERE path=?1",params![path,token,now+90000,crate::coordination::device_id()?]).map_err(err)?;Some(Job{path:path.into(),token})}else{None};tx.commit().map_err(err)?;Ok(job)
    })
    }
    pub fn heartbeat(&self, job: &Job) -> Result<(), String> {
        self.access(true,|db|{let now=crate::coordination::now()? as i64;let n=db.execute("UPDATE analysis_jobs SET expires=?3 WHERE path=?1 AND token=?2 AND state='running' AND expires>=?4",params![job.path.to_str(),job.token,now+90000,now]).map_err(err)?;if n!=1{return Err("Queue claim expired".into());}Ok(())})
    }
    pub fn defer(&self, job: &Job) -> Result<(), String> {
        self.access(true,|db|{let now=crate::coordination::now()? as i64;let count=db.execute("UPDATE analysis_jobs SET expires=?3 WHERE path=?1 AND token=?2 AND state='running' AND expires>=?4",params![job.path.to_str(),job.token,now+60000,now]).map_err(err)?;if count!=1{return Err("Queue claim expired".into());}Ok(())})
    }
    pub fn keep_alive(&self, job: &Job) -> KeepAlive {
        let (tx, rx) = std::sync::mpsc::channel();
        let q = self.clone();
        let job = Job {
            path: job.path.clone(),
            token: job.token.clone(),
        };
        let worker = std::thread::spawn(move || {
            while rx.recv_timeout(std::time::Duration::from_secs(20)).is_err() {
                if q.heartbeat(&job).is_err() {
                    break;
                }
            }
        });
        KeepAlive {
            stop: Some(tx),
            worker: Some(worker),
        }
    }
    pub fn finish(&self, job: &Job, result: Result<(), &str>) -> Result<(), String> {
        let stamp = std::fs::metadata(crate::music_sources::audio_path(&self.root, &job.path))
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
            .unwrap_or_default();
        self.access(true,|db|{
        let(state,error)=match result{Ok(())=>("done",String::new()),Err(e)=>("failed",e.chars().take(2048).collect())};
        let n=db.execute("UPDATE analysis_jobs SET state=?3,error=?4,token=NULL,expires=0,stamp=?6 WHERE path=?1 AND token=?2 AND state='running' AND expires>=?5",params![job.path.to_str(),job.token,state,error,crate::coordination::now()? as i64,stamp]).map_err(err)?;
        if n!=1{return Err("Queue ownership changed; completion rejected".into());}Ok(())
    })
    }
    pub fn retry_failed(&self) -> Result<usize, String> {
        self.access(true,|db|{
        let tx=db.transaction().map_err(err)?;
        let count:usize=tx.query_row("SELECT COUNT(*) FROM analysis_jobs WHERE state='failed'",[],|r|r.get(0)).map_err(err)?;
        tx.execute_batch("CREATE TEMP TABLE retry_jobs AS SELECT path,stamp,attempts FROM analysis_jobs WHERE state='failed' ORDER BY seq; DELETE FROM analysis_jobs WHERE state='failed'; INSERT INTO analysis_jobs(path,stamp,state,attempts) SELECT path,stamp,'pending',attempts FROM retry_jobs ORDER BY rowid; DROP TABLE retry_jobs;").map_err(err)?;tx.commit().map_err(err)?;Ok(count)
    })
    }
    pub fn stats(&self) -> Result<Stats, String> {
        self.access(false, |db| {
            let mut stats = Stats::default();
            let mut q = db
                .prepare("SELECT state,COUNT(*) FROM analysis_jobs GROUP BY state")
                .map_err(err)?;
            for row in q
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, usize>(1)?)))
                .map_err(err)?
            {
                let (s, n) = row.map_err(err)?;
                stats.total += n;
                match s.as_str() {
                    "done" => stats.done += n,
                    "failed" => stats.failed += n,
                    _ => stats.pending += n,
                }
            }
            Ok(stats)
        })
    }
}
pub struct KeepAlive {
    stop: Option<std::sync::mpsc::Sender<()>>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Drop for KeepAlive {
    fn drop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_observations_are_not_confirmed_with_a_second_stat() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("music");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("song.wav"), b"audio").unwrap();
        let library = crate::scan_job_inputs(&root).unwrap();
        std::fs::remove_file(root.join("song.wav")).unwrap();
        let q = Queue::open(&dir.path().join("queue.ampdb"), &root).unwrap();
        assert_eq!(q.enqueue_tracks(&library.tracks).unwrap(), 1);
        assert_eq!(q.next().unwrap().unwrap().path, Path::new("song.wav"));
    }
    #[test]
    fn cached_waiting_songs_start_without_a_filesystem_rescan() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("offline-root");
        let q = Queue::open(&dir.path().join("queue.ampdb"), &root).unwrap();
        let tracks = vec![crate::Track {
            path: "new.wav".into(),
            title: "New".into(),
            preparation: Default::default(),
        }];
        assert_eq!(q.seed_known(&tracks).unwrap(), 1);
        let job = q.next().unwrap().unwrap();
        assert_eq!(job.path, Path::new("new.wav"));
        q.finish(&job, Err("source offline")).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("new.wav"), b"returned").unwrap();
        q.enqueue([("new.wav".into(), false)]).unwrap();
        assert!(q.next().unwrap().is_some());
    }
    #[test]
    fn queue_paths_use_portable_relative_separators() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("music");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/song.wav"), b"audio").unwrap();
        let q = Queue::open(&dir.path().join("queue.ampdb"), &root).unwrap();
        q.enqueue([(PathBuf::from(r"sub\song.wav"), false)])
            .unwrap();
        let job = q.next().unwrap().unwrap();
        assert_eq!(job.path.to_str().unwrap(), "sub/song.wav");
        assert!(
            q.enqueue([(PathBuf::from(r"C:\Music\song.wav"), false)])
                .is_err()
        );
    }
    #[test]
    fn shared_index_claims_are_exclusive_expire_and_do_not_rewrite_unchanged_work() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("music");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("song.wav"), b"audio").unwrap();
        crate::catalog_db::compact(&root).unwrap();
        let mut a = Queue::open(&dir.path().join("a.ampdb"), &root).unwrap();
        a.shared = true;
        a.access(true, |db| db.execute_batch(SCHEMA).map_err(err))
            .unwrap();
        let mut b = Queue::open(&dir.path().join("b.ampdb"), &root).unwrap();
        b.shared = true;
        a.enqueue([("song.wav".into(), false)]).unwrap();
        let barrier = std::sync::Barrier::new(2);
        let (old, other) = std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                barrier.wait();
                a.next().unwrap()
            });
            let second = scope.spawn(|| {
                barrier.wait();
                b.next().unwrap()
            });
            (first.join().unwrap(), second.join().unwrap())
        });
        assert_ne!(old.is_some(), other.is_some());
        let old = old.or(other).unwrap();
        crate::catalog_db::write(
            &root.join("ampdj/tracks/song.wav.json"),
            &serde_json::to_vec(&crate::Preparation::default()).unwrap(),
        )
        .unwrap();
        assert_eq!(a.stats().unwrap().total, 1);
        assert!(b.next().unwrap().is_none());
        a.access(true, |db| {
            db.execute("UPDATE analysis_jobs SET expires=0", [])
                .map(|_| ())
                .map_err(err)
        })
        .unwrap();
        let next = b.next().unwrap().unwrap();
        assert_eq!(next.path, old.path);
        assert!(a.finish(&old, Ok(())).is_err());
        assert!(a.heartbeat(&old).is_err());
        assert!(a.defer(&old).is_err());
        b.finish(&next, Ok(())).unwrap();
        let before = std::fs::read(crate::catalog_db::path(&root)).unwrap();
        assert_eq!(a.enqueue([("song.wav".into(), false)]).unwrap(), 0);
        assert!(a.next().unwrap().is_none());
        assert_eq!(
            before,
            std::fs::read(crate::catalog_db::path(&root)).unwrap()
        );
        assert_eq!(a.stats().unwrap().done, 1);
    }
    #[test]
    fn fifo_changes_resume_and_failure_do_not_restart_completed_work() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("music");
        std::fs::create_dir(&root).unwrap();
        for name in ["a.wav", "b.wav", "c.wav"] {
            std::fs::write(root.join(name), b"audio").unwrap();
        }
        let file = dir.path().join("queue.ampdb");
        let q = Queue::open(&file, &root).unwrap();
        assert!(q.enqueue([(root.join("a.wav"), false)]).is_err());
        assert!(q.enqueue([("../escape.wav".into(), false)]).is_err());
        q.enqueue([("a.wav".into(), false), ("b.wav".into(), false)])
            .unwrap();
        let a = q.next().unwrap().unwrap();
        assert_eq!(a.path, Path::new("a.wav"));
        q.finish(&a, Ok(())).unwrap();
        let b = q.next().unwrap().unwrap();
        q.enqueue([("c.wav".into(), false)]).unwrap();
        drop(q);
        let q = Queue::open(&file, &root).unwrap();
        assert!(q.finish(&b, Ok(())).is_err());
        let b = q.next().unwrap().unwrap();
        assert_eq!(b.path, Path::new("b.wav"));
        q.finish(&b, Err("bad audio")).unwrap();
        let c = q.next().unwrap().unwrap();
        assert_eq!(c.path, Path::new("c.wav"));
        q.finish(&c, Ok(())).unwrap();
        assert_eq!(
            q.enqueue([
                ("a.wav".into(), false),
                ("b.wav".into(), false),
                ("c.wav".into(), false)
            ])
            .unwrap(),
            0
        );
        assert!(q.next().unwrap().is_none());
        std::fs::write(root.join("a.wav"), b"changed audio").unwrap();
        q.enqueue([("a.wav".into(), false)]).unwrap();
        assert_eq!(q.next().unwrap().unwrap().path, Path::new("a.wav"));
        assert_eq!(q.stats().unwrap().failed, 1);
    }
    #[test]
    fn six_thousand_jobs_are_persistent_without_retaining_preparation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("music");
        std::fs::create_dir(&root).unwrap();
        for i in 0..6001 {
            std::fs::write(root.join(format!("{i}.wav")), b"audio").unwrap();
        }
        let file = dir.path().join("queue.ampdb");
        let q = Queue::open(&file, &root).unwrap();
        q.enqueue((0..6001).map(|i| (PathBuf::from(format!("{i}.wav")), false)))
            .unwrap();
        // Simulate persisted completions at scale; actual claim/finish fencing is tested above.
        q.access(true, |db| {
            db.execute("UPDATE analysis_jobs SET state='done' WHERE seq<=5001", [])
                .map(|_| ())
                .map_err(err)
        })
        .unwrap();
        drop(q);
        let q = Queue::open(&file, &root).unwrap();
        assert_eq!(q.stats().unwrap().done, 5001);
        assert_eq!(q.stats().unwrap().pending, 1000);
        assert_eq!(q.next().unwrap().unwrap().path, Path::new("5001.wav"));
    }
}
