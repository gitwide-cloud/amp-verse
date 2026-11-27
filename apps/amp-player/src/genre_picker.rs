//! Compact genre browsing; category selection stores the existing exact tag values.
use eframe::egui;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Picker {
    search: String,
}

fn category(tag: &str) -> &'static str {
    let value = tag.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| value.contains(word));
    if has(&[
        "acapella",
        "a cappella",
        "a capella",
        "instrumental",
        "karaoke",
    ]) {
        "Vocal / instrumental versions"
    } else if has(&["gospel", "christian", "worship", "religious", "praise"]) {
        "Gospel / Christian"
    } else if has(&["reggae", "reggaeton", "ska", "dancehall"]) {
        "Reggae / Caribbean"
    } else if has(&["metal", "deathcore", "grindcore"]) {
        "Metal"
    } else if has(&["hip hop", "hip-hop", "hiphop", "rap", "trap"]) {
        "Hip-Hop / Rap"
    } else if has(&["r&b", "rnb", "rhythm", "soul", "funk"]) {
        "R&B / Soul / Funk"
    } else if has(&[
        "house",
        "techno",
        "electro",
        "dance",
        "edm",
        "trance",
        "dubstep",
        "drum and bass",
        "drum & bass",
        "dnb",
        "d&b",
        "breakbeat",
        "jungle",
        "ambient",
        "downtempo",
        "trip-hop",
        "trip hop",
        "synthwave",
        "hardstyle",
        "disco",
        "garage",
    ]) {
        "Electronic / Dance"
    } else if has(&[
        "latin",
        "salsa",
        "bachata",
        "merengue",
        "cumbia",
        "bossa",
        "samba",
        "mariachi",
        "banda",
        "regional mexican",
    ]) {
        "Latin"
    } else if has(&["country", "folk", "americana", "bluegrass"]) {
        "Country / Folk"
    } else if has(&["jazz", "blues", "swing"]) {
        "Jazz / Blues"
    } else if has(&[
        "classical",
        "orchestra",
        "opera",
        "soundtrack",
        "score",
        "anime",
        "game music",
    ]) {
        "Classical / Soundtracks"
    } else if has(&["rock", "punk", "grunge", "alternative", "indie"]) {
        "Rock / Alternative"
    } else if has(&["pop"]) {
        "Pop"
    } else if has(&["world", "afro", "ethnic", "celtic"]) {
        "World"
    } else if has(&[
        "70s", "70's", "80s", "80's", "90s", "90's", "00s", "oldies", "decades",
    ]) {
        "Decades / Oldies"
    } else {
        "Other genres"
    }
}

fn useful(value: &str) -> bool {
    !matches!(
        value.trim().to_lowercase().as_str(),
        "" | "."
            | "-"
            | "?"
            | "unknown"
            | "<unknown>"
            | "none"
            | "null"
            | "n/a"
            | "other"
            | "(none)"
    )
}

fn groups(
    tags: impl IntoIterator<Item = String>,
    selected: &[String],
    query: &str,
) -> BTreeMap<&'static str, Vec<String>> {
    let query = query.trim().to_lowercase();
    let choices: BTreeSet<_> = tags
        .into_iter()
        .map(|tag| amp_core::genres::canonical(&tag))
        .filter(|tag| useful(tag))
        .collect();
    // Legacy/custom selections remain visible and removable, even if the tag disappeared.
    let mut choices = choices;
    choices.extend(selected.iter().cloned());
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for tag in choices {
        let group = category(&tag);
        if query.is_empty()
            || tag.to_lowercase().contains(&query)
            || group.to_lowercase().contains(&query)
            || (query == "edm" && group == "Electronic / Dance")
        {
            groups.entry(group).or_default().push(tag);
        }
    }
    groups
}

fn set_choices(selected: &mut Vec<String>, choices: &[String], include: bool) {
    if include {
        for tag in choices {
            if !selected.contains(tag) && selected.len() < 256 {
                selected.push(tag.clone());
            }
        }
    } else {
        selected.retain(|tag| !choices.contains(tag));
    }
}

