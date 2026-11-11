use crate::{Player, mix_schedule::Timing};
use eframe::egui;
use std::sync::atomic::Ordering::Relaxed;
impl Player {
    pub(super) fn deck_sections(&mut self, ui: &mut egui::Ui, deck: usize) {
        let Some(track) = self.loaded_track(deck).cloned() else {
            return;
        };
        let Some(profile) = amp_core::energy_profile::read(&track.preparation) else {
            return;
        };
        let position = self.audio.as_ref().map_or(0., |a| a.source_position(deck));
        let mut choice = None;
        ui.horizontal_wrapped(|ui| {
            let old = self.run.transitions.timing;
            egui::ComboBox::from_id_salt(("section-timing", deck))
                .selected_text(self.run.transitions.timing.label())
                .show_ui(ui, |ui| {
                    for timing in [
                        Timing::Immediate,
                        Timing::Beat,
                        Timing::Bar,
                        Timing::Phrase8,
                        Timing::Phrase16,
                        Timing::Phrase32,
                    ] {
                        ui.selectable_value(
                            &mut self.run.transitions.timing,
                            timing,
                            timing.label(),
                        );
                    }
                });
            if old != self.run.transitions.timing
                && let Err(e) = crate::transition_styles::save(self.run.transitions)
            {
                self.status = e;
            }
            if self.run.transitions.timing.phrase_beats().is_some()
                && !track
                    .preparation
                    .extensions
                    .contains_key("mix_phrase_anchor")
            {
                ui.weak("Estimated phrase grid — review in Library");
            }
            if self.run.transitions.timing != Timing::Immediate
                && (track.preparation.bpm.is_none()
                    || (track.preparation.beat_offset.is_none()
                        && !track.preparation.extensions.contains_key("mix_bar_anchor")))
            {
                ui.weak("No usable grid: mix when prepared");
            }
            if self.run.transitions.timing == Timing::Bar
                && !track.preparation.extensions.contains_key("mix_bar_anchor")
            {
                ui.weak("No reviewed bar anchor: using beats");
            }
            if let Some(audio) = &mut self.audio {
                match audio.controls.section_state[deck].load(Relaxed) {
                    1 => {
                        let target = ui.ctx().data(|data| {
                            data.get_temp::<f64>(egui::Id::unique(("section-target", deck)))
                        });
                        let name = profile
                            .sections
                            .iter()
                            .find(|s| Some(s.start) == target)
                            .map(|s| {
                                format!(
                                    "{} at {:.1}s · energy {:.0}",
                                    s.name,
                                    s.start,
                                    1. + s.energy * 9.
                                )
                            })
                            .unwrap_or_default();
                        ui.label(format!("Queued: {name} · preparing / waiting for grid"));
                        if ui.button("Cancel queued section").clicked() {
                            audio.cancel_section(deck);
                        }
                    }
                    2 => {
                        ui.label("Mixing section");
                    }
                    3 => {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            "Section could not be prepared; current audio retained",
                        );
                    }
                    _ => {}
                }
            }
        });
        egui::ScrollArea::horizontal()
            .id_salt(("deck-sections", deck))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for section in &profile.sections {
                        let current = position >= section.start && position < section.end;
                        if ui
                            .selectable_label(
                                current,
                                format!("{} · {:.0}", section.name, 1. + section.energy * 9.),
                            )
                            .on_hover_text(format!(
                                "{:.1}–{:.1} seconds · click to select / queue",
                                section.start, section.end
                            ))
                            .clicked()
                        {
                            choice = Some(section.start);
                        }
                    }
                });
            });
        if let Some(at) = choice {
            ui.ctx()
                .data_mut(|data| data.insert_temp(egui::Id::unique(("section-target", deck)), at));
            self.select_deck_section(deck, at);
        }
    }
    fn select_deck_section(&mut self, deck: usize, at: f64) {
        let Some(mut track) = self.loaded_track(deck).cloned() else {
            return;
        };
        if at < amp_core::energy_profile::entry_bound(&track.preparation)
            || track.preparation.cue_out.is_some_and(|out| at >= out)
        {
            self.status = "Section is outside this track's protected cue bounds".into();
            return;
        }
        let original = track.preparation.clone();
        track.preparation.cue_in = at;
        let root = self.loaded_origin[deck]
            .clone()
            .unwrap_or_else(|| self.root.clone());
        let Some(audio) = &mut self.audio else { return };
        if audio.fading() {
            self.status = "Finish or cancel the current crossfade before choosing a section".into();
            return;
        }
        let playing = audio.playing(deck);
        let was_auto = self.run.auto;
        let grid = crate::mix_schedule::grid(
            &original,
            &original,
            at,
            audio.source_position(deck) - audio.position(deck),
            self.run.transitions.timing,
        );
        self.run.cancel_effects(audio);
        audio.set_fader(audio.fader());
        self.run.cancel_automation();
        self.auto = false;
        let result = if playing {
            audio.schedule_section(deck, &root, &track, grid, self.fade_seconds)
        } else {
            audio.load(deck, &root, &track, false)
        };
        if playing && result.is_ok() {
            self.run.auto = was_auto;
        }
        if !playing
            && result.is_ok()
            && let Some(index) = self.loaded[deck]
        {
            self.run.preloaded = Some((deck, index));
        }
        self.status = match result {
            Ok(()) => if playing {
                if grid.is_some() {
                    "Section queued on the audio grid"
                } else {
                    "Section queued; no usable grid, mixing when ready"
                }
            } else {
                "Section selected — press Play to start"
            }
            .into(),
            Err(e) => e,
        };
    }
}
