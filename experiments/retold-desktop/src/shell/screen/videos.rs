use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, Textarea},
};
use gpui_kit::{Context, Div, FontWeight, Styled, div, prelude::*, px};
use retold_app::library::Video;

use super::{empty, empty_state, heading, heading_small, muted, page_header, segmented};
use crate::shell::Shell;
use gpui_kit::assets::IconName;

impl Shell {
    pub(super) fn videos(&self, cx: &mut Context<Self>) -> Div {
        if self.draft.is_some() {
            return self.draft_form(cx);
        }
        let p = self.palette();
        if self.show_notes
            && self
                .notes
                .as_ref()
                .is_some_and(|panel| self.selected_video.as_deref() == Some(&panel.reference))
        {
            return self.note_page(cx);
        }
        if self.saved_media.is_some() {
            return self.media_page(cx);
        }
        if self.characters.is_some() {
            return self.character_page(cx);
        }
        if self.delivery.is_some() {
            return self.delivery_page(cx);
        }
        if self
            .materials
            .as_ref()
            .is_some_and(|panel| self.selected_video.as_deref() == Some(&panel.reference))
        {
            return self.materials_page(cx);
        }
        if self
            .narration
            .as_ref()
            .is_some_and(|panel| self.selected_video.as_deref() == Some(&panel.reference))
        {
            return self.narration_page(cx);
        }
        if self
            .lines
            .as_ref()
            .is_some_and(|panel| self.selected_video.as_deref() == Some(&panel.reference))
        {
            return self.lines_page(cx);
        }
        if self.show_scenes
            && self
                .scenes
                .as_ref()
                .is_some_and(|editor| self.selected_video.as_deref() == Some(&editor.reference))
        {
            return self.scene_page(cx);
        }
        if self.show_script
            && self
                .editor
                .as_ref()
                .is_some_and(|editor| self.selected_video.as_deref() == Some(&editor.reference))
        {
            return self.script_page(cx);
        }
        let Some(_) = &self.snapshot else {
            return empty("Videos", "Open a workspace to browse saved videos.", p);
        };
        if let Some(reference) = &self.selected_video
            && let Some(video) = self.find_video(reference)
        {
            if self.show_script {
                return self.script_page(cx);
            }
            return self.overview(video, cx);
        }
        let videos: Vec<_> = self
            .video_items()
            .iter()
            .filter(|video| video.is_archived == self.catalog_options.archive)
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(page_header(
                "Videos",
                "Every story recap in this workspace.",
                [self.new_video_button(cx).into_any_element()],
                p,
            ))
            .child(self.video_filters(cx))
            .children(videos.iter().enumerate().map(|(index, video)| {
                let row = self.video_row(index, video, cx);
                super::rise_in(
                    gpui_kit::ElementId::Name(
                        format!("desktop-video-rise-{}", video.reference).into(),
                    ),
                    index,
                    row,
                    cx,
                )
            }))
            .when(videos.is_empty(), |body| body.child(self.videos_empty(p)))
            .child(self.video_pagination(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.setup_summary(cx))
                    .child(self.catalog_issues())
                    .when(
                        self.snapshot
                            .as_ref()
                            .is_some_and(|snapshot| snapshot.total_videos > 0),
                        |footer| footer.child(self.saved_views(cx)),
                    )
                    .child(self.bulk_actions(cx)),
            )
    }

