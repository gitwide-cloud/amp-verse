//! A separate settings window keeps transition choices out of the deck layout.
use crate::{
    mix_schedule::Timing,
    transition_styles::{Settings, Style},
};
use eframe::egui;
fn switch(ui: &mut egui::Ui, value: &mut bool, label: &str, help: &str) {
    ui.horizontal_wrapped(|ui| {
        crate::skin::switch(ui, value, label);
        ui.weak(help);
    });
}
fn simplify(settings: &mut Settings) {
    if settings.style == Style::Variety {
        return;
    }
    let chosen = settings.pick(settings.every.saturating_sub(1));
    if chosen.is_none() {
        settings.enabled = false;
    }
    for (index, style) in [
        Style::Scratch,
        Style::Stutter,
        Style::Brake,
        Style::Filter,
        Style::BassSwap,
        Style::Beatmatch,
    ]
    .into_iter()
    .enumerate()
    {
        settings.techniques[index] = chosen == Some(style);
    }
    settings.loop_build = chosen == Some(Style::LoopBuild);
    settings.stem_swap = chosen == Some(Style::StemSwap);
    settings.mashup = chosen == Some(Style::Mashup);
    settings.style = Style::Variety;
}

pub fn show(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    wave_span: &mut f64,
    can_mashup: bool,
) -> bool {
    let mut mashup = false;
    let button = ui.add_sized([118., 28.], egui::Button::new("Transitions"));
    #[cfg(test)]
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique("transitions-button"), button.rect));
    let id = egui::Id::unique("transitions-window-open");
    let mut open = ui
        .ctx()
        .data_mut(|d| d.get_temp::<bool>(id))
        .unwrap_or(false);
    if button.clicked() {
        open = true;
        simplify(settings);
    }
    egui::Window::new("Auto DJ transitions").id(egui::Id::unique("transitions-window")).open(&mut open).default_width(620.).default_height(560.).max_height((ui.ctx().viewport_rect().height()-60.).max(200.)).resizable(true).vscroll(true).show(ui.ctx(),|ui|{
        ui.label("Turn on the transitions you like. Auto DJ rotates through enabled types; clean blends fill the other mixes.");
        switch(ui,&mut settings.enabled,"Use effects","Master switch for occasional effects. Timing and your crossfade duration stay unchanged.");
        ui.separator();let previous=*settings;
        for (index,label,help) in [
            (0,"Scratch","A short back-and-forward flourish."),(1,"Stutter","A brief rhythmic repeat."),
            (2,"Tape brake","Slow the outgoing deck briefly."),(3,"Filter sweep","Sweep tone during the blend."),
            (4,"Bass swap","Trade the low frequencies between decks."),(5,"Beatmatch tempo","Match prepared tempos within the supported range."),
        ] {switch(ui,&mut settings.techniques[index],label,help);}
        switch(ui,&mut settings.loop_build,"Loop buildup","Shorten a beat loop before the handoff; requires BPM and enough mix time.");
        switch(ui,&mut settings.stem_swap,"Stem handoff","Requires prepared stems on both decks.");
        switch(ui,&mut settings.mashup,"Vocal / backing","A vocal/instrumental mashup for compatible prepared pairs.");
        if *settings!=previous {settings.style=Style::Variety;}
        ui.separator();
        ui.horizontal_wrapped(|ui|{ui.label("Use an effect every");ui.add(egui::DragValue::new(&mut settings.every).range(2..=12));ui.label("mixes");});
        ui.weak("Unsupported combinations use a clean blend. Turning every type off is safe.");
        egui::CollapsingHeader::new("Timing and mix options").show(ui,|ui|{
            ui.label("When to start a transition");
            ui.horizontal_wrapped(|ui|{for (timing,label) in [(Timing::Immediate,"Now"),(Timing::Beat,"Next beat"),(Timing::Bar,"Next bar"),(Timing::Phrase8,"2 bars"),(Timing::Phrase16,"4 bars"),(Timing::Phrase32,"8 bars")] {ui.selectable_value(&mut settings.timing,timing,label);}});
            ui.weak("Bar counts assume four beats. Missing grids use a timed blend.");
            ui.horizontal_wrapped(|ui|{ui.label("Mashup hold");for beats in [8,16,32] {ui.selectable_value(&mut settings.mashup_beats,beats,format!("{beats} beats"));}});
            ui.add(egui::Slider::new(wave_span,4. ..=60.).text("Waveform seconds"));
            mashup=ui.add_enabled(can_mashup,egui::Button::new("Try mashup on next pair")).clicked();
        });
    });
    ui.ctx().data_mut(|d| {
        d.insert_temp(id, open);
    });
    mashup
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_selection_becomes_equivalent_switches_and_all_off_uses_clean_blends() {
        let mut settings = Settings {
            style: Style::Scratch,
            ..Default::default()
        };
        let previous = settings.pick(3);
        simplify(&mut settings);
        assert_eq!(settings.pick(3), previous);
        assert_eq!(
            settings.techniques,
            [true, false, false, false, false, false]
        );
        settings.techniques = [false; 6];
        assert_eq!(settings.pick(3), None);
        let mut disabled = Settings {
            style: Style::StemSwap,
            stem_swap: false,
            ..Default::default()
        };
        simplify(&mut disabled);
        assert_eq!(disabled.pick(3), None);
    }
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
