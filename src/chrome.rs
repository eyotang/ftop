//! Application-owned floating-panel chrome, without Kit's rectangular window border.
use gpui_kit::{base::Button, prelude::*, *};

pub const RADIUS: f32 = 18.;
#[cfg(target_os = "linux")]
pub const INSET: f32 = 20.;
#[cfg(not(target_os = "linux"))]
pub const INSET: f32 = 0.;

pub trait PanelButton: Sized {
    fn label(self, label: impl Into<SharedString>) -> Self;
    fn small(self) -> Self;
    fn ghost(self) -> Self {
        self
    }
}
impl PanelButton for Button {
    fn label(self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        self.accessibility_label(label.clone()).child(label)
    }
    fn small(self) -> Self {
        self.px(px(8.))
            .py(px(4.))
            .rounded(px(8.))
            .text_size(px(12.))
            .cursor_pointer()
            .hover(|style| style.bg(rgba(0x88889922)))
            .border_1()
            .border_color(transparent_black())
            .focus_visible(|style| style.border_color(rgb(0x5b9bff)))
            .styles(|styles| styles.selected(|style| style.bg(rgba(0x5b9bff30))))
    }
}
struct TooltipText(String);
impl Render for TooltipText {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.))
            .py(px(6.))
            .rounded(px(8.))
            .bg(rgb(0xe5e7ed))
            .text_color(rgb(0x171923))
            .text_size(px(11.))
            .child(self.0.clone())
    }
}
pub fn tooltip(text: String, cx: &mut App) -> AnyView {
    cx.new(|_| TooltipText(text)).into()
}
fn resize_edge(position: Point<Pixels>, size: Size<Pixels>) -> Option<ResizeEdge> {
    let x = f32::from(position.x);
    let y = f32::from(position.y);
    let right = f32::from(size.width) - INSET;
    let bottom = f32::from(size.height) - INSET;
    let left = (x - INSET).abs() < 5.;
    let top = (y - INSET).abs() < 5.;
    let r = (x - right).abs() < 5.;
    let b = (y - bottom).abs() < 5.;
    match (left, r, top, b) {
        (true, _, true, _) => Some(ResizeEdge::TopLeft),
        (_, true, true, _) => Some(ResizeEdge::TopRight),
        (true, _, _, true) => Some(ResizeEdge::BottomLeft),
        (_, true, _, true) => Some(ResizeEdge::BottomRight),
        (true, _, _, _) => Some(ResizeEdge::Left),
        (_, true, _, _) => Some(ResizeEdge::Right),
        (_, _, true, _) => Some(ResizeEdge::Top),
        (_, _, _, true) => Some(ResizeEdge::Bottom),
        _ => None,
    }
}
pub fn frame(content: impl IntoElement, window: &mut Window) -> Div {
    window.set_client_inset(px(INSET));
    div()
        .size_full()
        .p(px(INSET))
        .on_mouse_down(MouseButton::Left, |event, window, _| {
            if let Some(edge) = resize_edge(event.position, window.viewport_size()) {
                window.start_window_resize(edge);
            }
        })
        .child(
            div()
                .size_full()
                .rounded(px(RADIUS))
                .border_1()
                .border_color(rgba(0x9fa6b538))
                .shadow(vec![BoxShadow {
                    color: rgba(0x00000045).into(),
                    offset: point(px(0.), px(3.)),
                    blur_radius: px(16.),
                    spread_radius: px(-2.),
                    inset: false,
                }])
                .child(content),
        )
}

#[cfg(target_os = "macos")]
pub const FONT: &str = ".SystemUIFont";
#[cfg(target_os = "windows")]
pub const FONT: &str = "Segoe UI";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const FONT: &str = "Ubuntu Sans";

#[cfg(test)]
mod tests {
    use super::{INSET, resize_edge};
    use gpui_kit::{ResizeEdge, point, px, size};
    #[test]
    fn resize_corners_and_center_do_not_steal_content_clicks() {
        let size = size(px(800.), px(500.));
        assert!(matches!(
            resize_edge(point(px(INSET), px(INSET)), size),
            Some(ResizeEdge::TopLeft)
        ));
        assert!(matches!(
            resize_edge(point(px(800. - INSET), px(500. - INSET)), size),
            Some(ResizeEdge::BottomRight)
        ));
        assert!(resize_edge(point(px(100.), px(100.)), size).is_none());
    }
}
