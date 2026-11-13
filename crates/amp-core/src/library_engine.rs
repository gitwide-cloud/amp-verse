//! Shared Library Engine: storage, work coordination, preparation and organization.
#[cfg(feature = "analysis")]
pub fn analyze_job(
    root: &std::path::Path,
    relative: &std::path::Path,
    force: bool,
) -> Result<crate::coordination::AnalysisResult, String> {
    let (owner, path) = crate::music_sources::owner(root, relative).map_err(|e| e.to_string())?;
    catalog_db::ensure(&owner)?;
    crate::coordination::analyze_shared(&owner, &path, force)
}
#[cfg(feature = "analysis")]
#[path = "analysis.rs"]
pub mod analysis;
#[path = "analysis_queue.rs"]
pub mod analysis_queue;
#[path = "artwork.rs"]
pub mod artwork;
#[path = "catalog_db.rs"]
pub mod catalog_db;
#[path = "catalog_shards.rs"]
pub(crate) mod catalog_shards;
#[path = "consolidate.rs"]
pub mod consolidate;
#[path = "genres.rs"]
pub mod genres;
#[path = "media_kind.rs"]
pub mod media_kind;
#[path = "music_sources.rs"]
pub mod music_sources;
#[path = "offline.rs"]
pub mod offline;
#[path = "preparation.rs"]
pub mod preparation;
#[path = "review.rs"]
pub mod review;
#[path = "storage.rs"]
pub mod storage;
