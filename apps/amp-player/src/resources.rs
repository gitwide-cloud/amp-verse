//! Scoped resource discovery for Companion. Legacy full catalog remains compatible.
use super::*;
use serde_json::{Value, json};
pub fn stable_song_id(t: &amp_core::Track) -> String {
    t.preparation
        .source_sha256
        .as_ref()
        .map(|hash| format!("audio_{hash}"))
        .unwrap_or_else(|| amp_core::show::song_id(t))
}
#[derive(Default, serde::Deserialize)]
pub struct Query {
    #[serde(default)]
    pub song: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub playlist: String,
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub limit: Option<usize>,
}
pub fn page(catalog: &Value, query: &Query) -> Result<Value, String> {
    let empty = Vec::new();
    let collections = catalog["resource_playlists"].as_array().unwrap_or(&empty);
    let list = if query.scope.is_empty() || query.scope == "playlist" {
        Some(
            collections
                .iter()
                .find(|p| p["id"].as_str() == Some(&query.playlist) && p["available"] == true)
                .ok_or("Select a playlist marked Available in Companion")?,
        )
    } else if query.scope == "folder" || query.scope == "all" {
        None
    } else {
        return Err("Unknown discovery scope".into());
    };
    if query.scope == "folder"
        && (query.folder.is_empty()
            || query.folder.contains("..")
            || std::path::Path::new(&query.folder).is_absolute())
    {
        return Err("Choose a relative indexed folder".into());
    }
    let needle = query.q.to_lowercase();
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.min(1_000_000);
    let mut total = 0;
    let mut songs = Vec::new();
    for song in catalog["songs"].as_array().unwrap_or(&empty) {
        let included = if let Some(list) = list {
            list["tracks"]
                .as_array()
                .is_some_and(|ids| ids.contains(&song["id"]))
        } else if query.scope == "folder" {
            amp_core::folder_contains(
                std::path::Path::new(song["path"].as_str().unwrap_or("")),
                std::path::Path::new(&query.folder),
            )
        } else {
            true
        };
        if !query.song.is_empty()
            && song["resource_id"].as_str() != Some(&query.song)
            && song["id"].as_str() != Some(&query.song)
        {
            continue;
        }
        if !included
            || (!needle.is_empty()
                && !format!(
                    "{} {}",
                    song["title"].as_str().unwrap_or(""),
                    song["artist"].as_str().unwrap_or("")
                )
                .to_lowercase()
                .contains(&needle))
        {
            continue;
        }
        if total >= offset && songs.len() < limit {
            let mut value = song.clone();
            value["id"] = song["resource_id"].clone();
            songs.push(value);
        }
        total += 1;
    }
    Ok(
        json!({"songs":songs,"total":total,"offset":offset,"limit":limit,"next_offset":if offset+songs.len()<total{Some(offset+songs.len())}else{None}}),
    )
}
impl Player {
    pub(super) fn companion_playlists(&self) -> Vec<Value> {
        let tracks = self
            .library
            .as_ref()
            .map(|l| l.tracks.as_slice())
            .unwrap_or(&[]);
        let mut result:Vec<_>=self.run.settings.playlists.iter().map(|p|json!({"id":p.id,"name":p.name,"kind":"smart","available":p.companion_available,"tracks":tracks.iter().filter(|t|amp_core::show::eligible(p,t)).map(amp_core::show::song_id).collect::<Vec<_>>()})).collect();
        result.extend(self.dj_browser.playlists.iter().map(|(path,p)|json!({"id":performance::regular_playlist_id(path),"name":p.name,"kind":"playlist","available":p.companion_available,"tracks":tracks.iter().filter(|t|p.tracks.contains(&t.path)).map(amp_core::show::song_id).collect::<Vec<_>>()})));
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_is_default_and_pages_never_include_other_playlists() {
        let c = json!({"songs":[{"id":"one","resource_id":"stable-one","title":"First","path":"folder/sub/a.mp3"},{"id":"two","resource_id":"stable-two","title":"Second","path":"other/b.mp3"}],"resource_playlists":[{"id":"p","available":true,"tracks":["one"]},{"id":"hidden","available":false,"tracks":["two"]}]});
        assert!(page(&c, &Query::default()).is_err());
        let q = Query {
            playlist: "p".into(),
            limit: Some(1),
            ..Default::default()
        };
        let found = page(&c, &q).unwrap();
        assert_eq!(found["total"], 1);
        assert_eq!(found["songs"][0]["id"], "stable-one");
        assert!(
            page(
                &c,
                &Query {
                    playlist: "hidden".into(),
                    ..Default::default()
                }
            )
            .is_err()
        );
        let q = Query {
            scope: "folder".into(),
            folder: "folder".into(),
            ..Default::default()
        };
        assert_eq!(page(&c, &q).unwrap()["total"], 1);
    }
}