    fn video_filters(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div().flex().items_center().gap_2().child(
                    Input::new(&self.search)
                        .prefix(
                            gpui_kit::component::Icon::new(gpui_kit::assets::IconName::Search)
                                .text_color(p.secondary),
                        )
                        .aria_label("Find a video")
                        .cleanable(true)
                        .w_full(),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .child(segmented(
                        [
                            ("created", "Newest"),
                            ("title", "Title"),
                            ("channel", "Channel"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(index, (sort, label))| {
                            Button::new(("desktop-video-sort", index))
                                .self_start()
                                .label(label)
                                .selected(
                                    self.catalog_options.sort.as_deref().unwrap_or("created")
                                        == sort,
                                )
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.sort_videos(sort, cx)),
                                )
                        }),
                        p,
                    ))
                    .child(segmented(
                        [
                            Button::new("desktop-video-active")
                                .self_start()
                                .label("Active")
                                .selected(!self.catalog_options.archive)
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.archived_videos(false, cx)),
                                ),
                            Button::new("desktop-video-archived")
                                .self_start()
                                .label("Archived")
                                .selected(self.catalog_options.archive)
                                .disabled(self.busy)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.archived_videos(true, cx)),
                                ),
                        ],
                        p,
                    )),
            )
    }

    fn video_pagination(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let empty = self
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.total_videos == 0);
        if empty {
            return div();
        }
        let several_pages = !self.catalog_back.is_empty()
            || match &self.catalog_page {
                Some(page) => page.next_cursor.is_some(),
                None => self
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| snapshot.total_videos > snapshot.videos.len()),
            };
        let summary = self.catalog_page.as_ref().map_or_else(
            || "".into(),
            |page| {
                if page.total == 1 {
                    "1 video".to_owned()
                } else {
                    format!(
                        "Page {} · {} of {} videos",
                        self.catalog_back.len() + 1,
                        page.items.len(),
                        page.total
                    )
                }
            },
        );
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(muted(&summary, p))
            .when(several_pages, |body| {
                body.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("desktop-video-first")
                                .self_start()
                                .label(if self.catalog_page.is_some() {
                                    "Clear search"
                                } else {
                                    "Browse all saved videos"
                                })
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| this.search_videos(cx))),
                        )
                        .child(
                            Button::new("desktop-video-previous")
                                .self_start()
                                .label("Previous page")
                                .disabled(self.busy || self.catalog_back.is_empty())
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.previous_video_page(cx)),
                                ),
                        )
                        .child(
                            Button::new("desktop-video-next")
                                .self_start()
                                .label("Next page")
                                .disabled(
                                    self.busy
                                        || self
                                            .catalog_page
                                            .as_ref()
                                            .is_none_or(|page| page.next_cursor.is_none()),
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.next_video_page(cx))),
                        ),
                )
            })
    }

    pub(super) fn new_video_button(&self, cx: &mut Context<Self>) -> Button {
        let available = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.choices.as_ref().ok())
            .is_some_and(|choices| !choices.guides.is_empty() && !choices.channels.is_empty());
        let writer_missing = self
            .providers
            .snapshot
            .as_ref()
            .is_some_and(|providers| providers.routing.is_empty());
        Button::new("desktop-new-video")
            .self_start()
            .when(!writer_missing, |button| button.primary())
            .icon(gpui_kit::assets::IconName::Plus)
            .tooltip("New video (⌘N)")
            .label(if self.draft.is_some() {
                "Return to draft"
            } else {
                "New video"
            })
            .disabled(
                (!available && self.draft.is_none())
                    || self.busy
                    || self.import.is_some()
                    || self.new_root.is_some()
                    || self.editor.as_ref().is_some_and(|editor| editor.dirty(cx)),
            )
            .on_click(cx.listener(|this, _, window, cx| this.begin_draft(window, cx)))
    }

    pub(super) fn video_row(&self, index: usize, video: &Video, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let reference = video.reference.clone();
        div()
            .rounded_lg()
            .bg(p.surface)
            .p_4()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(video.title.clone()),
                    )
                    .child(muted(
                        &format!(
                            "{} · {}",
                            video.channel,
                            video
                                .editions
                                .iter()
                                .map(|edition| retold_app::desktop::edition_label(edition))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        p,
                    )),
            )
            .child({
                let chosen = video.reference.clone();
                let selected = self.selection.contains(&video.reference);
                Button::new(("desktop-video-select", index))
                    .self_start()
                    .label(if selected { "Selected" } else { "Select" })
                    .selected(selected)
                    .accessibility_label(format!("Select {}", video.title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.selection.remove(&chosen) {
                            this.selection.insert(chosen.clone());
                        }
                        cx.notify();
                    }))
            })
            .child(
                Button::new(("desktop-video", index))
                    .self_start()
                    .label("Open")
                    .accessibility_label(format!("Open {}", video.title))
                    .on_click(cx.listener(move |this, _, _, cx| this.select_video(&reference, cx))),
            )
    }

    fn overview(&self, video: &Video, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                Button::new("desktop-all-videos")
                    .self_start()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .ml(gpui_kit::px(-10.))
                    .label("All videos")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.selected_video = None;
                        cx.notify();
                    })),
            )
            .child(heading(&video.title))
            .child(muted(
                &format!(
                    "{} · {} · {} · Created {}",
                    video.reference,
                    video.guide,
                    video
                        .editions
                        .iter()
                        .map(|edition| retold_app::desktop::edition_label(edition))
                        .collect::<Vec<_>>()
                        .join(", "),
                    super::when(&video.created)
                ),
                p,
            ))
            .child(heading_small("Work on this video"))
            .child(self.tool_grid(cx))
            .child(heading_small("Video spending limit"))
            .child(muted(
                "Every run on this video together stays under this. Leave it unset to rely on the account limit.",
                p,
            ))
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Limit (USD)"))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        div().w(px(200.)).child(
                            gpui_kit::component::input::Input::new(&self.video_budget)
                                .aria_label("Video budget in USD"),
                        ),
                    )
                    .child(
                        Button::new("desktop-video-budget-set")
                            .label("Set limit")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.set_video_budget(cx))),
                    ),
            )
            .child(self.run_review(cx))
            .child(self.video_lifecycle(video, cx))
    }

    fn guide_field(
        &self,
        field: &retold_app::desktop::DraftField,
        state: &gpui_kit::Entity<gpui_kit::component::input::TextareaState>,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.palette();
        let value = state.read(cx).value();
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .max_w(px(480.))
            .child(div().font_weight(gpui_kit::FontWeight::MEDIUM).child(
                if field.required || field.prompt.contains("optional") {
                    field.prompt.clone()
                } else {
                    format!("{} (optional)", field.prompt)
                },
            ))
            .when(field.choices.is_empty(), |row| {
                row.child(
                    Textarea::new(state)
                        .aria_label(field.prompt.clone())
                        .disabled(self.busy)
                        .h(px(34.)),
                )
            })
            .when(!field.choices.is_empty(), |row| {
                row.child(div().flex().child(segmented(
                    field.choices.iter().enumerate().map(|(index, choice)| {
                        let state = state.clone();
                        let choice = choice.clone();
                        Button::new(gpui_kit::SharedString::from(format!(
                            "desktop-choice-{}-{index}",
                            field.name
                        )))
                        .self_start()
                        .label(super::title_case(&choice))
                        .accessibility_label(format!("{}: {choice}", field.prompt))
                        .selected(value.as_ref() == choice)
                        .disabled(self.busy)
                        .on_click(cx.listener(
                            move |_, _, window, cx| {
                                state.update(cx, |state, cx| {
                                    state.set_value(choice.clone(), window, cx)
                                });
                                cx.notify();
                            },
                        ))
                    }),
                    p,
                )))
            })
            .when_some(field.help.as_ref(), |row, help| row.child(muted(help, p)))
    }

    fn draft_form(&self, cx: &mut Context<Self>) -> Div {
        let Some(form) = &self.draft else {
            return div();
        };
        let p = self.palette();
        let Some(guide) = form.choices.guides.get(form.guide) else {
            return div();
        };
        div()
            .max_w(px(860.))
            .flex()
            .flex_col()
            .gap_4()
            .child(heading("New video"))
            .child(muted(
                "Set up the story. Nothing is generated until you start a run.",
                p,
            ))
            .child(heading_small("Guide"))
            .child(
                div().flex().flex_wrap().gap_2().children(
                    form.choices
                        .guides
                        .iter()
                        .enumerate()
                        .map(|(index, guide)| {
                            Button::new(("desktop-guide", index))
                                .self_start()
                                .label(guide.name.clone())
                                .selected(index == form.guide)
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if let Some(form) = &mut this.draft {
                                        form.select_guide(index, window, cx);
                                    }
                                    cx.notify();
                                }))
                        }),
                ),
            )
            .child(muted(&guide.summary, p))
            .child(heading_small("Channel"))
            .child(
                div().flex().flex_wrap().gap_2().children(
                    form.choices
                        .channels
                        .iter()
                        .enumerate()
                        .map(|(index, channel)| {
                            Button::new(("desktop-channel", index))
                                .self_start()
                                .label(channel.clone())
                                .selected(index == form.channel)
                                .disabled(self.busy)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(form) = &mut this.draft {
                                        form.channel = index;
                                    }
                                    cx.notify();
                                }))
                        }),
                ),
            )
            .children(
                form.fields
                    .iter()
                    .map(|(field, state)| self.guide_field(field, state, cx)),
            )
            .child(self.draft_source(cx))
            .child(heading_small("Narration editions"))
            .child(div().flex().flex_wrap().gap_2().children(
                guide.languages.iter().enumerate().map(|(index, language)| {
                    let language = language.clone();
                    Button::new(("desktop-language", index))
                        .self_start()
                        .label(language.clone())
                        .selected(form.languages.contains(&language))
                        .disabled(self.busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(form) = &mut this.draft {
                                if form.languages.contains(&language) {
                                    form.languages.retain(|selected| selected != &language);
                                } else {
                                    form.languages.push(language.clone());
                                }
                            }
                            cx.notify();
                        }))
                }),
            ))
            .child(heading_small("Video safety cap (USD)"))
            .child(
                Input::new(&form.budget)
                    .aria_label("Video safety cap USD")
                    .disabled(self.busy)
                    .w(px(180.)),
            )
            .child(muted(
                "This is the most the video may ever spend. Each run still asks before spending.",
                p,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("desktop-save-video")
                            .self_start()
                            .primary()
                            .label("Save offline draft")
                            .disabled(
                                self.busy || form.languages.is_empty() || !form.source_ready(cx),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.save_draft(cx))),
                    )
                    .child(
                        Button::new("desktop-cancel-video")
                            .self_start()
                            .label("Discard draft")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.draft = None;
                                cx.notify();
                            })),
                    ),
            )
    }
}

