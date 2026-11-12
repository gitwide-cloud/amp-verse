//! Derived sampler suggestions; never moves audio or changes song preparation.
use amp_core::Track;
use eframe::egui;

pub fn candidate(track: &Track) -> Option<(f64, bool)> {
    let seconds = track.preparation.analysis.as_ref()?.duration;
    if !seconds.is_finite() || !(0.01..=30.).contains(&seconds) {
        return None;
    }
    let names = format!(
        "{} {} {}",
        track.path.display(),
        track.title,
        track
            .preparation
            .extensions
            .get("genre")
            .map_or(String::new(), |v| v.to_string())
    );
    let words = names
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>();
    let words = format!(
        " {} ",
        words.split_whitespace().collect::<Vec<_>>().join(" ")
    );
    let effect = [
        "sfx",
        "fx",
        "sound effect",
        "sound effects",
        "samples",
        "sampler",
        "one shot",
        "oneshot",
        "airhorn",
        "air horn",
        "siren",
        "riser",
        "jingle",
        "dj drop",
        "dj drops",
        "scratch",
    ]
    .iter()
    .any(|word| words.contains(&format!(" {word} ")));
    Some((seconds, effect))
}
#[derive(Default)]
pub struct Browser {
    pub open: bool,
    pub side: usize,
    pub pad: usize,
    search: String,
    short_clips: bool,
}
impl Browser {
    pub fn show(&mut self, ctx: &egui::Context, tracks: &[Track]) -> Option<usize> {
        if !self.open {
            return None;
        }
        let mut selected = None;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("sample-library"),
            egui::ViewportBuilder::default()
                .with_title("AMP Verse · Sound effects")
                .with_inner_size([640., 480.])
                .with_min_inner_size([340., 260.])
                .with_resizable(true),
            |ui, _| {
                if ui.ctx().input(|i| i.viewport().close_requested()) {
                    self.open = false;
                }
                selected = self.content(ui, tracks);
            },
        );
        selected
    }
    fn content(&mut self, ui: &mut egui::Ui, tracks: &[Track]) -> Option<usize> {
        let mut selected = None;
        ui.label("Suggestions from analyzed files up to 30 seconds. Original files stay in their folders.");
        ui.horizontal_wrapped(|ui| {
            ui.label("Sampler");
            ui.selectable_value(&mut self.side, 0, "A");
            ui.selectable_value(&mut self.side, 1, "B");
            ui.label("Pad");
            let mut pad = self.pad + 1;
            ui.add(egui::DragValue::new(&mut pad).range(1..=9));
            self.pad = pad - 1;
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.search)
                .hint_text("Search effects or folders…")
                .desired_width(f32::INFINITY),
        );
        ui.checkbox(&mut self.short_clips, "Include all short clips");
        let query = self.search.to_lowercase();
        let found: Vec<_> = tracks
            .iter()
            .enumerate()
            .filter_map(|(index, track)| {
                let (seconds, effect) = candidate(track)?;
                ((effect || self.short_clips)
                    && format!("{} {}", track.title, track.path.display())
                        .to_lowercase()
                        .contains(&query))
                .then_some((index, track, seconds, effect))
            })
            .collect();
        ui.weak(format!(
            "{} files · choose one to load into {} pad {}",
            found.len(),
            if self.side == 0 { "A" } else { "B" },
            self.pad + 1
        ));
        ui.separator();
        egui::ScrollArea::vertical().max_height(ui.available_height()).show(ui, |ui| {
                    if found.is_empty() { ui.label("No matches yet. Analyze your sample folders, or enable all short clips. You can also load a file directly from a pad."); }
                    for (index,track,seconds,effect) in found {
                        ui.horizontal(|ui| {
                            let load = ui.button("Load");
                            ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(("sample-library-load",index)),load.rect));
                            if load.clicked() { selected=Some(index); }
                            ui.label(format!("{seconds:.1}s"));
                            ui.add(egui::Label::new(&track.title).truncate()).on_hover_text(track.path.display().to_string());
                            if !effect { ui.weak("Short clip"); }
                        });
                    }
                });
        selected
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn track(path: &str, seconds: f64) -> Track {
        Track {
            path: path.into(),
            title: "Clip".into(),
            preparation: amp_core::Preparation {
                analysis: Some(amp_core::preparation::AnalysisInfo {
                    duration: seconds,
                    engine: "fixture".into(),
                    tempo_confidence: 0.,
                    key_confidence: 0.,
                    reviewed: false,
                }),
                ..Default::default()
            },
        }
    }
    #[test]
    fn effects_use_word_hints_and_duration_without_modifying_tracks() {
        let t = track("DJ Sound Effects/air-horn.wav", 2.);
        let before = t.preparation.clone();
        assert_eq!(candidate(&t), Some((2., true)));
        assert_eq!(t.preparation, before);
        assert_eq!(
            candidate(&track("Music/short-intro.mp3", 12.)),
            Some((12., false))
        );
        assert_eq!(candidate(&track("SFX/long-song.mp3", 31.)), None);
        assert_eq!(candidate(&track("FX/invalid.wav", f64::NAN)), None);
        assert_eq!(
            candidate(&track("artistfx/song.wav", 3.)),
            Some((3., false))
        );
    }
    #[test]
    fn missing_analysis_is_not_a_confirmed_sample() {
        let mut t = track("Samples/horn.wav", 3.);
        t.preparation.analysis = None;
        assert_eq!(candidate(&t), None);
    }
    #[test]
    fn search_and_click_choose_the_right_clip_in_a_small_window() {
        let ctx = egui::Context::default();
        let tracks = vec![
            track("SFX/horn.wav", 2.),
            track("SFX/siren.wav", 4.),
            track("Music/short.mp3", 8.),
        ];
        let mut browser = Browser {
            open: true,
            side: 1,
            pad: 7,
            search: "siren".into(),
            ..Default::default()
        };
        let mut frame = |events| {
            let mut chosen = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(360., 320.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    chosen = browser.content(ui, &tracks);
                },
            );
            output.textures_delta.clear();
            chosen
        };
        frame(vec![]);
        let rect = ctx.data(|d| {
            d.get_temp::<egui::Rect>(egui::Id::unique(("sample-library-load", 1)))
                .unwrap()
        });
        assert!(rect.min.y >= 0. && rect.max.y < 320. && rect.max.x < 360.);
        let pos = rect.center();
        frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        assert_eq!(
            frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default()
            }]),
            Some(1)
        );
        assert_eq!((browser.side, browser.pad), (1, 7));
    }
}
