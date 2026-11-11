use eframe::egui;
// Visible hot-cue slots shared by main and auxiliary decks.
use amp_core::{
    Preparation,
    preparation::{self, Cue},
};
use std::path::Path;
#[derive(Clone, Copy)]
pub enum Action {
    Jump(f64),
    Set(u16),
    Clear(u16),
}
pub fn slot(p: &Preparation, number: u16) -> Option<&Cue> {
    p.manual_cues
        .iter()
        .chain(&p.cues)
        .find(|c| c.number == Some(number))
}
pub fn pads(ui: &mut egui::Ui, p: Option<&Preparation>, accent: egui::Color32) -> Option<Action> {
    pads_count(ui, p, accent, 8)
}
pub fn pads_count(
    ui: &mut egui::Ui,
    p: Option<&Preparation>,
    accent: egui::Color32,
    count: usize,
) -> Option<Action> {
    let mut action = None;
    if count == 8 {
        ui.label("HOT CUES");
    }
    let columns = if ui.available_width() < 210. { 2 } else { 4 };
    let width = ((ui.available_width() - ui.spacing().item_spacing.x * (columns - 1) as f32)
        / columns as f32)
        .max(30.);
    for row in 0..count / columns {
        ui.horizontal(|ui| {
            for col in 0..columns {
                let number = (row * columns + col + 1) as u16;
                let cue = p.and_then(|p| slot(p, number));
                let response = ui
                    .add_enabled_ui(p.is_some(), |ui| {
                        ui.add_sized(
                            egui::vec2(width, if count == 4 { 30. } else { 48. }),
                            egui::Button::new("")
                                .stroke(egui::Stroke::new(1., accent.gamma_multiply(0.6))),
                        )
                    })
                    .inner;
                let painter = ui.painter().with_clip_rect(response.rect.shrink(2.));
                let color = if cue.is_some() {
                    accent
                } else {
                    ui.visuals().weak_text_color()
                };
                painter.text(
                    if count == 4 {
                        response.rect.center()
                    } else {
                        response.rect.center_top() + egui::vec2(0., 5.)
                    },
                    if count == 4 {
                        egui::Align2::CENTER_CENTER
                    } else {
                        egui::Align2::CENTER_TOP
                    },
                    format!("{number}"),
                    egui::FontId::proportional(13.),
                    color,
                );
                let time = cue.map_or_else(
                    || "Set cue".into(),
                    |cue| format!("{:02}:{:05.2}", cue.seconds as u64 / 60, cue.seconds % 60.),
                );
                if count != 4 {
                    painter.text(
                        response.rect.center_bottom() - egui::vec2(0., 5.),
                        egui::Align2::CENTER_BOTTOM,
                        time,
                        egui::FontId::proportional(11.),
                        color,
                    );
                }
                let response = response.on_hover_text(cue.map_or(
                    "Click to save the current position. Right-click for cue options.".into(),
                    |cue| {
                        format!(
                            "{} · {:.3}s. Click to jump; right-click to edit.",
                            cue.label, cue.seconds
                        )
                    },
                ));
                if response.clicked() {
                    action = Some(cue.map_or(Action::Set(number), |c| Action::Jump(c.seconds)));
                }
                response.context_menu(|ui| {
                    if ui.button("Set at current position").clicked() {
                        action = Some(Action::Set(number));
                        ui.close();
                    }
                    if cue.is_some() && ui.button("Clear cue").clicked() {
                        action = Some(Action::Clear(number));
                        ui.close();
                    }
                });
            }
        });
    }
    if let Some(p) = p {
        let extra: Vec<_> = p
            .manual_cues
            .iter()
            .chain(&p.cues)
            .filter(|cue| cue.number.is_none_or(|number| number > 8))
            .collect();
        if !extra.is_empty() {
            ui.menu_button("More saved / analysis cues", |ui| {
                for cue in extra {
                    if ui
                        .button(format!("{} · {:.2}s", cue.label, cue.seconds))
                        .clicked()
                    {
                        action = Some(Action::Jump(cue.seconds));
                        ui.close();
                    }
                }
            });
        }
    }
    action
}
pub fn edit(
    root: &Path,
    path: &Path,
    loaded: &Preparation,
    action: Action,
    position: f64,
) -> Result<Preparation, String> {
    let number = match action {
        Action::Set(n) | Action::Clear(n) => n,
        Action::Jump(_) => return Err("Jump does not edit metadata".into()),
    };
    let (mut current, revision) = preparation::load_with_revision(path)?;
    if slot(&current, number) != slot(loaded, number) {
        return Err(
            "This cue changed on another device. Reload the track before editing it.".into(),
        );
    }
    if !(1..=8).contains(&number) || !position.is_finite() || position < 0. {
        return Err("Invalid cue position or slot".into());
    }
    let previous = slot(&current, number).cloned();
    current.manual_cues.retain(|c| c.number != Some(number));
    current.cues.retain(|c| c.number != Some(number));
    if matches!(action, Action::Set(_)) {
        let mut cue = previous.unwrap_or_else(|| Cue {
            id: format!("manual-hot-cue-{number}"),
            label: format!("Cue {number}"),
            kind: "hot_cue".into(),
            number: Some(number),
            ..Default::default()
        });
        cue.seconds = position;
        current.manual_cues.push(cue);
    }
    current.manual_override = true;
    current.schema_version = Some(1);
    preparation::save_if_unchanged(
        root,
        path.strip_prefix(root)
            .map_err(|_| "Track is outside the library")?,
        &current,
        revision.as_deref(),
    )?;
    Ok(current)
}
/// Keep the current playback analysis snapshot intact when a user edits pads.
pub fn update_snapshot(target: &mut Preparation, saved: Preparation) {
    target.manual_cues = saved.manual_cues;
    target.cues = saved.cues;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn persistent_edits_merge_unrelated_fields_and_reject_stale_cues() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("song.wav");
        std::fs::write(&audio, b"original audio").unwrap();
        let original = Preparation::default();
        let first = edit(dir.path(), &audio, &original, Action::Set(1), 12.).unwrap();
        assert_eq!(
            preparation::load(&audio).unwrap().manual_cues[0].seconds,
            12.
        );
        assert!(edit(dir.path(), &audio, &original, Action::Set(1), 20.).is_err());
        let (mut other, revision) = preparation::load_with_revision(&audio).unwrap();
        other.bpm = Some(128.);
        preparation::save_if_unchanged(
            dir.path(),
            Path::new("song.wav"),
            &other,
            revision.as_deref(),
        )
        .unwrap();
        let second = edit(dir.path(), &audio, &first, Action::Set(2), 24.).unwrap();
        assert_eq!(second.bpm, Some(128.));
        assert_eq!(slot(&second, 1).unwrap().seconds, 12.);
        let cleared = edit(dir.path(), &audio, &second, Action::Clear(1), 24.).unwrap();
        assert!(slot(&cleared, 1).is_none());
        assert!(slot(&cleared, 2).is_some());
        assert_eq!(std::fs::read(&audio).unwrap(), b"original audio");
    }
    #[test]
    fn slots_use_saved_numbers_not_vector_order() {
        let mut p = Preparation::default();
        p.manual_cues.push(Cue {
            number: Some(7),
            seconds: 12.,
            ..Default::default()
        });
        assert!(slot(&p, 1).is_none());
        assert_eq!(slot(&p, 7).unwrap().seconds, 12.);
    }
    #[test]
    fn cue_updates_keep_loaded_tempo() {
        let mut loaded = Preparation {
            bpm: Some(120.),
            ..Default::default()
        };
        let saved = Preparation {
            bpm: Some(130.),
            ..Default::default()
        };
        update_snapshot(&mut loaded, saved);
        assert_eq!(loaded.bpm, Some(120.));
    }
}
