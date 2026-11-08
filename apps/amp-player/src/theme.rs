use eframe::egui::{self, Color32 as C};
use std::{fs, io, path::PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Classic,
    ModernLight,
    ModernDark,
}
impl Theme {
    pub const ALL: [Self; 3] = [Self::Classic, Self::ModernLight, Self::ModernDark];
    pub fn id(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::ModernLight => "modern-light",
            Self::ModernDark => "modern-dark",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::ModernLight => "Modern Light",
            Self::ModernDark => "Modern Dark",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == value.trim())
    }
}

pub struct Palette {
    pub background: C,
    pub surface: C,
    pub elevated: C,
    pub border: C,
    pub text: C,
    pub accent: C,
    pub signal: C,
    pub muted: C,
}
impl Theme {
    pub fn palette(self) -> Palette {
        match self {
            Self::Classic => Palette {
                background: C::from_rgb(7, 8, 12),
                surface: C::from_rgb(31, 32, 42),
                elevated: C::from_rgb(59, 60, 72),
                border: C::from_rgb(112, 107, 129),
                text: C::from_rgb(235, 235, 242),
                accent: C::from_rgb(255, 194, 26),
                signal: C::from_rgb(130, 245, 61),
                muted: C::from_rgb(164, 164, 182),
            },
            Self::ModernLight => Palette {
                background: C::from_rgb(241, 243, 247),
                surface: C::WHITE,
                elevated: C::from_rgb(230, 234, 240),
                border: C::from_rgb(171, 180, 193),
                text: C::from_rgb(25, 33, 46),
                accent: C::from_rgb(132, 85, 0),
                signal: C::from_rgb(29, 109, 48),
                muted: C::from_rgb(84, 96, 111),
            },
            Self::ModernDark => Palette {
                background: C::from_rgb(13, 16, 22),
                surface: C::from_rgb(24, 29, 38),
                elevated: C::from_rgb(34, 41, 53),
                border: C::from_rgb(70, 81, 99),
                text: C::from_rgb(231, 236, 243),
                accent: C::from_rgb(255, 197, 54),
                signal: C::from_rgb(130, 212, 155),
                muted: C::from_rgb(155, 168, 187),
            },
        }
    }
}

pub fn settings_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("AMP_VERSE_CONFIG_DIR").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path).join("theme.txt"));
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("AMPVerse/theme.txt"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
            .map(|p| p.join("amp-verse/theme.txt"))
    }
}
pub fn load() -> Theme {
    std::env::var("AMP_THEME")
        .ok()
        .and_then(|v| Theme::parse(&v))
        .or_else(|| {
            settings_path()
                .and_then(|p| fs::read_to_string(p).ok())
                .and_then(|v| Theme::parse(&v))
        })
        .unwrap_or_default()
}
pub fn save(path: &std::path::Path, theme: Theme) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Theme settings folder unavailable"))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".theme-{}.tmp", std::process::id()));
    let result = (|| {
        fs::write(&temp, theme.id())?;
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
pub fn current(ctx: &egui::Context) -> Theme {
    ctx.data_mut(|d| d.get_temp::<Theme>(egui::Id::unique("amp-theme")))
        .unwrap_or_default()
}
pub fn apply(ctx: &egui::Context, theme: Theme) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("amp-theme"), theme));
    let p = theme.palette();
    let light = theme == Theme::ModernLight;
    ctx.set_theme(if light {
        egui::ThemePreference::Light
    } else {
        egui::ThemePreference::Dark
    });
    let mut style = (*ctx.global_style()).clone();
    style.visuals = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    style.visuals.panel_fill = p.background;
    style.visuals.window_fill = p.surface;
    style.visuals.selection.bg_fill = if light {
        C::from_rgb(255, 226, 143)
    } else {
        C::from_rgb(103, 72, 9)
    };
    style.visuals.selection.stroke = egui::Stroke::new(1., p.accent);
    style.visuals.override_text_color = Some(p.text);
    style.visuals.weak_text_color = Some(p.muted);
    style.visuals.widgets.inactive.bg_fill = p.elevated;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1., p.border);
    style.visuals.widgets.hovered.bg_fill = if light {
        C::from_rgb(255, 235, 186)
    } else {
        p.elevated
    };
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1., p.accent);
    style.visuals.hyperlink_color = p.accent;
    style.spacing.button_padding = egui::vec2(10., 6.);
    style.spacing.item_spacing = egui::vec2(8., 7.);
    ctx.set_global_style(style);
    for id in [
        egui::ViewportId::ROOT,
        egui::ViewportId::from_hash_of("amp-library"),
        egui::ViewportId::from_hash_of("amp-dj"),
        egui::ViewportId::from_hash_of("amp-addons"),
        egui::ViewportId::from_hash_of("amp-cast"),
    ] {
        ctx.request_repaint_of(id);
    }
}
pub fn selector(ui: &mut egui::Ui) {
    let previous = current(ui.ctx());
    let mut selected = previous;
    egui::ComboBox::from_id_salt("appearance")
        .selected_text(selected.label())
        .show_ui(ui, |ui| {
            for theme in Theme::ALL {
                ui.selectable_value(&mut selected, theme, theme.label());
            }
        });
    if selected != previous {
        apply(ui.ctx(), selected);
        let result = settings_path()
            .ok_or_else(|| io::Error::other("Settings folder unavailable"))
            .and_then(|p| save(&p, selected));
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::unique("theme-save-error"),
                result.err().map(|e| e.to_string()).unwrap_or_default(),
            )
        });
    }
    if let Some(error) = ui
        .ctx()
        .data_mut(|d| d.get_temp::<String>(egui::Id::unique("theme-save-error")))
        && !error.is_empty()
    {
        ui.label(format!("Theme active; could not save: {error}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_replace_and_round_trip_all_themes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings/theme.txt");
        for theme in Theme::ALL {
            save(&path, theme).unwrap();
            assert_eq!(
                Theme::parse(&fs::read_to_string(&path).unwrap()),
                Some(theme)
            );
        }
        assert_eq!(Theme::parse("unknown"), None);
        assert_eq!(Theme::parse(" modern-light\n"), Some(Theme::ModernLight));
    }
}
