use crate::{Player, mix_schedule::Timing};
use eframe::egui;
use std::sync::atomic::Ordering::Relaxed;
impl Player {
    /// Live waveforms select musical sections, rather than abruptly seeking audio.
    pub(super) fn mix_to_waveform_section(&mut self, ctx: &egui::Context, deck: usize, at: f64) {
        if !at.is_finite() {
            return;
        }
        let Some(track) = self.loaded_track(deck) else {
            return;
        };
        let target = amp_core::energy_profile::read(&track.preparation)
            .and_then(|profile| {
                profile
                    .sections
                    .into_iter()
                    .find(|section| at >= section.start && at < section.end)
            })
            .map_or(at.max(0.), |section| {
                section
                    .start
                    .max(amp_core::energy_profile::entry_bound(&track.preparation))
            });
        ctx.data_mut(|data| data.insert_temp(egui::Id::unique(("section-target", deck)), target));
        self.select_deck_section(deck, target);
    }
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
    pub(super) fn select_deck_section(&mut self, deck: usize, at: f64) {
        if deck >= 6 || !at.is_finite() {
            self.status = "Choose a valid deck section".into();
            return;
        }
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
        let in_pair = self.pair().contains(&deck);
        let Some(audio) = &mut self.audio else { return };
        if in_pair && audio.fading() {
            self.status = "Finish or cancel the current crossfade before choosing a section".into();
            return;
        }
        let playing = audio.playing(deck);
        let was_auto = self.run.auto;
        // A section is the landing, rather than the beginning of the fade.
        // Work only on this session clone; approved source bounds stay intact.
        let rate = f64::from(audio.controls.effective_rate(deck));
        let pre_roll = at - f64::from(self.fade_seconds) * rate;
        if playing && pre_roll >= amp_core::energy_profile::entry_bound(&original) {
            track.preparation.cue_in = pre_roll;
        }
        let grid = crate::mix_schedule::grid(
            &original,
            &original,
            track.preparation.cue_in,
            audio.source_position(deck) - audio.position(deck),
            self.run.transitions.timing,
        );
        if in_pair {
            self.run.cancel_effects(audio);
            audio.set_fader(audio.fader());
            self.run.cancel_automation();
        }
        if deck == 0 {
            self.auto = false;
        }
        let result = if playing {
            audio.schedule_section(deck, &root, &track, grid, self.fade_seconds)
        } else {
            audio.load(deck, &root, &track, false)
        };
        if playing && result.is_ok() {
            self.run.auto = was_auto;
        }
        if !playing
            && in_pair
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_waveform_selects_section_start_without_editing_saved_cues() {
        let ctx = egui::Context::default();
        let mut player = Player::default();
        let mut track = amp_core::Track {
            path: "missing.mp3".into(),
            title: "Sections".into(),
            preparation: Default::default(),
        };
        track.preparation.extensions.insert("energy_profile".into(), serde_json::json!({"version":1,"duration":20.,"step":1.,"curve":[],"sections":[{"id":"intro","name":"Intro","start":0.,"end":10.,"energy":0.3},{"id":"drop","name":"Drop","start":10.,"end":20.,"energy":0.9}]}));
        track.preparation.cue_in = 5.;
        track.preparation.manual_override = true;
        player.loaded_snapshot[0] = Some(track);
        player.mix_to_waveform_section(&ctx, 0, 16.);
        assert_eq!(
            ctx.data(|d| d.get_temp::<f64>(egui::Id::unique(("section-target", 0usize)))),
            Some(10.)
        );
        assert_eq!(
            player.loaded_snapshot[0]
                .as_ref()
                .unwrap()
                .preparation
                .cue_in,
            5.
        );
        player.mix_to_waveform_section(&ctx, 0, 7.);
        assert_eq!(
            ctx.data(|d| d.get_temp::<f64>(egui::Id::unique(("section-target", 0usize)))),
            Some(5.)
        );
    }
}
