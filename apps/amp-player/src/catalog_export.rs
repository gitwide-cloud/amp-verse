//! Select music and export a portable catalog without exposing storage internals.
#[path = "catalog_export/archive.rs"]
mod archive;
use amp_core::{
    Library, Playlist,
    show::{Settings, SmartPlaylist},
};
use eframe::egui;
use std::{
    collections::{BTreeSet, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc,
    },
    time::Duration,
};
struct Task {
    cancel: Arc<AtomicBool>,
    progress: mpsc::Receiver<String>,
    result: mpsc::Receiver<Result<PathBuf, String>>,
}
impl Drop for Task {
    fn drop(&mut self) {
        self.cancel.store(true, Relaxed);
    }
}
pub struct Panel {
    open: bool,
    root: PathBuf,
    all: bool,
    folders: BTreeSet<PathBuf>,
    lists: BTreeSet<PathBuf>,
    smart: BTreeSet<String>,
    filter: SmartPlaylist,
    picker: crate::genre_picker::Picker,
    search: String,
    selected: Vec<PathBuf>,
    selection_key: Option<Vec<u8>>,
    folder_cache: Option<(u64, Vec<PathBuf>)>,
    destination: Option<PathBuf>,
    rar: Option<PathBuf>,
    task: Option<Task>,
    status: String,
}
impl Default for Panel {
    fn default() -> Self {
        Self {
            open: false,
            root: PathBuf::new(),
            all: true,
            folders: Default::default(),
            lists: Default::default(),
            smart: Default::default(),
            filter: Settings::default().playlists.remove(0),
            picker: Default::default(),
            search: String::new(),
            selected: vec![],
            selection_key: None,
            folder_cache: None,
            destination: None,
            rar: None,
            task: None,
            status: String::new(),
        }
    }
}
fn selected(
    library: &Library,
    all: bool,
    folders: &BTreeSet<PathBuf>,
    playlist_paths: &HashSet<PathBuf>,
    smart: &[&SmartPlaylist],
    filter: &SmartPlaylist,
    query: &str,
) -> Vec<PathBuf> {
    let query = query.trim().to_lowercase();
    library
        .tracks
        .iter()
        .filter(|track| {
            (all || folders.iter().any(|p| track.path.starts_with(p))
                || playlist_paths.contains(&track.path)
                || smart.iter().any(|p| amp_core::show::eligible(p, track)))
                && amp_core::show::eligible(filter, track)
                && (query.is_empty()
                    || track.title.to_lowercase().contains(&query)
                    || track.path.to_string_lossy().to_lowercase().contains(&query)
                    || ["artist", "album", "genre"].iter().any(|key| {
                        crate::track_columns::cell(track, key)
                            .to_lowercase()
                            .contains(&query)
                    }))
        })
        .map(|t| t.path.clone())
        .collect()
}
impl Panel {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        root: &Path,
        library: Option<&Library>,
        settings: &Settings,
        playlists: &[(PathBuf, Playlist)],
        generation: u64,
    ) {
        if self.root != root && self.task.is_none() {
            self.root = root.to_owned();
            self.selected.clear();
            self.selection_key = None;
            self.folder_cache = None;
            self.folders.clear();
            self.lists.clear();
            self.smart.clear();
            self.all = true;
        }
        if let Some(task) = &self.task {
            while let Ok(status) = task.progress.try_recv() {
                self.status = status;
            }
            match task.result.try_recv() {
                Ok(result) => {
                    self.status = result.map_or_else(
                        |e| e,
                        |path| {
                            format!(
                                "Export complete: {}. All parts are 100 MB or smaller.",
                                path.display()
                            )
                        },
                    );
                    self.task = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status =
                        "Export stopped before completion. Original music is unchanged.".into();
                    self.task = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ui.ctx().request_repaint_after(Duration::from_millis(100));
                }
            }
        }
        ui.heading("Portable catalog");
        ui.label("Take selected music, playlists and saved analysis to another computer.");
        if ui
            .add_enabled(
                library.is_some(),
                egui::Button::new(if self.task.is_some() {
                    "Export progress…"
                } else {
                    "Export catalog…"
                }),
            )
            .clicked()
        {
            self.rar = archive::rar_path();
            self.open = true;
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Export catalog").open(&mut open).default_width(640.).resizable(true).vscroll(true).show(ui.ctx(),|ui|{
            ui.label("Choose the music to include. Your originals stay in place.");
            ui.weak("Creates RAR parts, each up to 100 MB, with a portable AMP Verse index. Playlists keep the exported selection.");
            if let Some(task)=&self.task{ui.label(&self.status);ui.spinner();if ui.button("Cancel export").clicked(){task.cancel.store(true,Relaxed);self.status="Canceling export…".into();}return;}
            let Some(library)=library else{ui.label("Connect a music library first.");return;};
            ui.checkbox(&mut self.all,"Entire music library");
            ui.collapsing("Folders",|ui|{
                if self.folder_cache.as_ref().is_none_or(|(seen,_)|*seen!=generation){
                    let folders:BTreeSet<_>=library.tracks.iter().flat_map(|track|track.path.parent().into_iter().flat_map(Path::ancestors).filter(|folder|!folder.as_os_str().is_empty()).map(Path::to_path_buf)).collect();
                    self.folder_cache=Some((generation,folders.into_iter().collect()));
                }
                let folders=&self.folder_cache.as_ref().unwrap().1;
                egui::ScrollArea::vertical().id_salt("export-folders").max_height(160.).show(ui,|ui|{
                    for folder in folders{let mut checked=self.folders.contains(folder);if ui.checkbox(&mut checked,if folder.as_os_str().is_empty(){"Music folder".to_owned()}else{folder.components().map(|part|amp_core::music_sources::display_name(&part.as_os_str().to_string_lossy())).collect::<Vec<_>>().join(" / ")}).changed(){if checked{self.folders.insert(folder.clone());self.all=false;}else{self.folders.remove(folder);}}}
                });
            });
            ui.collapsing("Playlists and Smart Playlists",|ui|{
                for(path,list)in playlists{let mut checked=self.lists.contains(path);if ui.checkbox(&mut checked,&list.name).changed(){if checked{self.lists.insert(path.clone());self.all=false;}else{self.lists.remove(path);}}}
                for list in &settings.playlists{let mut checked=self.smart.contains(&list.id);if ui.checkbox(&mut checked,format!("{} · Smart",list.name)).changed(){if checked{self.smart.insert(list.id.clone());self.all=false;}else{self.smart.remove(&list.id);}}}
            });
            ui.label("Categories");self.picker.show(ui,&mut self.filter.genres,Some(library));
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search title, artist, album or genre…").desired_width(ui.available_width()));
            ui.collapsing("More filters",|ui|crate::smart_filter_ui::show(ui,&mut self.filter,Some(library)));
            let key=serde_json::to_vec(&(generation,self.all,&self.folders,&self.lists,&self.smart,&self.filter.genres,&self.filter.filters,&self.search)).unwrap_or_default();
            if self.selection_key.as_ref()!=Some(&key){
                let list_paths:HashSet<_>=playlists.iter().filter(|(p,_)|self.lists.contains(p)).flat_map(|(_,p)|p.tracks.iter().cloned()).collect();
                let profiles:Vec<_>=settings.playlists.iter().filter(|p|self.smart.contains(&p.id)).collect();
                self.selected=selected(library,self.all,&self.folders,&list_paths,&profiles,&self.filter,&self.search);self.selection_key=Some(key);
            }
            ui.strong(format!("{} songs selected",self.selected.len()));
            ui.horizontal(|ui|{if ui.button("Choose destination…").clicked(){self.destination=rfd::FileDialog::new().set_title("Export catalog destination").pick_folder();}if let Some(path)=&self.destination{ui.label(path.file_name().unwrap_or(path.as_os_str()).to_string_lossy());}});
            if self.rar.is_none(){ui.label("RAR is not installed. Install WinRAR on Windows or RAR on Linux to create these archive parts.");ui.hyperlink_to("Get WinRAR / RAR","https://www.rarlab.com/download.htm");}
            if ui.add_enabled(!self.selected.is_empty()&&self.destination.is_some()&&self.rar.is_some(),egui::Button::new("Export catalog")).clicked(){
                let root=root.to_owned();let chosen:HashSet<_>=self.selected.iter().cloned().collect();
                let lists=playlists.iter().filter(|(path,_)|self.all||self.lists.contains(path)).map(|(_,list)|list.clone()).collect();
                let smart=settings.playlists.iter().filter(|list|self.all||self.smart.contains(&list.id)).map(|list|{
                    let mut result=list.clone();
                    result.allowed=library.tracks.iter().filter(|track|chosen.contains(&track.path)&&amp_core::show::eligible(list,track)).map(|track|track.path.to_string_lossy().replace('\\',"/")).collect();result
                }).collect();
                let selection=archive::Selection{paths:self.selected.clone(),playlists:lists,smart};let destination=self.destination.clone().unwrap();let rar=self.rar.clone().unwrap();let cancel=Arc::new(AtomicBool::new(false));let stop=cancel.clone();let(ptx,prx)=mpsc::sync_channel(8);let(tx,rx)=mpsc::channel();
                std::thread::spawn(move||{let result=archive::export(&root,selection,&destination,&rar,&stop,&ptx);let _=tx.send(result);});
                self.task=Some(Task{cancel,progress:prx,result:rx});self.status="Preparing your export…".into();
            }
        });
        self.open = open;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combined_sources_are_filtered_and_duplicate_songs_appear_once() {
        let song = |path: &str, title: &str| amp_core::Track {
            path: path.into(),
            title: title.into(),
            preparation: Default::default(),
        };
        let library = Library {
            tracks: vec![
                song("Rock/a.mp3", "A"),
                song("Dance/b.mp3", "B"),
                song("Jazz/c.mp3", "C"),
            ],
            tree: amp_core::Folder {
                name: "music".into(),
                folders: vec![],
                tracks: vec![],
            },
        };
        let filter = Settings::default().playlists.remove(0);
        let folders = BTreeSet::from([PathBuf::from("Rock")]);
        let lists = HashSet::from([PathBuf::from("Rock/a.mp3"), PathBuf::from("Dance/b.mp3")]);
        assert_eq!(
            selected(&library, false, &folders, &lists, &[], &filter, "").len(),
            2
        );
        assert_eq!(
            selected(&library, false, &folders, &lists, &[], &filter, "B"),
            vec![PathBuf::from("Dance/b.mp3")]
        );
        assert!(
            selected(
                &library,
                false,
                &Default::default(),
                &Default::default(),
                &[],
                &filter,
                ""
            )
            .is_empty()
        );
    }
}
