use eframe::egui;
use std::path::PathBuf;

pub fn source(response: &egui::Response, path: PathBuf) {
    response.dnd_set_drag_payload(path);
}
pub fn target(response: &egui::Response, accent: egui::Color32) -> Option<PathBuf> {
    let dragging = egui::DragAndDrop::has_payload_of_type::<PathBuf>(&response.ctx)
        || response
            .ctx
            .input(|input| !input.raw.hovered_files.is_empty());
    let hovered = response
        .ctx
        .layer_transform_to_global(response.layer_id)
        .unwrap_or_default()
        * response.rect;
    let hovered = response
        .ctx
        .pointer_hover_pos()
        .is_some_and(|position| hovered.contains(position));
    if dragging && hovered {
        response.ctx.layer_painter(response.layer_id).rect_stroke(
            response.rect.shrink(2.),
            4.,
            egui::Stroke::new(2., accent),
            egui::StrokeKind::Inside,
        );
    }
    response
        .dnd_release_payload::<PathBuf>()
        .map(|path| (*path).clone())
        .or_else(|| {
            if hovered {
                response.ctx.input(|input| {
                    input
                        .raw
                        .dropped_files
                        .first()
                        .map(|file| file.path().to_path_buf())
                })
            } else {
                None
            }
        })
}
