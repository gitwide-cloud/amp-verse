//! Explicit window merges, independent of geometric edge snapping.
use eframe::egui::{self, ViewportId};
pub const NAMES: [&str; 6] = [
    "AMP Verse",
    "AMP Library",
    "AMP DJ",
    "AMP Lab",
    "AMP Cast",
    "Plugin Manager",
];
pub fn viewport(pane: usize) -> ViewportId {
    if pane == 0 {
        ViewportId::ROOT
    } else {
        ViewportId::from_hash_of(
            [
                "",
                "amp-library",
                "amp-dj",
                "amp-lab-arrangement",
                "amp-cast",
                "amp-addons",
            ][pane],
        )
    }
}
pub struct Layout {
    pub parent: [Option<usize>; 6],
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            parent: [None, Some(0), None, None, None, None],
        }
    }
}
impl Layout {
    pub fn join(&mut self, child: usize, host: usize) -> bool {
        if child == 0 || child == host || child >= 6 || host >= 6 {
            return false;
        }
        let mut at = Some(host);
        while let Some(p) = at {
            if p == child {
                return false;
            }
            at = self.parent[p];
        }
        self.parent[child] = Some(host);
        true
    }
    pub fn target(&self, ctx: &egui::Context, pane: usize, open: [bool; 6]) -> Option<usize> {
        ctx.input(|i| {
            let a = i.raw.viewports.get(&viewport(pane))?.outer_rect?;
            (0..6)
                .filter(|&p| p != pane && open[p] && self.parent[p].is_none())
                .find(|&p| {
                    i.raw
                        .viewports
                        .get(&viewport(p))
                        .and_then(|v| v.outer_rect)
                        .is_some_and(|b| touching(a, b))
                })
        })
    }
}
fn touching(a: egui::Rect, b: egui::Rect) -> bool {
    let x = a.left() < b.right() - 8. && b.left() < a.right() - 8.;
    let y = a.top() < b.bottom() - 8. && b.top() < a.bottom() - 8.;
    (x && ((a.top() - b.bottom()).abs() <= 16. || (a.bottom() - b.top()).abs() <= 16.))
        || (y && ((a.left() - b.right()).abs() <= 16. || (a.right() - b.left()).abs() <= 16.))
}
pub fn embedded(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::unique("embedded-pane"))
            .unwrap_or(false)
    })
}
pub fn set_embedded(ctx: &egui::Context, value: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("embedded-pane"), value));
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merges_preserve_children_and_reject_cycles() {
        let mut l = Layout::default();
        assert!(l.join(2, 1));
        assert!(!l.join(1, 2));
        assert!(!l.join(0, 2));
        assert!(l.join(1, 4));
        assert_eq!(l.parent[2], Some(1));
        l.parent[1] = None;
        assert_eq!(l.parent[2], Some(1));
    }
    #[test]
    fn lights_only_for_neighboring_edges() {
        let a = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100., 100.));
        assert!(touching(a, a.translate(egui::vec2(100., 0.))));
        assert!(touching(a, a.translate(egui::vec2(0., 112.))));
        assert!(!touching(a, a.translate(egui::vec2(0., 117.))));
        assert!(!touching(a, a.translate(egui::vec2(100., 100.))));
    }
}