impl Shell {
    fn videos_empty(&self, p: crate::theme::Palette) -> Div {
        if self.catalog_options.search.is_some() {
            return empty_state(
                IconName::Search,
                "No matching videos",
                "Try a different search, or clear it to see every video.",
                p,
            );
        }
        if self.catalog_options.archive {
            return empty_state(
                IconName::Film,
                "No archived videos",
                "Videos you archive are kept here, out of the way.",
                p,
            );
        }
        empty_state(
            IconName::Film,
            "No videos yet",
            "Each video turns one story into a narrated recap. Press ⌘N to start one.",
            p,
        )
    }
}

/// One destination on the video page.
#[derive(Clone, Copy)]
enum Tool {
    Script,
    Scenes,
    Characters,
    Narration,
    Lines,
    Materials,
    Media,
    Notes,
    Export,
}

/// (id, icon, title, what it is for).
const TOOLS: [(Tool, &str, gpui_kit::assets::IconName, &str, &str); 9] = {
    use gpui_kit::assets::IconName;
    [
        (
            Tool::Script,
            "desktop-open-script",
            IconName::Clapperboard,
            "Script",
            "Read, edit and approve the narration",
        ),
        (
            Tool::Scenes,
            "desktop-open-scenes",
            IconName::Film,
            "Scenes",
            "Direct each shot and its frame",
        ),
        (
            Tool::Characters,
            "desktop-open-characters",
            IconName::Sparkles,
            "Characters",
            "Approve each character's look",
        ),
        (
            Tool::Narration,
            "desktop-open-narration",
            IconName::AudioWaveform,
            "Narration",
            "Listen to the recorded voice",
        ),
        (
            Tool::Lines,
            "desktop-open-lines",
            IconName::Layers,
            "Language versions",
            "Edit what each language says",
        ),
        (
            Tool::Materials,
            "desktop-open-materials",
            IconName::Package,
            "Materials",
            "Every file and where it came from",
        ),
        (
            Tool::Media,
            "desktop-open-media",
            IconName::MonitorPlay,
            "Saved media",
            "Watch finished renders",
        ),
        (
            Tool::Notes,
            "desktop-open-notes",
            IconName::MessageSquare,
            "Notes",
            "Review comments on this video",
        ),
        (
            Tool::Export,
            "desktop-open-export",
            IconName::FolderOpen,
            "Export",
            "Render or package for upload",
        ),
    ]
};

