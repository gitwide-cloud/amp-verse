//! Device-local musical presets; recall never changes the crossfade pair or background layers.
use super::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Scene {
    id: String,
    name: String,
    playlist: Option<String>,
    style: amp_core::show::Style,
    energy: Option<f64>,
    fade: f32,
    bpm: f32,
    quantization: u32,
}
fn file() -> Result<PathBuf, String> {
    theme::settings_path()
        .and_then(|p| p.parent().map(|p| p.join("ampdj/scenes.ampdb")))
        .ok_or("Settings directory unavailable".into())
}
fn load() -> Result<Vec<Scene>, String> {
    Ok(
        amp_core::catalog_db::local_record(&file()?, "music-scenes")?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| format!("Saved music scenes are invalid: {e}"))?
            .unwrap_or_default(),
    )
}
impl Player {
    pub(super) fn scene_resources(&self) -> serde_json::Value {
        match load() {
            Ok(scenes) => {
                serde_json::json!({"scenes":scenes.iter().map(|s|serde_json::json!({"id":s.id,"name":s.name})).collect::<Vec<_>>()})
            }
            Err(e) => serde_json::json!({"scenes":[],"error":e}),
        }
    }
    pub(super) fn save_music_scene(&self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name.len() > 120 || name.chars().any(char::is_control) {
            return Err("Choose a scene name (1–120 characters)".into());
        }
        let mut scenes = load()?;
        if scenes.iter().any(|s| s.name.eq_ignore_ascii_case(name)) {
            return Err("A scene already uses that name; choose a new name".into());
        }
        if scenes.len() >= 256 {
            return Err("Music scene limit reached".into());
        }
        let audio = self.audio.as_ref();
        use std::sync::atomic::Ordering::Relaxed;
        scenes.push(Scene {
            id: amp_core::coordination::token(),
            name: name.into(),
            playlist: self.run.playlist.clone(),
            style: self.run.style,
            energy: self.run.target,
            fade: self.fade_seconds,
            bpm: audio.map_or(120., |a| {
                f32::from_bits(a.controls.master_bpm.load(Relaxed))
            }),
            quantization: audio.map_or(4, |a| a.controls.quantization.load(Relaxed)),
        });
        amp_core::catalog_db::save_local_record(
            &file()?,
            "music-scenes",
            &serde_json::to_value(scenes).map_err(|e| e.to_string())?,
        )
    }
    pub(super) fn recall_music_scene(
        &mut self,
        id: &str,
        execution: control::Execution,
    ) -> Result<(), String> {
        let scene = load()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or("Saved music scene unavailable; refresh the picker")?;
        if !scene.fade.is_finite()
            || !(0.5..=30.).contains(&scene.fade)
            || !scene.bpm.is_finite()
            || !(40. ..=240.).contains(&scene.bpm)
            || ![0, 1, 4, 8, 16, 32].contains(&scene.quantization)
            || scene
                .energy
                .is_some_and(|e| !e.is_finite() || !(1. ..=10.).contains(&e))
        {
            return Err("Saved scene has invalid musical settings".into());
        }
        if let Some(playlist) = scene.playlist {
            self.execute_control(control::Action::LoadSmartPlaylist {
                playlist,
                execution,
                style: Some(scene.style),
                energy: scene.energy,
            })?;
        } else {
            self.run.style = scene.style;
            self.run.target = scene.energy;
        }
        self.ensure_audio()?;
        self.audio
            .as_ref()
            .unwrap()
            .set_clock(scene.bpm, scene.quantization)?;
        self.fade_seconds = scene.fade;
        crossfade_settings::save(scene.fade).map_err(|e| e.to_string())?;
        self.status = format!("Music scene: {}", scene.name);
        Ok(())
    }
}