impl Picker {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        selected: &mut Vec<String>,
        library: Option<&amp_core::Library>,
    ) {
        let summary = match selected.len() {
            0 => "Choose genres…".to_owned(),
            1 => selected[0].clone(),
            n => format!("{n} genres selected"),
        };
        let response = egui::ComboBox::from_id_salt("smart-genres")
            .selected_text(summary)
            .width(230.)
            .height(440.)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                ui.set_min_width(300.);
                let search_response = ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .id_salt("genre-search")
                        .hint_text("Search genres or categories…")
                        .desired_width(ui.available_width()),
                );
                #[cfg(test)]
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique("genre-search-hit"), search_response.rect);
                    d.insert_temp(egui::Id::unique("genre-visible-tags"), Vec::<String>::new());
                });
                #[cfg(not(test))]
                let _ = search_response;
                ui.horizontal(|ui| {
                    ui.weak(format!("{} selected", selected.len()));
                    if ui.small_button("Clear selection").clicked() {
                        selected.clear();
                    }
                    if !self.search.is_empty() && ui.small_button("Clear search").clicked() {
                        self.search.clear();
                    }
                });
                let tags = library
                    .into_iter()
                    .flat_map(|lib| &lib.tracks)
                    .flat_map(|track| amp_core::genres::values(&track.preparation));
                let groups = groups(tags, selected, &self.search);
                if groups.is_empty() {
                    ui.weak("No matching genres. Try another search.");
                }
                egui::ScrollArea::vertical()
                    .id_salt("genre-categories")
                    .max_height(310.)
                    .show(ui, |ui| {
                        for (name, choices) in groups {
                            let checked =
                                choices.iter().filter(|tag| selected.contains(tag)).count();
                            let label = if checked == 0 {
                                format!("{name} · {}", choices.len())
                            } else {
                                format!("{name} · {checked}/{} selected", choices.len())
                            };
                            let header = egui::CollapsingHeader::new(label).id_salt(name);
                            let header = if self.search.trim().is_empty() {
                                header
                            } else {
                                header.open(Some(true))
                            };
                            header.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let new = choices
                                        .iter()
                                        .filter(|tag| !selected.contains(tag))
                                        .count();
                                    if ui
                                        .add_enabled(
                                            selected.len() + new <= 256,
                                            egui::Button::new(if self.search.trim().is_empty() {
                                                "Select category"
                                            } else {
                                                "Select shown"
                                            }),
                                        )
                                        .clicked()
                                    {
                                        set_choices(selected, &choices, true);
                                    }
                                    if ui.small_button("Clear shown").clicked() {
                                        set_choices(selected, &choices, false);
                                    }
                                });
                                for tag in choices {
                                    let mut checked = selected.contains(&tag);
                                    let enabled = checked || selected.len() < 256;
                                    let response = ui.add_enabled(
                                        enabled,
                                        egui::Checkbox::new(&mut checked, &tag),
                                    );
                                    #[cfg(test)]
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp(
                                            egui::Id::unique(("genre-tag-hit", &tag)),
                                            response.rect,
                                        );
                                        let mut visible = d
                                            .get_temp::<Vec<String>>(egui::Id::unique(
                                                "genre-visible-tags",
                                            ))
                                            .unwrap_or_default();
                                        visible.push(tag.clone());
                                        d.insert_temp(
                                            egui::Id::unique("genre-visible-tags"),
                                            visible,
                                        );
                                    });
                                    if response.changed() {
                                        set_choices(selected, &[tag], checked);
                                    }
                                }
                            });
                        }
                    });
                ui.weak("Expand a category to choose individual genres.");
            });
        #[cfg(test)]
        ui.ctx().data_mut(|d| {
            d.insert_temp(egui::Id::unique("genre-picker-hit"), response.response.rect)
        });
        #[cfg(not(test))]
        let _ = response;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compact_categories_hide_junk_and_keep_existing_unknown_selections() {
        let tags = [
            ".",
            "<unknown>",
            "House",
            "Tech House",
            "RNB",
            "r&b",
            "Rock",
        ]
        .map(str::to_owned);
        let g = groups(tags, &["<unknown>".into()], "");
        assert_eq!(g["Electronic / Dance"], ["house", "tech house"]);
        assert_eq!(g["R&B / Soul / Funk"], ["R&B"]);
        assert_eq!(g["Other genres"], ["<unknown>"]);
        assert!(!g.values().flatten().any(|tag| tag == "."));
    }
    #[test]
    fn search_finds_subgenres_and_category_names_without_touching_selection() {
        let tags = ["Deep House", "Techno", "Rock"].map(str::to_owned);
        let selected = vec!["Rock".into()];
        assert_eq!(
            groups(tags.clone(), &selected, " HOUSE ")["Electronic / Dance"],
            ["deep house"]
        );
        assert_eq!(
            groups(tags.clone(), &selected, "electronic")["Electronic / Dance"].len(),
            2
        );
        assert!(groups(tags, &selected, "no-such-genre").is_empty());
        assert_eq!(selected, ["Rock"]);
    }
    #[test]
    fn selecting_and_clearing_only_affects_the_visible_choices() {
        let mut selected = vec!["Rock".into()];
        let shown = vec!["house".into(), "techno".into()];
        set_choices(&mut selected, &shown, true);
        set_choices(&mut selected, &shown, true);
        assert_eq!(selected, ["Rock", "house", "techno"]);
        set_choices(&mut selected, &["house".into()], false);
        assert_eq!(selected, ["Rock", "techno"]);
        let mut prep = amp_core::Preparation::default();
        prep.extensions
            .insert("genre".into(), serde_json::json!("Techno"));
        assert!(amp_core::genres::matches(&selected, &prep));
    }
    #[test]
    fn typing_in_picker_filters_real_checkboxes_and_keeps_other_selections() {
        let ctx = egui::Context::default();
        let mut picker = Picker::default();
        let mut selected = vec!["Rock".into()];
        let library = amp_core::Library {
            tree: amp_core::Folder {
                name: "Music".into(),
                folders: vec![],
                tracks: vec![0, 1, 2],
            },
            tracks: ["House", "Techno", "Rock"]
                .into_iter()
                .map(|genre| {
                    let mut preparation = amp_core::Preparation::default();
                    preparation
                        .extensions
                        .insert("genre".into(), serde_json::json!(genre));
                    amp_core::Track {
                        path: format!("{genre}.mp3").into(),
                        title: genre.into(),
                        preparation,
                    }
                })
                .collect(),
        };
        let mut clock = 0.;
        let mut frame = |events| {
            clock += 0.25;
            let mut out = ctx.run_ui(
                egui::RawInput {
                    time: Some(clock),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(420., 650.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| picker.show(ui, &mut selected, Some(&library)),
            );
            out.textures_delta.clear();
        };
        fn click(frame: &mut impl FnMut(Vec<egui::Event>), rect: egui::Rect) {
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
            frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }]);
        }
        let rect = |key| ctx.data(|d| d.get_temp::<egui::Rect>(key).unwrap());
        frame(vec![]);
        click(&mut frame, rect(egui::Id::unique("genre-picker-hit")));
        frame(vec![]);
        let search = rect(egui::Id::unique("genre-search-hit"));
        assert!(search.min.x >= 0. && search.max.x <= 420.);
        click(&mut frame, search);
        frame(vec![egui::Event::Text("house".into())]);
        frame(vec![]);
        let visible = ctx.data(|d| {
            d.get_temp::<Vec<String>>(egui::Id::unique("genre-visible-tags"))
                .unwrap()
        });
        assert_eq!(visible, ["house"]);
        click(
            &mut frame,
            rect(egui::Id::unique(("genre-tag-hit", "house"))),
        );
        drop(frame);
        assert_eq!(selected, ["Rock", "house"]);
    }
}