impl Shell {
    /// The video's destinations as a grid of tiles: icon, title and what each is for.
    fn tool_grid(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let busy = self.busy;
        let hover = p.text.opacity(0.05);
        div()
            .flex()
            .flex_wrap()
            .gap_3()
            .children(TOOLS.into_iter().enumerate().map(
                |(index, (tool, id, icon, title, caption))| {
                    let tile = div()
                        .id(id)
                        .test_support()
                        .w(gpui_kit::relative(0.31))
                        .min_w(px(200.))
                        .flex_grow(1.)
                        .flex()
                        .items_start()
                        .gap_3()
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(p.border)
                        .bg(p.surface)
                        .cursor_pointer()
                        .when(!busy, |tile| tile.hover(move |style| style.bg(hover)))
                        .when(busy, |tile| tile.opacity(0.5))
                        .child(
                            gpui_kit::component::Icon::new(icon)
                                .size(gpui_kit::rems(1.3))
                                .text_color(p.accent),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(div().font_weight(FontWeight::MEDIUM).child(title))
                                .child(div().text_sm().text_color(p.secondary).child(caption)),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.busy {
                                return;
                            }
                            match tool {
                                Tool::Script => this.open_script(window, cx),
                                Tool::Scenes => this.begin_scenes(window, cx),
                                Tool::Characters => this.begin_characters(cx),
                                Tool::Narration => this.begin_narration(cx),
                                Tool::Lines => this.begin_lines(window, cx),
                                Tool::Materials => this.begin_materials(cx),
                                Tool::Media => this.begin_media(cx),
                                Tool::Notes => this.begin_notes(window, cx),
                                Tool::Export => this.begin_delivery(window, cx),
                            }
                        }));
                    super::rise_in(
                        gpui_kit::ElementId::Name(format!("{id}-rise").into()),
                        index,
                        tile,
                        cx,
                    )
                },
            ))
    }
}
