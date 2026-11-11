//! Linux context menus use GPUI Kit's drawn menu backend, not AppKit.
use gpui_kit::*;
use std::{cell::Cell, rc::Rc};

#[derive(Clone, PartialEq, Action)]
#[action(namespace = glim_linux_menu, no_json)]
struct ChooseMenuItem {
    menu: u64,
    index: usize,
}

#[derive(Default)]
struct MenuSelection {
    next: u64,
    active: Option<(u64, Rc<Cell<Option<usize>>>)>,
}
impl Global for MenuSelection {}

pub(super) struct NativeMenu {
    selected: Rc<Cell<Option<usize>>>,
}
impl NativeMenu {
    pub fn new(
        items: &[(Option<String>, bool)],
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Self> {
        if !cx.has_global::<MenuSelection>() {
            cx.set_global(MenuSelection::default());
            cx.on_action(|action: &ChooseMenuItem, cx| {
                if let Some((id, selected)) = &cx.global::<MenuSelection>().active
                    && *id == action.menu
                {
                    selected.set(Some(action.index));
                }
            });
        }
        let selected = Rc::new(Cell::new(None));
        let registry = cx.global_mut::<MenuSelection>();
        registry.next += 1;
        let id = registry.next;
        registry.active = Some((id, selected.clone()));
        let mut menu = component::native_menu::NativeMenu::new();
        for (index, (title, enabled)) in items.iter().enumerate() {
            menu = match title {
                Some(title) => menu.menu_with_disabled(
                    title.clone(),
                    !enabled,
                    Box::new(ChooseMenuItem { menu: id, index }),
                ),
                None => menu.separator(),
            };
        }
        menu.show(window.mouse_position(), window, cx);
        Some(Self { selected })
    }

    pub async fn choose(self, cx: &mut AsyncWindowContext) -> Option<usize> {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
            if let Some(index) = self.selected.get() {
                return Some(index);
            }
            let showing = cx
                .update(|window, _| {
                    window
                        .context_stack()
                        .iter()
                        .any(|context| context.contains("PopupMenu"))
                })
                .unwrap_or(false);
            if !showing {
                return None;
            }
        }
    }
}
