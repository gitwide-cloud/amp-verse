//! Device-local, immutable analysis observations. No source audio or manual cues are stored.
//! This is AMP's original Rust implementation, not a translation of another DJ engine.
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
const APPLICATION: i64 = 0x414d504b;
const VERSION: i64 = 1;
const MAX_DOCUMENT: usize = 65536;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Observation {
    pub source_sha256: String,
    pub producer: String,
    pub producer_version: String,
    pub settings_sha256: String,
    pub kind: String,
    pub data: serde_json::Value,
}
impl Observation {
    fn validate(&self) -> Result<(), String> {
        for hash in [&self.source_sha256, &self.settings_sha256] {
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            {
                return Err("Expected a lowercase SHA-256 identity".into());
            }
        }
        for value in [&self.producer, &self.producer_version, &self.kind] {
            if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
                return Err("Invalid observation provenance".into());
            }
        }
        if !self.data.is_object() || serde_json::to_vec(self).map_err(error)?.len() > MAX_DOCUMENT {
            return Err("Observation must be an object no larger than 64 KiB".into());
        }
        Ok(())
    }
    pub fn id(&self) -> Result<String, String> {
        self.validate()?;
        // Preserve the v1 struct-field layout and explicitly sort nested payload objects.
        // Cargo feature unification may enable serde_json/preserve_order in another module.
        let mut canonical = self.clone();
        canonical.data.sort_all_objects();
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&canonical).map_err(error)?)
        ))
    }
    /// A portable-preparation summary is evidence, not a raw DSP result or a verified beatgrid.
    /// Exact file SHA comes from the saved preparation; importing does not rehash the audio.
    pub fn from_preparation(p: &crate::Preparation) -> Result<Option<Self>, String> {
        p.validate()?;
        let (Some(hash), Some(a)) = (&p.source_sha256, &p.analysis) else {
            return Ok(None);
        };
        let record = Self {
            source_sha256: hash.to_ascii_lowercase(),
            producer: "amp-preparation-summary".into(),
            producer_version: "1".into(),
            settings_sha256: format!(
                "{:x}",
                Sha256::digest(b"summary-v1;no-paths;no-cues;no-stem-audio")
            ),
            kind: "prepared_summary".into(),
            data: serde_json::json!({
                "analysis_engine":a.engine,"duration_seconds":a.duration,"bpm":p.bpm,
                "key":p.key,"harmonic_key":p.harmonic_key,
                "tempo_confidence":a.tempo_confidence,"key_confidence":a.key_confidence,
                "reviewed":a.reviewed,"manual_override":p.manual_override,
                "identity_evidence":"saved_preparation","automatic_reuse_eligible":false
            }),
        };
        record.validate()?;
        Ok(Some(record))
    }
}
#[derive(Debug, Serialize)]
pub struct Stored {
    pub id: String,
    pub observation: Observation,
}
#[derive(Default, Debug, Serialize, PartialEq)]
pub struct Stats {
    pub observations: u64,
    pub audio_identities: u64,
}
pub struct Store {
    file: PathBuf,
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
impl Store {
    /// Caller supplies device-local server storage, never a shared music/NAS catalog.
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }
    fn open(&self, write: bool) -> Result<Option<Connection>, String> {
        match std::fs::symlink_metadata(&self.file) {
            Ok(m) if !m.is_file() || m.file_type().is_symlink() => {
                return Err("Knowledge database must be a regular file".into());
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !write {
                    return Ok(None);
                }
                std::fs::create_dir_all(self.file.parent().ok_or("Database directory missing")?)
                    .map_err(error)?;
            }
            Err(e) => return Err(error(e)),
        }
        let flags = if write {
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
        } else {
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let db = Connection::open_with_flags(&self.file, flags).map_err(error)?;
        db.busy_timeout(std::time::Duration::from_secs(2))
            .map_err(error)?;
        let app: i64 = db
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .map_err(error)?;
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(error)?;
        if app == 0 && version == 0 && write {
            let tables: i64 = db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |r| r.get(0)).map_err(error)?;
            if tables != 0 {
                return Err("Refusing to modify an unrelated database".into());
            }
            db.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE IF NOT EXISTS observations(id TEXT PRIMARY KEY,source_sha256 TEXT NOT NULL,document TEXT NOT NULL);
                CREATE INDEX IF NOT EXISTS observations_audio ON observations(source_sha256,id);
                PRAGMA application_id=1095585867; PRAGMA user_version=1; COMMIT;").map_err(error)?;
        } else if app != APPLICATION || version != VERSION {
            return Err(
                "Unsupported knowledge database identity or schema; retained unchanged".into(),
            );
        }
        if write {
            db.execute_batch("PRAGMA synchronous=FULL;")
                .map_err(error)?;
        }
        Ok(Some(db))
    }
    /// Transactional and idempotent. Different versions/results coexist; none replace a prior row.
    pub fn insert(&self, records: &[Observation]) -> Result<usize, String> {
        if records.len() > 100 {
            return Err("At most 100 observations per transaction".into());
        }
        let documents = records
            .iter()
            .map(|r| Ok((r.id()?, serde_json::to_string(r).map_err(error)?)))
            .collect::<Result<Vec<_>, String>>()?;
        if documents.is_empty() {
            return Ok(0);
        }
        let mut db = self.open(true)?.ok_or("Database unavailable")?;
        let transaction = db.transaction().map_err(error)?;
        let mut inserted = 0;
        for (record, (id, document)) in records.iter().zip(documents) {
            inserted += transaction.execute("INSERT INTO observations(id,source_sha256,document) VALUES(?1,?2,?3) ON CONFLICT(id) DO NOTHING",params![id,record.source_sha256,document]).map_err(error)?;
        }
        transaction.commit().map_err(error)?;
        Ok(inserted)
    }
    pub fn lookup(
        &self,
        source: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Stored>, String> {
        if source.len() != 64
            || !source
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err("Invalid source identity".into());
        }
        if !(1..=100).contains(&limit) {
            return Err("Page limit must be 1 to 100".into());
        }
        if after.is_some_and(|s| {
            s.len() != 64
                || !s
                    .bytes()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        }) {
            return Err("Invalid continuation ID".into());
        }
        let Some(db) = self.open(false)? else {
            return Ok(Vec::new());
        };
        let mut statement = db.prepare("SELECT id,document FROM observations WHERE source_sha256=?1 AND id>?2 ORDER BY id LIMIT ?3").map_err(error)?;
        let rows = statement
            .query_map(params![source, after.unwrap_or(""), limit as i64], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .map_err(error)?;
        rows.map(|row| {
            let (id, document) = row.map_err(error)?;
            if document.len() > MAX_DOCUMENT {
                return Err("Stored observation exceeds limit".into());
            }
            let observation: Observation = serde_json::from_str(&document).map_err(error)?;
            if observation.id()? != id || observation.source_sha256 != source {
                return Err("Knowledge record integrity check failed".into());
            }
            Ok(Stored { id, observation })
        })
        .collect()
    }
    pub fn stats(&self) -> Result<Stats, String> {
        let Some(db) = self.open(false)? else {
            return Ok(Stats::default());
        };
        db.query_row(
            "SELECT COUNT(*),COUNT(DISTINCT source_sha256) FROM observations",
            [],
            |r| {
                Ok(Stats {
                    observations: r.get(0)?,
                    audio_identities: r.get(1)?,
                })
            },
        )
        .optional()
        .map_err(error)
        .map(|s| s.unwrap_or_default())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn record() -> Observation {
        Observation {
            source_sha256: "a".repeat(64),
            producer: "amp-test".into(),
            producer_version: "1".into(),
            settings_sha256: "b".repeat(64),
            kind: "measured".into(),
            data: serde_json::json!({"bpm":120}),
        }
    }
    #[test]
    fn json_insertion_order_is_irrelevant_and_existing_v1_ids_remain_valid() {
        assert_eq!(
            record().id().unwrap(),
            "ade8bc8f4c7c988fb0d63a517d841d9cef2acb2f9d1b26ea440d5258d967cd4a"
        );
        let mut first = record();
        first.data =
            serde_json::from_str(r#"{"z":1,"a":{"z":2,"a":3},"list":[{"z":4,"a":5}]}"#).unwrap();
        let mut second = first.clone();
        second.data =
            serde_json::from_str(r#"{"list":[{"a":5,"z":4}],"a":{"a":3,"z":2},"z":1}"#).unwrap();
        assert_eq!(first.id().unwrap(), second.id().unwrap());
    }
    #[test]
    fn immutable_reuse_versions_and_pagination_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("knowledge.ampdb");
        let db = Store::new(file.clone());
        let a = record();
        let mut b = a.clone();
        b.producer_version = "2".into();
        let mut c = a.clone();
        c.settings_sha256 = "c".repeat(64);
        assert_eq!(db.insert(&[a.clone(), b, c]).unwrap(), 3);
        assert_eq!(db.insert(&[a]).unwrap(), 0);
        let db = Store::new(file);
        assert_eq!(
            db.stats().unwrap(),
            Stats {
                observations: 3,
                audio_identities: 1
            }
        );
        let first = db.lookup(&"a".repeat(64), None, 2).unwrap();
        let next = db.lookup(&"a".repeat(64), Some(&first[1].id), 2).unwrap();
        assert_eq!(next.len(), 1);
        assert!(db.lookup(&"d".repeat(64), None, 2).unwrap().is_empty());
    }
    #[test]
    fn rejects_invalid_batch_before_creating_database() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("knowledge.ampdb");
        let db = Store::new(file.clone());
        let mut bad = record();
        bad.source_sha256 = "different remix".into();
        assert!(db.insert(&[record(), bad]).is_err());
        assert!(!file.exists());
        let mut large = record();
        large.data = serde_json::json!({"text":"x".repeat(65536)});
        assert!(db.insert(&[large]).is_err());
        assert!(db.insert(&vec![record(); 101]).is_err());
        assert!(db.lookup(&"a".repeat(64), None, 101).is_err());
    }
    #[test]
    fn preserves_unknown_database_and_detects_modified_record() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("knowledge.ampdb");
        {
            let db = Connection::open(&file).unwrap();
            db.execute_batch(
                "CREATE TABLE owner_data(value TEXT); INSERT INTO owner_data VALUES('keep');",
            )
            .unwrap();
        }
        let db = Store::new(file.clone());
        assert!(db.insert(&[record()]).is_err());
        let c = Connection::open(&file).unwrap();
        assert_eq!(
            c.query_row("SELECT value FROM owner_data", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        drop(c);
        let other = dir.path().join("other.ampdb");
        let db = Store::new(other.clone());
        db.insert(&[record()]).unwrap();
        Connection::open(other)
            .unwrap()
            .execute(
                "UPDATE observations SET document=?1",
                [serde_json::to_string(&Observation {
                    data: serde_json::json!({"bpm":90}),
                    ..record()
                })
                .unwrap()],
            )
            .unwrap();
        assert!(db.lookup(&"a".repeat(64), None, 10).is_err());
    }
    #[test]
    fn summary_does_not_export_paths_cues_or_claim_automatic_reuse() {
        let mut p = crate::Preparation {
            source_sha256: Some("A".repeat(64)),
            bpm: Some(120.),
            ..Default::default()
        };
        assert!(Observation::from_preparation(&p).unwrap().is_none());
        p.analysis = Some(crate::preparation::AnalysisInfo {
            engine: "native-test".into(),
            duration: 120.,
            tempo_confidence: 0.8,
            key_confidence: 0.5,
            reviewed: false,
        });
        p.extensions
            .insert("private_path".into(), serde_json::json!("secret"));
        let row = Observation::from_preparation(&p).unwrap().unwrap();
        let encoded = serde_json::to_string(&row).unwrap();
        assert!(!encoded.contains("secret"));
        assert!(!encoded.contains("cue_in"));
        assert_eq!(row.source_sha256, "a".repeat(64));
        assert_eq!(row.data["automatic_reuse_eligible"], false);
    }
    #[test]
    fn missing_read_does_not_create_a_database() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("missing/knowledge.ampdb");
        let db = Store::new(file.clone());
        assert_eq!(db.stats().unwrap(), Stats::default());
        assert!(db.lookup(&"a".repeat(64), None, 10).unwrap().is_empty());
        assert!(!file.exists());
    }
}
