//! Library search and cursor navigation preserve the last successful page on failure.
use gpui_kit::Context;
use retold_app::library::{ListOptions, Video};

use super::{Shell, backend::Request};

const PAGE_SIZE: usize = 64;

/// How long typing must pause before the video list filters.
const SEARCH_PAUSE: std::time::Duration = std::time::Duration::from_millis(250);

impl Shell {
    pub(super) fn video_items(&self) -> &[Video] {
        self.catalog_page
            .as_ref()
            .map(|page| page.items.as_slice())
            .or_else(|| {
                self.snapshot
                    .as_ref()
                    .map(|snapshot| &snapshot.videos[..snapshot.videos.len().min(PAGE_SIZE)])
            })
            .unwrap_or_default()
    }

    pub(super) fn find_video(&self, reference: &str) -> Option<&Video> {
        self.video_items()
            .iter()
            .find(|video| video.reference == reference)
            .or_else(|| {
                self.snapshot
                    .as_ref()?
                    .videos
                    .iter()
                    .find(|video| video.reference == reference)
            })
    }

    /// Search once typing pauses for a moment, so each keystroke stays instant.
    pub(super) fn search_as_you_type(&mut self, cx: &mut Context<Self>) {
        self.search_generation += 1;
        let generation = self.search_generation;
        let pause = cx.background_executor().timer(SEARCH_PAUSE);
        cx.spawn(async move |this, cx| {
            pause.await;
            let _ = this.update(cx, |this, cx| {
                let latest = this.search_generation == generation;
                let applied = this.catalog_options.search.clone().unwrap_or_default();
                let typed = this.video_search(cx).unwrap_or_default();
                if latest && !this.busy && typed != applied {
                    this.search_videos(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn search_videos(&mut self, cx: &mut Context<Self>) {
        let mut options = self.catalog_options.clone();
        options.search = self.video_search(cx);
        options.cursor = None;
        self.load_video_page(options, Vec::new(), cx);
    }

    pub(super) fn sort_videos(&mut self, sort: &str, cx: &mut Context<Self>) {
        let mut options = self.catalog_options.clone();
        options.search = self.video_search(cx);
        options.cursor = None;
        options.sort = Some(sort.into());
        self.load_video_page(options, Vec::new(), cx);
    }

    pub(super) fn archived_videos(&mut self, archived: bool, cx: &mut Context<Self>) {
        let mut options = self.catalog_options.clone();
        options.search = self.video_search(cx);
        options.cursor = None;
        options.archive = archived;
        self.load_video_page(options, Vec::new(), cx);
    }

    fn video_search(&self, cx: &Context<Self>) -> Option<String> {
        let query = self.search.read(cx).value().trim().to_owned();
        (!query.is_empty()).then_some(query)
    }

    pub(super) fn next_video_page(&mut self, cx: &mut Context<Self>) {
        let Some(cursor) = self
            .catalog_page
            .as_ref()
            .and_then(|page| page.next_cursor.clone())
        else {
            return;
        };
        let mut history = self.catalog_back.clone();
        history.push(self.catalog_options.cursor.clone());
        let mut options = self.catalog_options.clone();
        options.cursor = Some(cursor);
        self.load_video_page(options, history, cx);
    }

    pub(super) fn previous_video_page(&mut self, cx: &mut Context<Self>) {
        let mut history = self.catalog_back.clone();
        let Some(cursor) = history.pop() else { return };
        let mut options = self.catalog_options.clone();
        options.cursor = cursor;
        self.load_video_page(options, history, cx);
    }

    pub(super) fn load_video_page(
        &mut self,
        mut options: ListOptions,
        history: Vec<Option<String>>,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        options.limit = Some(PAGE_SIZE);
        self.request(
            Request::Videos {
                library: snapshot.context.library.clone(),
                options,
                history,
            },
            cx,
        );
    }
}
