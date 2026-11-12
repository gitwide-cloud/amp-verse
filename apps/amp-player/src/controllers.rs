//! Mixxx and partial VirtualDJ XML input interoperability. Never evaluates scripts or opens HID.
use crate::Player;
use eframe::egui;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{atomic::Ordering::Relaxed, mpsc},
};

const MAX_XML: usize = 2 * 1024 * 1024;
fn read_xml(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let mut xml = String::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_XML as u64 + 1)
        .read_to_string(&mut xml)
        .map_err(|e| e.to_string())?;
    if xml.len() > MAX_XML {
        return Err("Mapping exceeds 2 MB".into());
    }
    Ok(xml)
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Play(usize),
    Stop(usize),
    Sync(usize),
    Cue(usize),
    Loop(usize),
    Crossfader,
    Volume(usize),
    Eq(usize, usize),
    Pitch(usize),
    Phones(usize),
}
impl Target {
    fn continuous(self) -> bool {
        matches!(
            self,
            Self::Crossfader | Self::Volume(_) | Self::Eq(_, _) | Self::Pitch(_)
        )
    }
}
#[derive(Clone, Debug)]
struct Binding {
    status: u8,
    number: Option<u8>,
    target: Target,
    invert: bool,
}
#[derive(Clone, Debug)]
struct Mapping {
    name: String,
    bindings: Vec<Binding>,
    issues: Vec<String>,
}
fn child<'a>(n: roxmltree::Node<'a, 'a>, tag: &str) -> &'a str {
    n.children()
        .find(|n| n.has_tag_name(tag))
        .and_then(|n| n.text())
        .unwrap_or("")
        .trim()
}
fn number(s: &str) -> Result<u8, String> {
    let n = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u8::from_str_radix(hex, 16)
    } else {
        s.parse()
    };
    n.map_err(|_| format!("Invalid MIDI number: {s}"))
}
fn target(group: &str, key: &str) -> Option<Target> {
    if group == "[Master]" && key == "crossfader" {
        return Some(Target::Crossfader);
    }
    let d = match group {
        "[Channel1]" => 0,
        "[Channel2]" => 1,
        _ => return None,
    };
    Some(match key {
        "play" => Target::Play(d),
        "stop" => Target::Stop(d),
        "beatsync" | "beatsync_tempo" => Target::Sync(d),
        "cue_goto" => Target::Cue(d),
        "beatloop_4_toggle" => Target::Loop(d),
        "volume" => Target::Volume(d),
        "filterLow" => Target::Eq(d, 0),
        "filterMid" => Target::Eq(d, 1),
        "filterHigh" => Target::Eq(d, 2),
        "rate" => Target::Pitch(d),
        "pfl" => Target::Phones(d),
        _ => return None,
    })
}
fn parse(xml: &str) -> Result<Mapping, String> {
    if xml.len() > MAX_XML {
        return Err("Mapping exceeds 2 MB".into());
    }
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            nodes_limit: 100_000,
            ..Default::default()
        },
    )
    .map_err(|e| format!("Invalid mapping XML: {e}"))?;
    let root = doc.root_element();
    if root.attribute("schemaVersion").is_some_and(|v| v != "1") {
        return Err("Unsupported mapping schema version".into());
    }
    if !matches!(
        root.tag_name().name(),
        "MixxxMIDIPreset" | "MixxxControllerPreset"
    ) {
        return Err("Choose a Mixxx .midi.xml mapping. HID mappings are not supported.".into());
    }
    let controllers: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name("controller"))
        .collect();
    if controllers.len() != 1 {
        return Err("Import a mapping with one controller per file.".into());
    }
    let controller = controllers[0];
    let mut mapping = Mapping {
        name: root
            .children()
            .find(|n| n.has_tag_name("info"))
            .map(|n| child(n, "name").to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                controller
                    .attribute("id")
                    .unwrap_or("Imported controller")
                    .into()
            }),
        bindings: Vec::new(),
        issues: Vec::new(),
    };
    let scripts = controller
        .descendants()
        .filter(|n| n.has_tag_name("file"))
        .count();
    if scripts > 0 {
        mapping.issues.push(format!("{scripts} script files are not executed. Scripted modes/jog wheels need an AMP mapping."));
    }
    let outputs = controller
        .descendants()
        .filter(|n| n.has_tag_name("output"))
        .count();
    if outputs > 0 {
        mapping.issues.push(format!(
            "{outputs} output mappings skipped: LEDs/displays are not driven."
        ));
    }
    for c in controller
        .children()
        .filter(|n| n.has_tag_name("controls"))
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name("control"))
    {
        if mapping.bindings.len() + mapping.issues.len() >= 4096 {
            return Err("Too many mapping controls".into());
        }
        let group = child(c, "group");
        let key = child(c, "key");
        let options: Vec<_> = c
            .children()
            .filter(|n| n.has_tag_name("options"))
            .flat_map(|n| n.children())
            .filter(|n| n.is_element())
            .map(|n| n.tag_name().name().to_ascii_lowercase())
            .collect();
        let result = (|| {
            let target = target(group, key).ok_or("Control has no AMP equivalent")?;
            if options
                .iter()
                .any(|s| !matches!(s.as_str(), "normal" | "button" | "invert" | "soft-takeover"))
            {
                return Err("Script, relative encoder, or other unsupported input option");
            }
            if options.iter().any(|s| s == "button") && target.continuous() {
                return Err("Button option on a continuous control");
            }
            let status = number(child(c, "status")).map_err(|_| "Invalid status")?;
            if !matches!(status & 0xf0, 0x90 | 0xb0 | 0xe0) {
                return Err("Unsupported MIDI message type");
            }
            if target.continuous() && status & 0xf0 == 0x90 {
                return Err("Note-controlled fader requires a custom mapping");
            }
            if !target.continuous() && status & 0xf0 == 0xe0 {
                return Err("Pitch bend cannot act as a button");
            }
            let number = if status & 0xf0 == 0xe0 {
                None
            } else {
                let n = number(child(c, "midino")).map_err(|_| "Invalid MIDI control number")?;
                if n > 127 {
                    return Err("MIDI number exceeds 127");
                }
                Some(n)
            };
            let invert = options.iter().any(|s| s == "invert");
            if invert && !target.continuous() {
                return Err("Inverted button requires custom mapping");
            }
            Ok(Binding {
                status,
                number,
                target,
                invert,
            })
        })();
        match result {
            Ok(b) => mapping.bindings.push(b),
            Err(why) => mapping.issues.push(format!("{group} {key}: {why}")),
        }
    }
    // Ambiguous hardware messages must never trigger several different commands.
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();
    for b in &mapping.bindings {
        if !seen.insert((b.status, b.number)) {
            duplicates.insert((b.status, b.number));
        }
    }
    mapping
        .bindings
        .retain(|b| !duplicates.contains(&(b.status, b.number)));
    if !duplicates.is_empty() {
        mapping.issues.push(format!(
            "{} ambiguous MIDI assignments skipped",
            duplicates.len()
        ));
    }
    if mapping.bindings.is_empty() {
        return Err(format!(
            "No supported input controls. {}",
            mapping.issues.join("; ")
        ));
    }
    Ok(mapping)
}
// Translate only literal actions. Imported VDJScript is never executed.
fn virtualdj_xml(mapper: &str, definition: &str) -> Result<(String, Vec<String>), String> {
    fn document(xml: &str) -> Result<roxmltree::Document<'_>, String> {
        if xml.len() > MAX_XML {
            return Err("Mapping exceeds 2 MB".into());
        }
        roxmltree::Document::parse_with_options(
            xml,
            roxmltree::ParsingOptions {
                nodes_limit: 100_000,
                ..Default::default()
            },
        )
        .map_err(|e| format!("Invalid VirtualDJ XML: {e}"))
    }
    let maps = document(mapper)?;
    let defs = document(definition)?;
    let m = maps.root_element();
    let d = defs.root_element();
    if m.children().count() > 4096 || d.children().count() > 4096 {
        return Err("Too many mapping controls".into());
    }
    if !m.has_tag_name("mapper") || !d.has_tag_name("device") || d.attribute("type") != Some("MIDI")
    {
        return Err(
            "Choose a VirtualDJ mapper XML and its MIDI device-definition XML. HID is unsupported."
                .into(),
        );
    }
    if m.attribute("device").is_none() || m.attribute("device") != d.attribute("name") {
        return Err("The mapper and device definition belong to different controllers.".into());
    }
    let mut controls = String::new();
    let mut issues = Vec::new();
    let mut count = 0;
    for map in m.children().filter(|n| n.has_tag_name("map")) {
        if issues.len() >= 4096 {
            return Err("Too many mapping controls".into());
        }
        let name = map.attribute("value").unwrap_or("");
        let action = map.attribute("action").unwrap_or("").trim();
        let elements: Vec<_> = d
            .children()
            .filter(|n| n.attribute("name") == Some(name))
            .collect();
        if elements.is_empty() {
            issues.push(format!("{name}: no device input definition"));
        }
        for input in elements {
            count += 1;
            if count > 4096 {
                return Err("Too many mapping controls".into());
            }
            let result = (|| -> Result<String, &'static str> {
                let mut words = action.split_whitespace();
                let mut deck = input
                    .attribute("deck")
                    .and_then(|v| v.parse::<usize>().ok());
                let mut verb = words.next().ok_or("Empty action")?;
                if verb == "deck" {
                    deck = Some(
                        words
                            .next()
                            .and_then(|v| v.parse().ok())
                            .ok_or("Only numbered decks are supported")?,
                    );
                    verb = words.next().ok_or("Missing action")?;
                }
                let key = match verb {
                    "play_pause" => "play",
                    "stop" => "stop",
                    "sync" => "beatsync",
                    "volume" => "volume",
                    "eq_low" => "filterLow",
                    "eq_mid" => "filterMid",
                    "eq_high" => "filterHigh",
                    "pitch_slider" => "rate",
                    "pfl" => "pfl",
                    "crossfader" => "crossfader",
                    _ => return Err("Unsupported action or script"),
                };
                if words.next().is_some() {
                    return Err("Action parameters, chains and conditions are unsupported");
                }
                let group = if key == "crossfader" {
                    "[Master]"
                } else {
                    match deck {
                        Some(1) => "[Channel1]",
                        Some(2) => "[Channel2]",
                        _ => return Err("Only fixed deck 1/2 assignments are supported"),
                    }
                };
                if input.attributes().any(|a| {
                    !matches!(
                        a.name(),
                        "name"
                            | "deck"
                            | "channel"
                            | "note"
                            | "cc"
                            | "pitch"
                            | "inverted"
                            | "autoled"
                            | "ghost"
                    )
                }) {
                    return Err("Special input encoding or deck layers are unsupported");
                }
                if input
                    .attribute("inverted")
                    .is_some_and(|v| !matches!(v, "true" | "false"))
                {
                    return Err("Unknown inversion encoding");
                }
                let channel = input
                    .attribute("channel")
                    .ok_or("An explicit MIDI channel is required")?
                    .parse::<u8>()
                    .map_err(|_| "Invalid MIDI channel")?;
                if channel > 15 {
                    return Err("Invalid MIDI channel");
                }
                let continuous = target(group, key).unwrap().continuous();
                let (status, num) = match input.tag_name().name() {
                    "button"
                        if !continuous
                            && input.attribute("inverted").is_none_or(|v| v == "false") =>
                    {
                        match (input.attribute("note"), input.attribute("cc")) {
                            (Some(n), None) => {
                                (0x90 | channel, number(n).map_err(|_| "Invalid note")?)
                            }
                            (None, Some(n)) => {
                                (0xb0 | channel, number(n).map_err(|_| "Invalid CC")?)
                            }
                            _ => return Err("Ambiguous button encoding"),
                        }
                    }
                    "slider" if continuous => {
                        match (
                            input.attribute("cc"),
                            input.attribute("pitch"),
                            input.attribute("note"),
                        ) {
                            (Some(n), None, None) => {
                                (0xb0 | channel, number(n).map_err(|_| "Invalid CC")?)
                            }
                            (None, Some("true"), None) => (0xe0 | channel, 0),
                            _ => return Err("Only absolute CC or pitch sliders are supported"),
                        }
                    }
                    _ => return Err("Jog, relative, HID, output or mismatched control skipped"),
                };
                if num > 127 {
                    return Err("MIDI number exceeds 127");
                }
                let invert = if input.attribute("inverted") == Some("true") {
                    "<invert/>"
                } else {
                    ""
                };
                Ok(format!(
                    "<control><group>{group}</group><key>{key}</key><status>{status}</status><midino>{num}</midino><options>{invert}</options></control>"
                ))
            })();
            match result {
                Ok(c) => controls.push_str(&c),
                Err(e) => issues.push(format!("{name}: {e}")),
            }
        }
    }
    let xml = format!(
        "<MixxxMIDIPreset><controller id=\"VirtualDJ import\"><controls>{controls}</controls></controller></MixxxMIDIPreset>"
    );
    parse(&xml)?;
    Ok((xml, issues))
}
fn message_value(b: &Binding, message: &[u8]) -> Option<f32> {
    if message.len() == 3
        && b.status & 0xf0 == 0x90
        && message[0] == b.status - 0x10
        && b.number == Some(message[1])
    {
        return Some(0.);
    }
    if message.len() != 3 || message[0] != b.status || message[1] > 127 || message[2] > 127 {
        return None;
    }
    let mut value = if let Some(n) = b.number {
        if message[1] != n {
            return None;
        }
        message[2] as f32 / 127.
    } else {
        (message[1] as u16 | ((message[2] as u16) << 7)) as f32 / 16383.
    };
    if b.invert {
        value = 1. - value;
    }
    Some(value)
}
#[derive(Default, Clone)]
struct Pickup {
    previous: Option<f32>,
    acquired: bool,
    down: bool,
}
impl Pickup {
    fn accept(&mut self, value: f32, current: f32) -> bool {
        if self.acquired && self.previous.is_some_and(|p| (p - current).abs() > 0.05) {
            self.acquired = false;
        }
        if (value - current).abs() <= 0.025
            || self
                .previous
                .is_some_and(|p| (p - current) * (value - current) <= 0.)
        {
            self.acquired = true;
        }
        self.previous = Some(value);
        self.acquired
    }
}
fn profile_path() -> Option<PathBuf> {
    crate::theme::settings_path().map(|p| p.with_file_name("controllers.ampdb"))
}
pub struct Panel {
    mapping: Option<Mapping>,
    state: Vec<Pickup>,
    ports: Vec<(String, String)>,
    selected: Option<String>,
    connection: Option<midir::MidiInputConnection<()>>,
    receiver: Option<mpsc::Receiver<[u8; 3]>>,
    status: String,
}
impl Default for Panel {
    fn default() -> Self {
        let mapping = profile_path()
            .and_then(|p| {
                amp_core::catalog_db::local_record(&p, "mixxx-import")
                    .ok()
                    .flatten()
            })
            .and_then(|v| {
                let mut mapping = parse(v["xml"].as_str()?).ok()?;
                if let Some(issues) = v["issues"].as_array() {
                    mapping
                        .issues
                        .extend(issues.iter().filter_map(|v| v.as_str().map(str::to_owned)));
                }
                Some(mapping)
            });
        Self {
            state: vec![Pickup::default(); mapping.as_ref().map_or(0, |m| m.bindings.len())],
            mapping,
            ports: Vec::new(),
            selected: None,
            connection: None,
            receiver: None,
            status:
                "Import a MIDI mapping, review the supported controls, then choose your device."
                    .into(),
        }
    }
}
impl Panel {
    fn import(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.install_mapping(read_xml(path)?, Vec::new())
    }
    fn import_virtualdj(
        &mut self,
        mapper: &std::path::Path,
        definition: &std::path::Path,
    ) -> Result<(), String> {
        let (xml, issues) = virtualdj_xml(&read_xml(mapper)?, &read_xml(definition)?)?;
        self.install_mapping(xml, issues)
    }
    fn install_mapping(&mut self, xml: String, issues: Vec<String>) -> Result<(), String> {
        let mut mapping = parse(&xml)?;
        mapping.issues.extend(issues.clone());
        let profile = profile_path().ok_or("Settings folder unavailable")?;
        amp_core::catalog_db::save_local_record(
            &profile,
            "mixxx-import",
            &serde_json::json!({"xml":xml,"issues":issues}),
        )?;
        self.disconnect();
        self.state = vec![Pickup::default(); mapping.bindings.len()];
        self.mapping = Some(mapping);
        self.status = "Mapping imported. Review skipped controls before connecting; compatibility is partial.".into();
        Ok(())
    }
    fn disconnect(&mut self) {
        self.connection = None;
        self.receiver = None;
        self.state.fill(Pickup::default());
    }
    fn refresh(&mut self) -> Result<(), String> {
        let input = midir::MidiInput::new("AMP DJ device discovery").map_err(|e| e.to_string())?;
        self.ports = input
            .ports()
            .iter()
            .filter_map(|p| {
                input
                    .port_name(p)
                    .ok()
                    .map(|name| (p.id().to_string(), name))
            })
            .collect();
        Ok(())
    }
    fn connect(&mut self, ctx: &egui::Context) -> Result<(), String> {
        let id = self.selected.as_ref().ok_or("Choose a MIDI input")?;
        if self.mapping.is_none() {
            return Err("Import a mapping first".into());
        }
        let mut input = midir::MidiInput::new("AMP DJ controller").map_err(|e| e.to_string())?;
        input.ignore(midir::Ignore::SysexAndTime);
        let port = input
            .find_port_by_id(id)
            .ok_or("Device disappeared; refresh devices")?;
        let (tx, rx) = mpsc::sync_channel(512);
        let ctx = ctx.clone();
        let connection = input
            .connect(
                &port,
                "AMP DJ controller",
                move |_, data, _| {
                    if let Ok(message) = <[u8; 3]>::try_from(data) {
                        let _ = tx.try_send(message);
                        ctx.request_repaint();
                    }
                },
                (),
            )
            .map_err(|e| e.to_string())?;
        self.disconnect();
        self.receiver = Some(rx);
        self.connection = Some(connection);
        self.status = "Connected. Move faders to their on-screen positions to pick them up.".into();
        Ok(())
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.heading("DJ controllers");
        ui.hyperlink_to(
            "Get mappings from Mixxx",
            "https://github.com/mixxxdj/mixxx/tree/main/res/controllers",
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Import VirtualDJ mapping…").clicked()
                && let Some(mapper) = rfd::FileDialog::new()
                    .set_title("Choose VirtualDJ mapping (Mappers folder)")
                    .add_filter("VirtualDJ mapper XML", &["xml"])
                    .pick_file()
                && let Some(definition) = rfd::FileDialog::new()
                    .set_title("Choose matching MIDI definition (Devices folder)")
                    .add_filter("VirtualDJ MIDI definition XML", &["xml"])
                    .pick_file()
                && let Err(e) = self.import_virtualdj(&mapper, &definition)
            {
                self.status = e;
            }
            if ui.button("Import Mixxx mapping…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Mixxx MIDI XML", &["xml"])
                    .pick_file()
                && let Err(e) = self.import(&path)
            {
                self.status = e;
            }
            if ui.button("Find MIDI devices").clicked()
                && let Err(e) = self.refresh()
            {
                self.status = e;
            }
            if ui
                .add_enabled(self.connection.is_some(), egui::Button::new("Disconnect"))
                .clicked()
            {
                self.disconnect();
                self.status = "Disconnected".into();
            }
        });
        egui::ComboBox::from_id_salt("midi-port")
            .selected_text(
                self.selected
                    .as_ref()
                    .and_then(|id| self.ports.iter().find(|(p, _)| p == id))
                    .map_or("Choose MIDI input", |(_, name)| name.as_str()),
            )
            .show_ui(ui, |ui| {
                for (id, name) in &self.ports {
                    ui.selectable_value(&mut self.selected, Some(id.clone()), name);
                }
            });
        if ui
            .add_enabled(
                self.mapping.is_some() && self.selected.is_some() && self.connection.is_none(),
                egui::Button::new("Connect supported controls"),
            )
            .clicked()
            && let Err(e) = self.connect(ui.ctx())
        {
            self.status = e;
        }
        ui.label(&self.status);
        ui.weak("Partial Mixxx / VirtualDJ MIDI XML support. VirtualDJ needs both mapper and matching device-definition files; built-in definitions may not be exportable. No scripts, HID, jog wheels, shift layers or LED feedback. Review skipped controls before connecting.");
        if let Some(mapping) = &self.mapping {
            ui.label(format!(
                "{} · {} supported inputs",
                mapping.name,
                mapping.bindings.len()
            ));
            egui::CollapsingHeader::new("Review supported and skipped controls").show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(200.)
                    .show(ui, |ui| {
                        for b in &mapping.bindings {
                            ui.label(format!("{:02X} {:?} → {:?}", b.status, b.number, b.target));
                        }
                        for issue in &mapping.issues {
                            ui.label(issue);
                        }
                    });
            });
        }
    }
}
impl Player {
    pub(super) fn poll_controllers(&mut self) {
        let messages: Vec<_> = self
            .controllers
            .receiver
            .as_ref()
            .map(|rx| rx.try_iter().take(512).collect())
            .unwrap_or_default();
        if messages.is_empty() {
            return;
        }
        let bindings = self
            .controllers
            .mapping
            .as_ref()
            .map(|m| m.bindings.clone())
            .unwrap_or_default();
        for message in messages {
            for (index, b) in bindings.iter().enumerate() {
                let Some(value) = message_value(b, &message) else {
                    continue;
                };
                // Hardware takeover stops pending automation but never edits the shared library.
                let current = match b.target {
                    Target::Crossfader => self.audio.as_ref().map_or(self.fader, |a| a.fader()),
                    Target::Volume(d) => self.channel_gain[d],
                    Target::Pitch(d) => (16. - self.pitch[d]) / 32.,
                    Target::Eq(d, band) => {
                        if self.eq[d][band] <= 0. {
                            (self.eq[d][band] + 24.) / 48.
                        } else {
                            0.5 + self.eq[d][band] / 24.
                        }
                    }
                    _ => 0.,
                };
                let state = &mut self.controllers.state[index];
                if b.target.continuous() {
                    if !state.accept(value, current) {
                        continue;
                    }
                } else {
                    let down = value > 0.;
                    let pressed = down && !state.down;
                    state.down = down;
                    if !pressed {
                        continue;
                    }
                }
                if let Err(e) = self.controller_input(b.target, value) {
                    self.controllers.status = e;
                }
            }
        }
    }
    fn controller_input(&mut self, target: Target, value: f32) -> Result<(), String> {
        if !value.is_finite() || !(0. ..=1.).contains(&value) {
            return Err("Invalid controller value".into());
        }
        let deck = match target {
            Target::Crossfader => None,
            Target::Play(d)
            | Target::Stop(d)
            | Target::Sync(d)
            | Target::Cue(d)
            | Target::Loop(d)
            | Target::Volume(d)
            | Target::Pitch(d)
            | Target::Eq(d, _)
            | Target::Phones(d) => Some(d),
        };
        if deck.is_some_and(|d| d >= 6) || matches!(target,Target::Eq(_,b) if b>=3) {
            return Err("Invalid controller target".into());
        }
        if let Target::Play(d) | Target::Cue(d) | Target::Loop(d) | Target::Sync(d) = target {
            let audio = self.audio.as_ref().ok_or("Load a track first")?;
            if !audio.load_current(d) || !audio.loaded(d) {
                return Err("Deck is not ready".into());
            }
            if matches!(target, Target::Cue(_) | Target::Loop(_)) && self.loaded_track(d).is_none()
            {
                return Err("Load a track first".into());
            }
            if matches!(target, Target::Loop(_))
                && self
                    .loaded_track(d)
                    .and_then(|t| t.preparation.bpm)
                    .is_none()
            {
                return Err("Prepare BPM before using beat loops".into());
            }
            if matches!(target, Target::Sync(_)) {
                audio.sync(d, !audio.controls.synced[d].load(Relaxed))?;
            }
        }
        if matches!(target, Target::Crossfader) && self.audio.is_none() {
            return Err("Load a track first".into());
        }
        let manual = !matches!(target, Target::Phones(_));
        if manual
            && (matches!(target, Target::Crossfader)
                || deck.is_some_and(|d| self.pair().contains(&d)))
        {
            if let Some(a) = &self.audio {
                self.run.cancel_effects(a);
            }
            self.run.cancel_automation();
        }
        if manual && (deck == Some(0) || matches!(target, Target::Crossfader)) {
            self.auto = false;
        }
        match target {
            Target::Play(d) => {
                let audio = self.audio.as_ref().ok_or("Load a track first")?;
                if !audio.load_current(d) || !audio.loaded(d) {
                    return Err("Deck is not ready".into());
                }
                let play =
                    !audio.playing(d) || self.playback_owner != crate::playback_owner::Owner::Dj;
                if play {
                    self.transport_play(crate::playback_owner::Owner::Dj, d)?;
                } else {
                    self.audio.as_ref().unwrap().controls.play[d].store(false, Relaxed);
                }
            }
            Target::Stop(d) => {
                self.stop(d);
            }
            Target::Sync(_) => {}
            Target::Cue(d) => {
                let cue = self
                    .loaded_track(d)
                    .ok_or("Load a track first")?
                    .preparation
                    .cue_in;
                let position = self
                    .audio
                    .as_ref()
                    .ok_or("Load a track first")?
                    .source_position(d);
                self.seek_deck_relative(d, cue - position);
            }
            Target::Loop(d) => {
                let bpm = self
                    .loaded_track(d)
                    .and_then(|t| t.preparation.bpm)
                    .ok_or("Prepare BPM before using beat loops")?;
                self.audio
                    .as_ref()
                    .ok_or("Load a track first")?
                    .loop_toggle(
                        d,
                        (240. / (bpm * (1. + self.pitch[d] as f64 / 100.))) as f32,
                    )?;
            }
            Target::Crossfader => {
                self.fader = value;
                self.audio
                    .as_ref()
                    .ok_or("Load a track first")?
                    .set_fader(value);
            }
            Target::Volume(d) => {
                self.channel_gain[d] = value;
                if let Some(a) = &self.audio {
                    a.set_gain(d, value);
                }
            }
            Target::Pitch(d) => {
                self.pitch[d] = (0.5 - value) * 32.;
                if let Some(a) = &self.audio {
                    a.set_rate(d, 1. + self.pitch[d] / 100.);
                }
            }
            Target::Eq(d, band) => {
                self.eq[d][band] = if value <= 0.5 {
                    (value - 0.5) * 48.
                } else {
                    (value - 0.5) * 24.
                };
                if let Some(a) = &self.audio {
                    a.set_eq(d, band, self.eq[d][band]);
                }
            }
            Target::Phones(d) => {
                self.mixer_panel.toggle_cue(d);
                if let Some(a) = &self.audio {
                    self.mixer_panel.apply(a);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_controller_commands_and_background_faders_preserve_automation() {
        let mut p = Player {
            auto: true,
            ..Default::default()
        };
        p.run.auto = true;
        p.run.preloaded = Some((1, 0));
        for target in [
            Target::Play(1),
            Target::Cue(1),
            Target::Loop(1),
            Target::Sync(1),
            Target::Crossfader,
        ] {
            assert!(p.controller_input(target, 1.).is_err());
            assert!(p.auto && p.run.auto);
            assert_eq!(p.run.preloaded, Some((1, 0)));
        }
        p.controller_input(Target::Volume(2), 0.7).unwrap();
        assert!(p.run.auto && p.auto);
        assert_eq!(p.channel_gain[2], 0.7);
        assert!(p.controller_input(Target::Eq(0, 7), 0.5).is_err());
        assert!(p.controller_input(Target::Volume(0), f32::NAN).is_err());
        assert!(p.auto && p.run.auto);
    }
    #[test]
    fn virtualdj_import_translates_literal_actions_and_reports_scripts() {
        let mapper = r#"<mapper device="TEST"><map value="PLAY" action="play_pause"/><map value="VOL" action="volume"/><map value="PITCH" action="deck 2 pitch_slider"/><map value="JOG" action="jogwheel"/><map value="SHIFT" action="var 'shift' ? play_pause : sync"/></mapper>"#;
        let definition = r#"<device name="TEST" type="MIDI"><button name="PLAY" note="0x11" channel="0" deck="1"/><slider name="VOL" cc="7" channel="1" deck="2" inverted="true"/><slider name="PITCH" pitch="true" channel="1"/><jog name="JOG" cc="9" channel="0" deck="1"/><button name="SHIFT" note="3" channel="0" deck="1"/></device>"#;
        let (xml, issues) = virtualdj_xml(mapper, definition).unwrap();
        let mapping = parse(&xml).unwrap();
        assert_eq!(mapping.bindings.len(), 3);
        assert_eq!(mapping.bindings[0].target, Target::Play(0));
        assert_eq!(
            message_value(&mapping.bindings[0], &[0x90, 0x11, 127]),
            Some(1.)
        );
        assert_eq!(
            message_value(&mapping.bindings[0], &[0x80, 0x11, 0]),
            Some(0.)
        );
        assert_eq!(mapping.bindings[1].target, Target::Volume(1));
        assert!(mapping.bindings[1].invert);
        assert_eq!(mapping.bindings[2].status, 0xe1);
        assert_eq!(issues.len(), 2);
    }
    #[test]
    fn virtualdj_import_rejects_missing_definitions_unsafe_encodings_and_ambiguity() {
        let mapper = r#"<mapper device="T"><map value="P" action="play_pause"/></mapper>"#;
        let valid = r#"<device name="T" type="MIDI"><button name="P" note="1" channel="0" deck="1"/></device>"#;
        assert!(virtualdj_xml(mapper, valid).is_ok());
        for definition in [
            valid.replace("name=\"T\"", "name=\"Other\""),
            valid.replace("type=\"MIDI\"", "type=\"HID\""),
            valid.replace("channel=\"0\"", "channel=\"16\""),
            valid.replace("channel=\"0\"", ""),
            valid.replace("note=\"1\"", "note=\"128\""),
            valid.replace("note=\"1\"", "note=\"1\" nbdecks=\"2\""),
            valid.replace(
                "</device>",
                "<button name=\"P\" note=\"1\" channel=\"0\" deck=\"2\"/></device>",
            ),
        ] {
            assert!(virtualdj_xml(mapper, &definition).is_err(), "{definition}");
        }
        assert!(
            virtualdj_xml(
                mapper,
                "<!DOCTYPE device [<!ENTITY x SYSTEM 'file:///private'>]><device>&x;</device>"
            )
            .is_err()
        );
        assert!(virtualdj_xml(&" ".repeat(MAX_XML + 1), valid).is_err());
    }
    #[test]
    #[ignore = "requires a working native MIDI service; no ports are connected"]
    fn native_midi_backend_enumerates_without_taking_control() {
        let input = midir::MidiInput::new("AMP controller verification").unwrap();
        for port in input.ports() {
            assert!(input.port_name(&port).is_ok());
            assert!(!port.id().is_empty());
        }
    }
    // Original fixture describing the public file format, not a bundled Mixxx mapping.
    fn xml(controls: &str) -> String {
        format!(
            "<MixxxControllerPreset schemaVersion=\"1\"><info><name>Test</name></info><controller id=\"Test MIDI\"><controls>{controls}</controls></controller></MixxxControllerPreset>"
        )
    }
    fn control(group: &str, key: &str, status: &str, n: &str, options: &str) -> String {
        format!(
            "<control><group>{group}</group><key>{key}</key><status>{status}</status><midino>{n}</midino><options>{options}</options></control>"
        )
    }
    #[test]
    fn import_distinguishes_supported_controls_scripts_and_unknown_options() {
        let m = parse(&xml(&(control(
            "[Channel1]",
            "play",
            "0x90",
            "0x01",
            "<Normal/>",
        ) + &control(
            "[Channel2]",
            "volume",
            "0xB1",
            "7",
            "<Soft-takeover/>",
        ) + &control(
            "[Channel1]",
            "jog",
            "0xb0",
            "9",
            "<Script-Binding/>",
        ) + &control(
            "[Channel1]",
            "rate",
            "0xb0",
            "10",
            "<fourteen-bit-msb/>",
        ))))
        .unwrap();
        assert_eq!(m.bindings.len(), 2);
        assert_eq!(m.issues.len(), 2);
        assert_eq!(m.bindings[1].target, Target::Volume(1));
        assert_eq!(message_value(&m.bindings[0], &[0x90, 1, 127]), Some(1.));
        assert_eq!(message_value(&m.bindings[0], &[0x80, 1, 40]), Some(0.));
        assert_eq!(message_value(&m.bindings[0], &[0x91, 1, 127]), None);
        assert_eq!(message_value(&m.bindings[0], &[0x90, 2, 127]), None);
        assert_eq!(message_value(&m.bindings[0], &[0x90, 1]), None);
    }
    #[test]
    fn ambiguous_hid_and_external_entities_are_not_imported() {
        let c = control("[Channel1]", "play", "0x90", "1", "");
        assert!(parse(&xml(&(c.clone() + &c))).is_err());
        assert!(parse("<MixxxHIDPreset/>").is_err());
        assert!(parse("<!DOCTYPE x [<!ENTITY leak SYSTEM 'file:///private'>]><MixxxControllerPreset>&leak;</MixxxControllerPreset>").is_err());
        assert!(parse(&" ".repeat(MAX_XML + 1)).is_err());
    }
    #[test]
    fn pitch_bend_and_pickup_handle_resolution_inversion_and_gui_changes() {
        let m = parse(&xml(&control(
            "[Channel1]",
            "rate",
            "0xE2",
            "",
            "<Invert/>",
        )))
        .unwrap();
        assert_eq!(message_value(&m.bindings[0], &[0xe2, 0, 0]), Some(1.));
        assert_eq!(message_value(&m.bindings[0], &[0xe2, 127, 127]), Some(0.));
        let mut p = Pickup::default();
        assert!(!p.accept(0., 0.6));
        assert!(!p.accept(0.3, 0.6));
        assert!(p.accept(0.7, 0.6));
        assert!(p.accept(0.8, 0.7));
        assert!(!p.accept(0.9, 0.2));
        assert!(p.accept(0.1, 0.2));
    }
    #[test]
    fn midi_queue_dispatches_deck_b_fader_and_manual_takeover_without_metadata_writes() {
        let mut player = Player::default();
        let saved = serde_json::to_value(&player.run.settings).unwrap();
        player.run.auto = true;
        let m = parse(&xml(&control("[Channel2]", "volume", "0xB1", "7", ""))).unwrap();
        player.controllers.state = vec![Pickup::default(); m.bindings.len()];
        player.controllers.mapping = Some(m);
        let (tx, rx) = mpsc::sync_channel(5);
        player.controllers.receiver = Some(rx);
        tx.send([0xb1, 7, 127]).unwrap();
        tx.send([0xb1, 7, 64]).unwrap();
        player.poll_controllers();
        assert!((player.channel_gain[1] - 64. / 127.).abs() < 0.001);
        assert_eq!(player.channel_gain[0], 1.);
        assert!(!player.run.auto);
        assert_eq!(serde_json::to_value(&player.run.settings).unwrap(), saved);
        player.controllers.disconnect();
        assert!(player.controllers.receiver.is_none());
    }
}
