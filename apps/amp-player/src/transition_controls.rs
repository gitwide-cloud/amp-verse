//! Compact performance setup; the popup never consumes deck or waveform height.
use crate::{
    mix_schedule::Timing,
    transition_styles::{Settings, Style},
};
use eframe::egui;
pub fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    wave_span: &mut f64,
    can_mashup: bool,
) -> bool {
    let mut mashup = false;
    let button = ui.add_sized([118., 28.], egui::Button::new("Transitions ▾"));
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("transitions-button"), button.rect));
    egui::Popup::menu(&button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui|{
        let width=(ui.ctx().viewport_rect().width()-36.).clamp(250.,560.);
        ui.set_width(width);ui.strong("Transition settings");
        let id=egui::Id::unique("transition-settings-tab");let mut tab=ui.ctx().data_mut(|d|d.get_persisted::<usize>(id)).unwrap_or(0).min(2);
        ui.horizontal_wrapped(|ui|{for (i,label) in ["Timing","Effects","Advanced Mixing"].into_iter().enumerate(){if ui.selectable_label(tab==i,label).clicked(){tab=i;}}});
        ui.ctx().data_mut(|d|d.insert_persisted(id,tab));ui.separator();
        egui::ScrollArea::vertical().id_salt("transition-popup-scroll").max_height((ui.ctx().viewport_rect().height()-130.).clamp(100.,340.)).show(ui,|ui|{
            match tab {
                0=>{
                    ui.horizontal_wrapped(|ui|{for timing in [Timing::Immediate,Timing::Beat,Timing::Bar,Timing::Phrase8,Timing::Phrase16,Timing::Phrase32]{ui.selectable_value(&mut settings.timing,timing,timing.label()).on_hover_text("Uses prepared beat phase and reviewed phrase anchors. Missing grids use a timed blend.");}});
                    ui.toggle_value(&mut settings.techniques[5],"Beatmatch tempo").on_hover_text("Allows the existing beatmatch technique to adjust tempo within ±16%. It requires prepared BPM; phrase timing also requires a usable grid.");
                    ui.add(egui::Slider::new(wave_span,4. ..=60.).text("Waveform seconds"));
                }
                1=>{
                    ui.toggle_value(&mut settings.enabled,"Automatic transition effects").on_hover_text("Turns occasional effects on/off without changing your saved crossfade time or timing choice.");
                    ui.horizontal_wrapped(|ui|{for (i,name) in ["Scratch","Stutter","Tape brake","Filter sweep","Bass swap"].into_iter().enumerate(){ui.toggle_value(&mut settings.techniques[i],name);}});
                    ui.toggle_value(&mut settings.loop_build,"Loop buildup").on_hover_text("Needs BPM and at least six beats of mix time. Unsuitable pairs use a clean fade.");
                    ui.horizontal_wrapped(|ui|{ui.label("Selection");egui::ComboBox::from_id_salt("transition-style").selected_text(settings.style.label()).show_ui(ui,|ui|{for style in [Style::Variety,Style::Fade,Style::Scratch,Style::Stutter,Style::Brake,Style::Filter,Style::BassSwap,Style::Beatmatch,Style::LoopBuild,Style::StemSwap,Style::Mashup]{ui.selectable_value(&mut settings.style,style,style.label());}}).response.on_hover_text("Variety rotates enabled techniques. A specific technique must also be enabled; otherwise a clean fade is used.");});
                }
                _=>{
                    ui.toggle_value(&mut settings.stem_swap,"Stem handoff").on_hover_text("Needs prepared stems on both decks; otherwise a clean fade is used.");
                    ui.toggle_value(&mut settings.mashup,"Vocal / instrumental mashup").on_hover_text("Uses prepared stem sets, compatible tempo/tuning and a phrase grid. Only suitable pairs are combined.");
                    ui.horizontal_wrapped(|ui|{ui.label("Mashup hold");egui::ComboBox::from_id_salt("mashup-hold").selected_text(format!("{} beats",settings.mashup_beats)).show_ui(ui,|ui|{for beats in [8,16,32]{ui.selectable_value(&mut settings.mashup_beats,beats,format!("{beats} beats"));}}).response.on_hover_text("Separate from the crossfade duration.");});
                    ui.horizontal_wrapped(|ui|{ui.label("Effect frequency");ui.add(egui::DragValue::new(&mut settings.every).range(2..=12));ui.label("mixes");}).response.on_hover_text("Only enabled techniques are used, at this frequency. Clean blends fill the other transitions.");
                    mashup=ui.add_enabled(can_mashup,egui::Button::new("Mashup next pair")).on_hover_text("Requests the existing mashup handoff for the next suitable prepared pair.").clicked();
                }
            }
        });
    });
    mashup
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opening_settings_does_not_push_decks_down_or_reset_choices() {
        let ctx = egui::Context::default();
        let mut settings = Settings {
            enabled: false,
            mashup: true,
            mashup_beats: 32,
            loop_build: true,
            stem_swap: true,
            ..Default::default()
        };
        let original = settings;
        let mut wave = 24.;
        let mut button = egui::Rect::NOTHING;
        let mut heights = Vec::new();
        for click in [None, Some(true), Some(false), None] {
            let mut raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000., 700.),
                )),
                ..Default::default()
            };
            if button.is_finite() {
                raw.events.push(egui::Event::PointerMoved(button.center()));
                if let Some(pressed) = click {
                    raw.events.push(egui::Event::PointerButton {
                        pos: button.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    });
                }
            }
            let mut output = ctx.run_ui(raw, |ui| {
                let before = ui.next_widget_position().y;
                ui.horizontal(|ui| {
                    assert!(!show(ui, &mut settings, &mut wave, true));
                });
                heights.push(ui.next_widget_position().y - before);
            });
            output.textures_delta.clear();
            button = ctx
                .data(|d| d.get_temp(egui::Id::unique("transitions-button")))
                .unwrap();
        }
        assert!(heights.iter().all(|h| *h <= 36.), "{heights:?}");
        assert_eq!(settings, original);
        assert_eq!(wave, 24.);
    }
}
