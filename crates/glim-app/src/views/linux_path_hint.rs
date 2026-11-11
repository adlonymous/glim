//! File path hints rendered through GPUI's tooltip layer on Linux.
use gpui_kit::*;
use std::rc::Rc;

#[derive(Default)]
pub(super) struct PathHint {
    target: Option<(String, String, Bounds<Pixels>)>,
}
impl PathHint {
    pub fn hide_for(&mut self, path: &str, bounds: Bounds<Pixels>) {
        if self
            .target
            .as_ref()
            .is_some_and(|(_, current, anchor)| current == path && *anchor == bounds)
        {
            self.target = None;
        }
    }
    pub fn show(&mut self, title: &str, path: &str, _: bool, bounds: Bounds<Pixels>, _: &Window) {
        self.target = Some((title.to_owned(), path.to_owned(), bounds));
    }
    pub fn render(&self) -> Option<AnyElement> {
        let (title, path, bounds) = self.target.clone()?;
        let label = if path.is_empty() {
            title
        } else {
            format!("{title}\n{path}")
        };
        Some(
            canvas(
                move |_, window, cx| {
                    let view = component::tooltip::Tooltip::new(label.clone()).build(window, cx);
                    window.set_tooltip(AnyTooltip {
                        view,
                        mouse_position: window.mouse_position(),
                        check_visible_and_update: Rc::new(move |_, window, _| {
                            bounds.contains(&window.mouse_position())
                        }),
                    });
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full()
            .into_any_element(),
        )
    }
}
