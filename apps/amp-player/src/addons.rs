//! Built-in component catalog and explicit external-editor connection. No remote code loading.
use crate::skin::{self, GOLD, Module};
use eframe::egui::{self, RichText};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

struct Entry {
    name: &'static str,
    kind: &'static str,
    summary: &'static str,
    features: &'static [&'static str],
}
const ENTRIES: [Entry; 6] = [
    Entry {
        name: "AMP VERSE",
        kind: "Included",
        summary: "Compact base player and module launcher.",
        features: &[
            "Local playback and network library access",
            "Top-bar module dropdown",
            "Independent Library and DJ windows",
        ],
    },
    Entry {
        name: "AMP Library",
        kind: "Included",
        summary: "Music folders, preparation data and playlists.",
        features: &[
            "Folder tree and file search",
            "Prepared cue IN/OUT and BPM metadata",
            "Save/load playlists; add and remove queued tracks",
        ],
    },
    Entry {
        name: "AMP DJ",
        kind: "Included",
        summary: "Two-deck prepared playback prototype.",
        features: &[
            "Two independent local playback decks",
            "Prepared cue IN/OUT",
            "Prepared Auto DJ and automatic crossfade",
            "Nine-pad sound-effect sampler",
        ],
    },
    Entry {
        name: "AMP Cast",
        kind: "Companion",
        summary: "Native Cast dashboard and independent headless server.",
        features: &[
            "Native dashboard: connect, queue, listeners, start/stop",
            "One MP3 encoder shared by listeners",
            "Runs independently from the base player",
            "Server requires separate startup and FFmpeg",
        ],
    },
    Entry {
        name: "AMP Lab",
        kind: "Included",
        summary: "Integrated MP3 Lab editor foundation and mix arrangement.",
        features: &[
            "Waveform trim handles, fades, gain and normalization",
            "Original/processed preview and selection MP3 export",
            "Cue-aligned arrangement with combined or individual export",
            "Multitrack fades, beat sync and stems remain unfinished",
        ],
    },
    Entry {
        name: "AMP Analysis Engine",
        kind: "Optional",
        summary: "Installable BPM, key, energy, waveform and cue analysis.",
        features: &[
            "Separate executable; absent from base playback",
            "Shared-library leases and protected manual metadata",
            "Runs only when analysis is requested",
            "Requires FFmpeg; install a build matching your OS",
        ],
    },
];

