//! Plain-language filter rows backed by the shared eligibility engine.
use amp_core::{
    Library,
    show::SmartPlaylist,
    smart_filters::{Field, Filter, Kind},
};
use eframe::egui;
pub fn show(ui: &mut egui::Ui, playlist: &mut SmartPlaylist, library: Option<&Library>) {
    ui.add_space(12.);
    ui.strong("Analysis and track filters");
    ui.weak("All filters must match. Text values separated by ; mean either value. Missing values do not match ranges or text filters.");
    let mut add = None;
    ui.horizontal_wrapped(|ui| {
        ui.menu_button("Add filter…", |ui| {
            egui::ScrollArea::vertical()
                .id_salt("available-smart-filters")
                .max_height(320.)
                .show(ui, |ui| {
                    for &field in Field::ALL {
                        if ui
                            .add_enabled(
                                playlist.filters.len() < 64
                                    && !playlist.filters.iter().any(|f| f.field() == field),
                                egui::Button::new(field.label()),
                            )
                            .clicked()
                        {
                            add = Some(field);
                            ui.close();
                        }
                    }
                });
        });
        if !playlist.filters.is_empty() && ui.button("Clear these filters").clicked() {
            playlist.filters.clear();
        }
    });
    if let Some(field) = add {
        playlist.filters.push(Filter::new(field));
    }
    let mut remove = None;
    for (index, filter) in playlist.filters.iter_mut().enumerate() {
        ui.push_id(("smart-filter",index),|ui|{
            ui.horizontal_wrapped(|ui|{
                if ui.small_button("×").on_hover_text("Remove this filter").clicked(){remove=Some(index);}
                ui.strong(filter.field().label());
                match filter {
                    Filter::Number{field,min,max}=>{
                        if let Kind::Number(lo,hi)=field.kind(){
                            ui.label("from");ui.add(egui::DragValue::new(min).range(lo..=hi).speed(0.5));
                            ui.label("to");ui.add(egui::DragValue::new(max).range(lo..=hi).speed(0.5));
                            if *min>*max {std::mem::swap(min,max);}
                        }
                    }
                    Filter::Flag{value,..}=>{ui.selectable_value(value,true,"Yes");ui.selectable_value(value,false,"No");}
                    Filter::Text{field,terms,exact}=>{
                        ui.selectable_value(exact,true,"is");ui.selectable_value(exact,false,"contains");
                        ui.add(egui::TextEdit::singleline(terms).desired_width(220.).hint_text("Value; another value"));
                        ui.menu_button("Choose values…",|ui|{
                            let mut values=std::collections::BTreeSet::new();
                            if let Some(library)=library{for track in &library.tracks{values.extend(amp_core::smart_filters::texts(*field,&track.preparation).into_iter().filter(|v|!v.trim().is_empty()));}}
                            if values.is_empty(){ui.weak("No saved values in this library yet.");}
                            egui::ScrollArea::vertical().max_height(220.).show(ui,|ui|{
                                for value in values {
                                    let mut parts:Vec<String>=terms.split(';').map(str::trim).filter(|v|!v.is_empty()).map(str::to_owned).collect();
                                    let mut selected=parts.iter().any(|s|s.eq_ignore_ascii_case(&value));
                                    if ui.checkbox(&mut selected,&value).changed(){if selected{parts.push(value);}else{parts.retain(|s|!s.eq_ignore_ascii_case(&value));}*terms=parts.join("; ");}
                                }
                            });
                        });
                    }
                }
            });
            if filter.field()==Field::StemCount {ui.weak("Counts saved stem entries. Device-local stem readiness is shown separately in the DJ library.");}
            if filter.field()==Field::SectionName {ui.weak("Uses saved section labels; automatic energy sections are not semantic verse/chorus labels.");}
        });
    }
    if let Some(index) = remove {
        playlist.filters.remove(index);
    }
    if !playlist.filters.is_empty() {
        let count = library.map_or(0, |l| {
            l.tracks
                .iter()
                .filter(|t| amp_core::show::eligible(playlist, t))
                .count()
        });
        ui.label(format!(
            "{count} songs match all saved source, genre and filter choices"
        ));
    }
}
