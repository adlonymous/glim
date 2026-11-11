//! Linux PDF pages are drawn in GPUI; unported native formats open externally.
use crate::app::reader_jobs::{self, JobClass, Priority};
use gpui_kit::{
    component::{
        ActiveTheme, Disableable,
        button::{Button, ButtonVariants},
        h_flex, v_flex,
    },
    prelude::FluentBuilder,
    *,
};
use std::path::PathBuf;

pub(super) struct NativeReader {
    path: PathBuf,
    folder: bool,
    shuffle: bool,
    page: usize,
    pages: usize,
    image: Option<super::media::ImageReader>,
    task: Option<Task<()>>,
    error: Option<String>,
    html: Option<String>,
    browser_file: Option<glim_services::linux_pdf::ExternalHtmlPreview>,
}
impl NativeReader {
    fn empty(path: PathBuf) -> Self {
        Self {
            path,
            folder: false,
            shuffle: false,
            page: 1,
            pages: 0,
            image: None,
            task: None,
            error: None,
            html: None,
            browser_file: None,
        }
    }
    pub fn new(path: PathBuf, folder: bool, shuffle: bool, cx: &mut Context<Self>) -> Self {
        let mut reader = Self::empty(path);
        reader.folder = folder;
        reader.shuffle = shuffle;
        reader.open(cx);
        reader
    }
    pub fn new_paused(path: PathBuf, folder: bool, shuffle: bool, cx: &mut Context<Self>) -> Self {
        Self::new(path, folder, shuffle, cx)
    }
    pub fn html(
        path: PathBuf,
        text: Option<String>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Self {
        let mut reader = Self::empty(path);
        reader.html = text;
        reader
    }
    pub fn openapi(path: PathBuf, _: &[u8]) -> Self {
        Self::empty(path)
    }
    pub fn reload_html_file(&mut self, cx: &mut Context<Self>) {
        self.browser_file = None;
        cx.notify();
    }
    pub fn update_html(&mut self, text: String, cx: &mut Context<Self>) {
        self.html = Some(text);
        self.browser_file = None;
        cx.notify();
    }
    // HTML navigation happens in the external browser until a Linux WebView is available.
    pub fn set_html_open(&mut self, _: super::OpenLink) {}
    pub fn reveal_html_anchor(&mut self, _: &str) {}
    pub(super) fn hide(&mut self) {
        self.task = None;
    }
    pub(super) fn open(&mut self, cx: &mut Context<Self>) {
        if self.task.is_some() || self.image.is_some() || !self.is_pdf() {
            return;
        }
        self.error = None;
        let path = self.path.clone();
        let page = self.page;
        let (cancel, guard) = glim_services::work::Cancellation::task();
        let work = reader_jobs::run(
            cx.background_executor().clone(),
            JobClass::Native,
            Priority::Interactive,
            cancel,
            move |cancel| {
                glim_services::linux_pdf::render_pdf_page(
                    &path,
                    page,
                    glim_services::limits::IMAGE_PREVIEW_EDGE,
                    cancel,
                )
            },
        );
        self.task = Some(cx.spawn(async move |view, cx| {
            let _guard = guard;
            let result = work.await;
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(page) => {
                        view.pages = page.pages;
                        view.image = Some(super::media::ImageReader::new(vec![page.png], cx));
                    }
                    Err(error) => view.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn find(&self, _: bool) -> bool {
        false
    }
    pub(super) fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if active { self.open(cx) } else { self.hide() }
    }
    fn is_pdf(&self) -> bool {
        self.path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
    }
    fn change_page(&mut self, next: bool, cx: &mut Context<Self>) {
        self.page = if next {
            (self.page + 1).min(self.pages)
        } else {
            self.page.saturating_sub(1).max(1)
        };
        self.image = None;
        self.task = None;
        self.open(cx);
    }
    fn open_external(&mut self, cx: &mut Context<Self>) {
        let path = self.path.clone();
        let html = self.html.clone();
        self.task = Some(cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    glim_services::linux_pdf::open_external_preview(&path, html.as_deref())
                })
                .await;
            let _ = view.update(cx, |view, cx| {
                view.task = None;
                match result {
                    Ok(file) => view.browser_file = file,
                    Err(error) => view.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        }));
    }
}
impl Render for NativeReader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().relative().size_full();
        if let Some(image) = &self.image {
            root = root
                .child(image.render(cx))
                .child(super::controls::floating(
                    "linux-pdf-controls",
                    super::controls::ActionBindings::new()
                        .action(
                            "previous",
                            crate::views::icon_button(
                                "pdf-previous",
                                gpui_kit::assets::IconName::ChevronLeft,
                                glim_services::locale::tr("Previous page"),
                            )
                            .disabled(self.page <= 1)
                            .on_click(cx.listener(|view, _, _, cx| view.change_page(false, cx))),
                        )
                        .action("page-status", format!("{} / {}", self.page, self.pages))
                        .action(
                            "next",
                            crate::views::icon_button(
                                "pdf-next",
                                gpui_kit::assets::IconName::ChevronRight,
                                glim_services::locale::tr("Next page"),
                            )
                            .disabled(self.page >= self.pages)
                            .on_click(cx.listener(|view, _, _, cx| view.change_page(true, cx))),
                        ),
                    cx,
                ));
        } else {
            root = root.child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .p_6()
                    .child(
                        div().text_sm().child(
                            self.path
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned(),
                        ),
                    )
                    .when(self.task.is_some(), |view| {
                        view.child(glim_services::locale::tr("Loading preview…"))
                    })
                    .when(
                        self.task.is_none() && self.error.is_none() && !self.is_pdf(),
                        |view| {
                            view.child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(glim_services::locale::tr(
                                        "This format uses an external application on Linux.",
                                    )),
                            )
                        },
                    )
                    .when_some(self.error.clone(), |view, error| {
                        view.child(div().text_sm().text_color(cx.theme().danger).child(error))
                    })
                    .child(
                        h_flex()
                            .gap_2()
                            .when(self.error.is_some() && self.is_pdf(), |row| {
                                row.child(
                                    Button::new("retry-linux-pdf")
                                        .label(glim_services::locale::tr("Retry"))
                                        .on_click(cx.listener(|view, _, _, cx| view.open(cx))),
                                )
                            })
                            .child(
                                Button::new("linux-open-external")
                                    .primary()
                                    .label(glim_services::locale::tr("Open in Default App"))
                                    .disabled(self.task.is_some())
                                    .on_click(cx.listener(|view, _, _, cx| view.open_external(cx))),
                            ),
                    ),
            );
        }
        root
    }
}
impl super::contract::ReaderView for super::surface::SurfaceEntity<NativeReader> {
    fn kind(&self) -> &'static str {
        "native"
    }
    fn render(&mut self, _: &mut App) -> AnyElement {
        self.0.clone().into_any_element()
    }
    fn set_active(&self, active: bool, cx: &mut App) {
        self.update(
            cx,
            |view, cx| if active { view.open(cx) } else { view.hide() },
        );
    }
    fn operations(&self) -> super::contract::ReaderOperations<'_> {
        super::contract::ReaderOperations {
            playback: Some(self),
            ..Default::default()
        }
    }
}
impl super::surface::ReaderSurface for super::surface::SurfaceEntity<NativeReader> {}
impl super::contract::PlaybackState for super::surface::SurfaceEntity<NativeReader> {
    fn playlist_shuffle(&self, cx: &App) -> Option<bool> {
        let view = self.read(cx);
        view.folder.then_some(view.shuffle)
    }
}