pub struct Manager {
    pub selected: usize,
    download: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
    search: String,
    filter: usize,
    lab: Option<PathBuf>,
    status: String,
    icon: Option<egui::TextureHandle>,
}
impl Default for Manager {
    fn default() -> Self {
        Self {
            selected: 5,
            download: None,
            search: String::new(),
            filter: 0,
            lab: std::env::var_os("AMP_LAB_EXE")
                .map(PathBuf::from)
                .filter(|p| p.is_file()),
            status: "Downloads are manual. No automatic installation or update checks.".into(),
            icon: None,
        }
    }
}
impl Manager {
    fn launch_lab(&mut self) {
        let Some(path) = &self.lab else { return };
        let mut command = Command::new(path);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        self.status=match command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
            Ok(_)=>"Opened the external editor. Open your audio file in that app; file handoff is not implemented yet.".into(),
            Err(e)=>format!("Could not open editor: {e}"),
        };
    }
    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<Module> {
        if let Some(receiver) = &self.download {
            match receiver.try_recv() {
                Ok(result) => {
                    self.status = match result {
                        Ok(()) => {
                            "Analysis add-on downloaded and installed. Ready to analyze.".into()
                        }
                        Err(e) => format!("Download failed: {e}"),
                    };
                    self.download = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status = "Download stopped unexpectedly. Please retry.".into();
                    self.download = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
        }
        if self.icon.is_none() {
            let icon = crate::app_icon();
            self.icon = Some(ui.ctx().load_texture(
                "addon-brand",
                egui::ColorImage::from_rgba_unmultiplied(
                    [icon.width as usize, icon.height as usize],
                    &icon.rgba,
                ),
                egui::TextureOptions::LINEAR,
            ));
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("GitHub / gitwide-cloud").color(skin::accent(ui)));
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search components...")
                    .desired_width(245.),
            );
            egui::ComboBox::from_id_salt("addon-filter")
                .selected_text(["All components", "Included", "Companions & optional"][self.filter])
                .show_ui(ui, |ui| {
                    for (i, label) in ["All components", "Included", "Companions & optional"]
                        .iter()
                        .enumerate()
                    {
                        ui.selectable_value(&mut self.filter, i, *label);
                    }
                });
            ui.hyperlink_to(
                "Releases / updates",
                "https://github.com/gitwide-cloud/amp-verse/releases",
            );
        });
        let height = (ui.available_height() - 125.).max(330.);
        let mut action = None;
        ui.horizontal_top(|ui| {
            skin::panel(ui,false,|ui| {
                ui.set_width(130.); ui.set_min_height(height);
                skin::label(ui,"Add-ons"); ui.separator();
                for (i,label) in ["All components","Included","Optional"].iter().enumerate() { ui.selectable_value(&mut self.filter,i,*label); }
                ui.separator(); ui.weak("GitHub Releases\nManual downloads\nOffline catalog");
            });
            skin::panel(ui,false,|ui| {
                ui.set_width(330.); ui.set_min_height(height);
                skin::label(ui,"Available components"); ui.separator();
                egui::ScrollArea::vertical().id_salt("addon-list").max_height(height-70.).show(ui,|ui| {
                    let search=self.search.to_lowercase(); let mut visible=0;
                    for (i,entry) in ENTRIES.iter().enumerate() {
                        if self.filter==1 && entry.kind!="Included" || self.filter==2 && entry.kind=="Included" || !format!("{} {}",entry.name,entry.summary).to_lowercase().contains(&search) {continue;}
                        visible+=1;
                        let label=if entry.kind=="Included" {format!("{}  /  v{}",entry.name,env!("CARGO_PKG_VERSION"))}else{format!("{}  /  {}",entry.name,entry.kind)};
                        if ui.selectable_label(self.selected==i,RichText::new(label).strong().size(17.)).clicked() {self.selected=i;}
                        ui.label(entry.summary); ui.add_space(5.); ui.separator();
                    }
                    if visible==0 { ui.weak("No matching components."); }
                });
            });
            skin::panel(ui,false,|ui| {
                ui.set_min_width(ui.available_width()); ui.set_min_height(height);
                let entry=&ENTRIES[self.selected];
                ui.horizontal(|ui| {
                    if let Some(icon)=&self.icon { ui.image((icon.id(),egui::vec2(58.,58.))); }
                    ui.vertical(|ui| {ui.label(RichText::new(entry.name).strong().size(25.).color(skin::accent(ui)));ui.label(entry.kind);});
                });
                ui.separator(); ui.label(entry.summary); ui.add_space(7.);
                for feature in entry.features { ui.label(format!("- {feature}")); }
                ui.add_space(12.);
                match self.selected {
                    0..=2=>{
                        ui.label(RichText::new(format!("Included in this build / {}",env!("CARGO_PKG_VERSION"))).color(skin::signal(ui)));
                        if skin::button(ui,"OPEN",GOLD).clicked() { action=Some(match self.selected {0=>Module::Verse,1=>Module::Library,_=>Module::Dj}); }
                    },
                    3=>{
                        let companion=std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join(if cfg!(windows){"amp-cast.exe"}else{"amp-cast"}))).is_some_and(|p|p.is_file());
                        ui.label(if companion{"Companion executable present beside AMP Verse"}else{"Companion executable not found beside AMP Verse"});
                        if skin::button(ui,"OPEN NATIVE CAST",GOLD).clicked() { action=Some(Module::Cast); }
                        ui.hyperlink_to("Open browser listener","http://127.0.0.1:8787");
                        ui.hyperlink_to("Download AMP Verse builds","https://github.com/gitwide-cloud/amp-verse/actions");
                        ui.weak("The server must already be running. Presence does not imply an active station.");
                    },
                    4=>{
                        if skin::button(ui,"OPEN AMP LAB",GOLD).clicked() { action=Some(Module::LabSetup); }
                        ui.weak("Integrated waveform editor included. External editor connection is optional.");
                        ui.hyperlink_to("Download MP3 Lab releases","https://github.com/gitwide-cloud/mp3-lab/releases");
                        if ui.button("Connect installed editor...").clicked() && let Some(path)=rfd::FileDialog::new().pick_file() { self.lab=Some(path); self.status="Editor selected for this session. Click Open editor to launch it.".into(); }
                        if let Some(path)=&self.lab {ui.add(egui::Label::new(path.display().to_string()).truncate());}
                        if ui.add_enabled(self.lab.as_ref().is_some_and(|p|p.is_file()),egui::Button::new("Open editor")).clicked() {self.launch_lab();}
                        ui.weak("The connected executable stays separate. Use AMP_LAB_EXE to configure it at startup; picker connections last for this session.");
                    },
                    _=>{
                        let installed=crate::engine::path();
                        ui.label(if installed.is_some(){"Analysis add-on installed"}else{"Analysis add-on not installed"});
                        if ui.add_enabled(self.download.is_none(), egui::Button::new(if self.download.is_some() {"Downloading..."} else {"Download & install analysis engine"})).clicked() {
                            let (sender, receiver) = std::sync::mpsc::channel();
                            self.download = Some(receiver);
                            self.status = "Downloading the engine from GitHub Releases...".into();
                            std::thread::spawn(move || { let _ = sender.send(crate::engine_download::download()); });
                        }
                        ui.hyperlink_to("Open releases in browser", "https://github.com/gitwide-cloud/amp-verse/releases");
                        if ui.button("Install downloaded engine...").clicked() && let Some(source)=rfd::FileDialog::new().pick_file() {
                            self.status=match std::env::current_exe().map_err(|e|e.to_string()).and_then(|exe| {
                                let directory=exe.parent().ok_or("Missing application directory")?.join("addons");
                                std::fs::create_dir_all(&directory).map_err(|e|e.to_string())?;
                                let target=directory.join(if cfg!(windows){"amp-engine.exe"}else{"amp-engine"});
                                if source.canonicalize().ok()!=target.canonicalize().ok() {
                                    std::fs::copy(source,&target).map_err(|e|e.to_string())?;
                                }
                                #[cfg(unix)] {use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(directory.join("amp-engine"),std::fs::Permissions::from_mode(0o755)).map_err(|e|e.to_string())?;}
                                Ok(())
                            }) {Ok(())=>"Analysis add-on installed. Library analysis is available.".into(),Err(e)=>format!("Could not install analysis add-on: {e}")};
                        }
                        if let Some(path)=installed {
                            ui.add(egui::Label::new(path.display().to_string()).truncate());
                            let local=std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join("addons").join(if cfg!(windows){"amp-engine.exe"}else{"amp-engine"}))).is_some_and(|p|p==path);
                            if local && ui.button("Remove analysis add-on").clicked() {
                                self.status=match std::fs::remove_file(path) {Ok(())=>"Analysis add-on removed. Prepared playback and manual editing remain available.".into(),Err(e)=>format!("Could not remove add-on: {e}")};
                            }
                        }
                        ui.weak("Downloads start only when requested. SHA-256 is checked before installation. Analysis starts separately.");
                    },
                }
            });
        });
        skin::panel(ui, false, |ui| {
            ui.set_min_width(ui.available_width());
            skin::label(ui, "Included in AMP Verse");
            ui.horizontal(|ui| {
                for name in ["AMP VERSE", "AMP Library", "AMP DJ"] {
                    ui.label(
                        RichText::new(format!("{name} / {}", env!("CARGO_PKG_VERSION")))
                            .color(skin::signal(ui)),
                    );
                    ui.separator();
                }
            });
        });
        ui.label(RichText::new(&self.status).color(skin::accent(ui)));
        action
    }
}
