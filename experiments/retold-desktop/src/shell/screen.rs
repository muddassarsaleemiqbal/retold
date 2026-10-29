//! Pure native layout: a quiet workspace with explicit empty, partial and error states.
mod account_admin;
mod activity;
mod admin;
mod authoring;
mod backups;
mod budget;
mod characters;
mod delivery;
mod delivery_presets;
mod destinations;
mod forms;
mod job;
mod lifecycle;
mod lines;
mod materials;
mod narration;
mod notes;
mod onboarding;
mod preferences;
mod providers;
mod readiness;
mod recovery;
mod retake;
mod run;
mod saved_media;
mod scene_cast;
mod scenes;
mod script;
mod series;
mod setup;
mod source;
mod videos;
mod views;

use gpui_kit::component::{
    Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{
    Context, Div, FontWeight, IntoElement, Render, Role, Styled, Window, div, img, prelude::*, px,
    rems,
};

use super::{Destination, Shell};
use crate::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette();
        let compact = super::navigation::compact(window);
        self.compact_layout = compact;
        MOTION.store(!cx.reduce_motion(), std::sync::atomic::Ordering::Relaxed);
        self.forget_stale_status();
        self.track_status(cx);
        div()
            .size_full()
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .bg(p.canvas)
            .text_color(p.text)
            .text_size(rems(0.875))
            .on_action(
                cx.listener(|this, _: &super::commands::SearchCommands, window, cx| {
                    this.search_commands(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &super::commands::OpenWorkspace, _, cx| {
                    this.choose_workspace(cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &super::commands::CreateVideo, window, cx| {
                    this.begin_draft(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &super::commands::RefreshWorkspace, _, cx| this.refresh(cx)),
            )
            .on_action(
                cx.listener(|this, _: &super::commands::ToggleAppearance, _, cx| {
                    this.toggle_appearance(cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &super::commands::ApproveCurrent, window, cx| {
                    this.approve_current(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &super::commands::PreviousScene, window, cx| {
                    this.step_scene(false, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &super::commands::NextScene, window, cx| {
                    this.step_scene(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &super::commands::GoHome, _, cx| {
                this.go_from_shortcut(Destination::Home, cx)
            }))
            .on_action(cx.listener(|this, _: &super::commands::GoVideos, _, cx| {
                this.go_from_shortcut(Destination::Videos, cx)
            }))
            .on_action(cx.listener(|this, _: &super::commands::GoSeries, _, cx| {
                this.go_from_shortcut(Destination::Series, cx)
            }))
            .on_action(cx.listener(|this, _: &super::commands::GoActivity, _, cx| {
                this.go_from_shortcut(Destination::Activity, cx)
            }))
            .on_action(
                cx.listener(|this, _: &super::commands::GoProviders, _, cx| {
                    this.go_from_shortcut(Destination::Providers, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &super::commands::GoBudget, _, cx| {
                this.go_from_shortcut(Destination::Budget, cx)
            }))
            .on_action(cx.listener(|this, _: &super::commands::GoSettings, _, cx| {
                this.go_from_shortcut(Destination::Settings, cx)
            }))
            .on_action(cx.listener(|this, _: &super::commands::GoHelp, _, cx| {
                this.go_from_shortcut(Destination::Help, cx)
            }))
            .child(self.header(compact, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(!compact, |body| body.child(self.navigation(cx)))
                    .child(
                        div()
                            .id("desktop-content")
                            .test_support()
                            .flex_1()
                            .min_w_0()
                            .overflow_y_scroll()
                            .px_8()
                            .pt_6()
                            .pb_16()
                            .child(self.recovery_notice(cx))
                            .child(self.retained_script_notice(cx))
                            .child(self.retained_scene_notice(cx))
                            .child(self.retained_note_notice(cx))
                            .when_some(self.refresh_error.as_ref(), |body, error| {
                                body.child(self.workspace_problem(error, cx))
                            })
                            .when_some(self.preference_error.as_ref(), |body, error| {
                                body.child(error_notice(
                                    &format!(
                                        "Window preferences could not be saved or restored: {error}"
                                    ),
                                    p,
                                ))
                            })
                            .child(if self.show_lab {
                                div().h(px(820.)).child(self.lab.clone())
                            } else {
                                div()
                                    .w_full()
                                    .max_w(px(CONTENT_WIDTH))
                                    .mx_auto()
                                    .child(entrance(self.page_key(), self.page(cx), cx))
                            }),
                    ),
            )
            .child(
                div()
                    .id("desktop-status")
                    .role(Role::Status)
                    .aria_label(self.status.clone())
                    .min_h(px(26.))
                    .py_1()
                    .flex()
                    .items_center()
                    .px_4()
                    .bg(p.sidebar)
                    .border_t_1()
                    .border_color(p.border)
                    .text_size(rems(0.85))
                    .text_color(p.secondary)
                    .child({
                        let line = self.status_line();
                        reveal(&line, div().child(line.clone()), 3., 160)
                    }),
            )
    }
}

impl Shell {
    fn header(&self, compact: bool, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_3()
            .min_h(px(48.))
            .py_2()
            .px_4()
            .border_b_1()
            .border_color(p.border)
            .when(!compact, |header| {
                header.child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(self.window_title()),
                )
            })
            .when(compact, |header| {
                header.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("desktop-navigation")
                                .self_start()
                                .icon(IconName::ChevronDown)
                                .label(self.destination.label())
                                .accessibility_label("Navigate workspace")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_navigation(window, cx)
                                })),
                        )
                        .child(
                            Button::new("desktop-command-search")
                                .icon(IconName::Search)
                                .label("Search")
                                .accessibility_label("Search commands and saved videos")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.search_commands(window, cx)
                                })),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .max_w_full()
                    .gap_2()
                    .when(self.snapshot.is_some(), |bar| {
                        bar.child(
                            Button::new("desktop-emergency-stop")
                                .self_start()
                                .icon(IconName::CircleStop)
                                .when(self.work_active(), |button| button.danger())
                                .when(!self.work_active(), |button| button.ghost())
                                .label(if self.control_busy {
                                    "Stopping…"
                                } else {
                                    "Stop all"
                                })
                                .accessibility_label("Stop all production jobs in this workspace")
                                .disabled(
                                    self.control_busy
                                        || self
                                            .snapshot
                                            .as_ref()
                                            .is_none_or(|snapshot| snapshot.activity.is_err()),
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.stop_all(cx))),
                        )
                    })
                    .child(
                        Button::new("desktop-appearance")
                            .self_start()
                            .ghost()
                            .icon(if self.light {
                                IconName::Moon
                            } else {
                                IconName::Sun
                            })
                            .accessibility_label(if self.light {
                                "Use dark appearance"
                            } else {
                                "Use light appearance"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_appearance(cx);
                            })),
                    ),
            )
    }

    fn navigation(&self, cx: &mut Context<Self>) -> gpui_kit::Stateful<Div> {
        let p = self.palette();
        let workspace = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.context.root.file_name())
            .and_then(|name| name.to_str())
            .map_or_else(
                || "Open a workspace".to_owned(),
                |name| name.trim_start_matches('.').to_owned(),
            );
        let groups: [(&str, &[Destination]); 3] = [
            (
                "Library",
                &[Destination::Home, Destination::Videos, Destination::Series],
            ),
            ("Production", &[Destination::Activity]),
            (
                "Setup",
                &[
                    Destination::Providers,
                    Destination::Budget,
                    Destination::Settings,
                    Destination::Help,
                ],
            ),
        ];
        let mut list = div()
            .id("desktop-sidebar")
            .w(px(220.))
            .min_h_0()
            .overflow_y_scroll()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_px()
            .px_2()
            .py_3()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            .child(self.workspace_row(workspace, cx))
            .child(self.search_row(cx));
        for (title, destinations) in groups {
            list = list.child(
                div()
                    .px_2()
                    .pt_4()
                    .pb_1()
                    .text_size(rems(0.85))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.secondary)
                    .child(title.to_uppercase()),
            );
            for destination in destinations.iter().copied() {
                list = list.child(self.nav_row(destination, cx));
            }
        }
        list
    }

    fn workspace_row(&self, workspace: String, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette();
        let blocked = self.busy || self.control_busy || self.has_unsaved_setup(cx);
        div()
            .id("desktop-open-workspace")
            .test_support()
            .flex()
            .items_center()
            .gap_2()
            .h(px(32.))
            .px_2()
            .rounded_md()
            .font_weight(FontWeight::SEMIBOLD)
            .hover(move |style| style.bg(p.text.opacity(0.06)))
            .child(gpui_kit::component::Icon::new(IconName::FolderOpen).size(rems(1.2)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(workspace),
            )
            .child(
                gpui_kit::component::Icon::new(IconName::ChevronRight)
                    .size(px(12.))
                    .text_color(p.secondary),
            )
            .when(!blocked, |row| {
                row.on_click(cx.listener(|this, _, _, cx| this.choose_workspace(cx)))
            })
    }

    fn search_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette();
        div()
            .id("desktop-command-search")
            .test_support()
            .flex()
            .items_center()
            .gap_2()
            .h(px(28.))
            .px_2()
            .mb_1()
            .rounded_md()
            .bg(p.control)
            .border_1()
            .border_color(p.border)
            .text_color(p.secondary)
            .child(gpui_kit::component::Icon::new(IconName::Search).size(px(14.)))
            .child(div().flex_1().child("Search"))
            .child(
                div()
                    .px_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(p.border)
                    .text_size(rems(0.8))
                    .child("⌘K"),
            )
            .on_click(cx.listener(|this, _, window, cx| this.search_commands(window, cx)))
    }

    /// A source-list row: icon and left-aligned label; the selected row is tinted with the accent.
    fn nav_row(&self, destination: Destination, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette();
        let selected = !self.show_lab && self.destination == destination;
        let available = self.snapshot.is_some()
            || matches!(
                destination,
                Destination::Home | Destination::Settings | Destination::Help
            );
        let tint = p.accent_fill.opacity(0.2);
        let hover = p.text.opacity(0.06);
        div()
            .id(gpui_kit::SharedString::from(format!(
                "desktop-nav-{}",
                destination.identifier()
            )))
            .test_support()
            .flex()
            .items_center()
            .gap_2()
            .h(px(28.))
            .px_2()
            .rounded_md()
            .cursor_default()
            .relative()
            .when(selected, |row| {
                row.child(selection_wash(&self.page_key(), tint))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if self.high_contrast { p.text } else { p.accent })
            })
            .when(!selected && available, |row| {
                row.hover(move |style| style.bg(hover))
            })
            .when(!available, |row| row.opacity(0.4))
            .when(self.high_contrast && selected, |row| {
                row.border_1().border_color(p.text)
            })
            .child(gpui_kit::component::Icon::new(destination.icon()).size(rems(1.2)))
            .child(div().flex_1().child(destination.label()))
            .when_some(
                Destination::ALL
                    .iter()
                    .position(|candidate| *candidate == destination)
                    .filter(|_| available),
                |row, position| {
                    row.child(
                        div()
                            .text_xs()
                            .text_color(p.secondary.opacity(0.7))
                            .child(format!("⌘{}", position + 1)),
                    )
                },
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if !available {
                    return;
                }
                this.go_to(destination, cx);
                cx.notify();
            }))
    }

    /// Changes whenever a different page or video is shown, restarting its entrance.
    fn page_key(&self) -> String {
        let subpage = [
            (self.show_script, "script"),
            (self.show_scenes, "scenes"),
            (self.show_notes, "notes"),
            (self.delivery.is_some(), "export"),
            (self.characters.is_some(), "characters"),
            (self.lines.is_some(), "lines"),
            (self.materials.is_some(), "materials"),
            (self.narration.is_some(), "narration"),
            (self.saved_media.is_some(), "media"),
            (self.observed_job.is_some(), "job"),
        ]
        .into_iter()
        .filter(|(open, _)| *open)
        .map(|(_, name)| name)
        .collect::<Vec<_>>()
        .join("+");
        format!(
            "desktop-page-{}-{}-{}-{subpage}",
            self.destination.identifier(),
            self.selected_video.as_deref().unwrap_or_default(),
            self.form
                .as_ref()
                .map_or("", |form| form.operation.path.as_str())
        )
    }

    fn page(&self, cx: &mut Context<Self>) -> Div {
        if self.busy && self.snapshot.is_none() {
            return empty(
                "Opening workspace…",
                "Reading saved work in the background. No jobs are being started.",
                self.palette(),
            );
        }
        if self.form.is_some() && self.snapshot.is_some() {
            return self.form_page(cx);
        }
        let page = match self.destination {
            Destination::Home => self.home(cx),
            Destination::Videos => self.videos(cx),
            Destination::Series => self.series_page(cx),
            Destination::Activity => self.activity(cx),
            Destination::Providers => self
                .providers_page(cx)
                .child(self.provider_login(cx))
                .child(self.provider_tests(cx)),
            Destination::Budget => self
                .budget_page(cx)
                .child(self.account_setup(cx))
                .child(self.limit_editor(cx))
                .child(self.reconciliation(cx)),
            Destination::Settings => self.settings(cx),
            Destination::Help => self.help(cx),
        };
        if self.snapshot.is_none() {
            return page;
        }
        page.child(self.more_actions(self.destination, cx))
    }

    fn home(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        if self.new_root.is_some() {
            return self.new_workspace_form(cx);
        }
        let Some(snapshot) = &self.snapshot else {
            return self.welcome(cx);
        };
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(page_header(
                if snapshot.videos.is_empty() { "Welcome" } else { "Pick up where you left off" },
                "Turn any story into a narrated recap video.",
                (!snapshot.videos.is_empty()).then(|| self.new_video_button(cx).into_any_element()),
                p,
            ))
            .when(!snapshot.videos.is_empty(), |body| {
                body.child(
                    div()
                        .flex()
                        .gap_3()
                        .child(metric("Videos", &snapshot.total_videos.to_string(), p))
                        .child(metric(
                            "In queue",
                            &snapshot.activity.as_ref().map_or_else(|_| "—".into(), |activity| activity.page.total.to_string()),
                            p,
                        )),
                )
            })
            .child(self.readiness(cx))
            .when(snapshot.videos.is_empty(), |body| {
                body.child(empty_state(
                    IconName::Clapperboard,
                    "Make your first story recap",
                    "Choose a guide, name the story, and Retold drafts the script, scenes and narration for you to review.",
                    p,
                ).child(div().flex().child(self.new_video_button(cx))))
            })
            .when(!snapshot.videos.is_empty(), |body| {
                body.child(heading_small("Recent videos")).children(
                    snapshot
                        .videos
                        .iter()
                        .filter(|video| !video.is_archived)
                        .take(6)
                        .enumerate()
                        .map(|(index, video)| self.video_row(index, video, cx)),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.setup_summary(cx))
                    .child(self.catalog_issues()),
            )
            .child(self.recent_workspaces(cx))
    }

    /// The workspace could not be read: say so plainly, keep the detail small, offer the way out.
    fn workspace_problem(&self, error: &str, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let headline = if self.snapshot.is_some() {
            "This workspace can't be read right now. You're seeing what was last loaded."
        } else {
            "This workspace can't be opened."
        };
        let detail = error.trim_end_matches('.').replace(".. ", ". ");
        div()
            .flex()
            .flex_col()
            .gap_2()
            .mb_6()
            .child(decision(
                headline,
                [
                    Button::new("desktop-workspace-retry")
                        .icon(IconName::RefreshCw)
                        .label("Try again")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    Button::new("desktop-workspace-choose")
                        .primary()
                        .icon(IconName::FolderOpen)
                        .label("Open another workspace…")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.choose_workspace(cx))),
                ],
                p,
            ))
            .child(
                div()
                    .text_xs()
                    .text_color(p.secondary)
                    .child(middle_truncated(&detail, 160)),
            )
    }

    /// First launch: what the app is, how it works, and the one step to start.
    fn welcome(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let returning = !self.preferences.recent_workspaces.is_empty();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_6()
            .pt_8()
            .child(reveal(
                "welcome-hero",
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .child(img("logo.png").size(px(88.)))
                    .child(
                        div()
                            .text_size(rems(2.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Welcome to Retold Studio"),
                    )
                    .child(
                        div()
                            .max_w(px(520.))
                            .text_center()
                            .text_color(p.secondary)
                            .child("Turn any story into a narrated recap video. Everything lives in a workspace — a folder for your videos, guides and settings."),
                    ),
                10.,
                320,
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("desktop-new-workspace")
                            .when(!returning, |button| button.primary())
                            .icon(IconName::Plus)
                            .label("Create a workspace…")
                            .on_click(cx.listener(|this, _, _, cx| this.choose_new_workspace(cx))),
                    )
                    .child(
                        Button::new("desktop-open-first")
                            .when(returning, |button| button.primary())
                            .icon(IconName::FolderOpen)
                            .label("Open a workspace…")
                            .tooltip("Open a workspace (⌘O)")
                            .on_click(cx.listener(|this, _, _, cx| this.choose_workspace(cx))),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .max_w(px(860.))
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .children(HOW_IT_WORKS.into_iter().enumerate().map(
                        |(index, (icon, title, body))| {
                            let step = div()
                                .flex_1()
                                .min_w(px(220.))
                                .flex()
                                .flex_col()
                                .gap_2()
                                .p_4()
                                .rounded_lg()
                                .border_1()
                                .border_color(p.border)
                                .bg(p.surface)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(
                                            div()
                                                .size(px(22.))
                                                .rounded_full()
                                                .bg(p.accent_fill.opacity(0.18))
                                                .text_color(p.accent)
                                                .text_xs()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child((index + 1).to_string()),
                                        )
                                        .child(
                                            gpui_kit::component::Icon::new(icon)
                                                .size(rems(1.1))
                                                .text_color(p.secondary),
                                        ),
                                )
                                .child(div().font_weight(FontWeight::MEDIUM).child(title))
                                .child(div().text_sm().text_color(p.secondary).child(body));
                            rise_in(
                                gpui_kit::ElementId::Name(format!("welcome-step-{index}").into()),
                                index + 2,
                                step,
                                cx,
                            )
                        },
                    )),
            )
            .child(div().w_full().max_w(px(860.)).child(self.recent_workspaces(cx)))
    }

    fn setup_summary(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(snapshot) = &self.snapshot else {
            return div();
        };
        match &snapshot.choices {
            Err(error) => notice(&error.message, p),
            Ok(choices) => div().flex().flex_col().gap_2()
                .when(choices.guides.is_empty(), |body| body.child(notice("Install or create a guide before making a video. This workspace has no valid guides.", p)))
                .when(choices.channels.is_empty(), |body| body.child(notice("Add a channel with narration voices before making a video.", p)))
                .children(choices.issues.iter().map(|issue| notice(issue, p)))
                .when(choices.guides.is_empty() || choices.channels.is_empty(), |body| body.child(self.import_buttons(cx))),
        }
    }

    fn catalog_issues(&self) -> Div {
        let p = self.palette();
        let Some(snapshot) = &self.snapshot else {
            return div();
        };
        let issues = self
            .catalog_page
            .as_ref()
            .map_or(snapshot.issues.as_slice(), |page| page.issues.as_slice());
        div().flex().flex_col().gap_2()
            .when(!issues.is_empty(), |body| body.child(notice(&format!("{} invalid video manifests were found. These results are partial; inspect the workspace before selecting work by an ambiguous name.", issues.len()), p)))
            .when(self.catalog_page.is_none() && snapshot.total_videos > snapshot.videos.len(), |body| body.child(notice("The recent preview covers the newest 200 records. Use Videos search or Browse all saved videos to page through older work.", p)))
    }

    fn settings(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        if self.import.is_some() {
            return self.resource_import_form(cx);
        }
        let mut body = div().flex().flex_col().gap_4().child(page_header(
            "Settings",
            "Where this workspace lives, how the app looks, and what it imports.",
            [],
            p,
        ));
        if let Some(snapshot) = &self.snapshot {
            let root = snapshot.context.root.clone();
            body = body.child(heading_small("Location")).child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .rounded_lg()
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        gpui_kit::component::Icon::new(IconName::FolderOpen)
                            .size(rems(1.2))
                            .text_color(p.secondary),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(middle_truncated(&home_relative(&root), PATH_CHARACTERS)),
                    )
                    .child(
                        Button::new("desktop-show-workspace")
                            .self_start()
                            .label("Show in Finder")
                            .on_click(cx.listener(move |_, _, _, cx| cx.reveal_path(&root))),
                    ),
            );
        } else {
            body = body.child(muted("Open a workspace to inspect its locations.", p));
        }
        body.child(self.appearance_settings(cx))
            .child(heading_small("Library"))
            .child(setting_row(
                "Guides and channels",
                self.import_buttons(cx),
                p,
            ))
            .child(setting_row(
                "Saved data",
                div().flex().child(
                    Button::new("desktop-refresh")
                        .icon(IconName::RefreshCw)
                        .label("Reload from disk")
                        .disabled(self.busy || self.snapshot.is_none() || self.import.is_some())
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                ),
                p,
            ))
            .child(
                Button::new("desktop-show-administration")
                    .ml(px(-10.))
                    .self_start()
                    .ghost()
                    .icon(if self.show_administration {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .label(if self.show_administration {
                        "Hide administration"
                    } else {
                        "Show presets, profiles and backups"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_administration = !this.show_administration;
                        cx.notify();
                    })),
            )
            .when(self.show_administration, |body| {
                body.child(self.presets_and_profiles(cx))
                    .child(self.backup_controls(cx))
                    .child(self.authoring_controls(cx))
            })
    }

    fn help(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        empty("Help", "Every page lists its own actions under More actions. Search with ⌘K to jump anywhere.", p)
            .child(heading_small("Keyboard shortcuts"))
            .child(div().flex().flex_col().gap_2().children(
                [("⌘K", "Search pages, videos and actions"), ("⌘1–⌘8", "Go to a page"), ("⌘N", "New video"), ("⌘O", "Open a workspace"), ("⌘R", "Reload the workspace"), ("⌘[ ⌘]", "Previous or next scene"), ("⌘↩", "Approve the open script or character")]
                    .into_iter()
                    .map(|(keys, action)| {
                        div().flex().items_center().gap_3()
                            .child(div().w(rems(5.)).flex().child(div().px(px(6.)).rounded(px(4.)).border_1().border_color(p.border).bg(p.surface).text_xs().font_weight(FontWeight::MEDIUM).child(keys)))
                            .child(div().child(action))
                    }),
            ))
            .when(self.show_developer, |body| body
                .child(Button::new("desktop-input-lab").self_start().label("Open fictional input & media lab").on_click(cx.listener(|this, _, _, cx| { this.show_lab = true; cx.notify(); })))
                .child(muted("The lab keeps fictional edits only in this session. It does not modify production videos.", p)))
            .child(self.diagnostics(cx))
    }
}

/// How long a status message stays before the bar returns to ambient state.
const STATUS_HOLD: std::time::Duration = std::time::Duration::from_secs(5);

/// Width of the readable content column.
const CONTENT_WIDTH: f32 = 1200.;

impl Shell {
    /// A message already shown on the previous page does not follow you to the next one;
    /// a message set by the navigation itself is new and stays.
    fn forget_stale_status(&mut self) {
        let page = self.page_key();
        if page == self.shown_page {
            return;
        }
        if self.status_seen.0 == self.status {
            self.status.clear();
        }
        self.shown_page = page;
    }

    /// Notice a new status message and repaint once it has been shown long enough.
    fn track_status(&mut self, cx: &mut Context<Self>) {
        if self.status_seen.0 == self.status {
            return;
        }
        self.status_seen = (self.status.clone(), std::time::Instant::now());
        let timer = cx.background_executor().timer(STATUS_HOLD);
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// The latest message while fresh, then quiet ambient state.
    fn status_line(&self) -> String {
        let fresh = self.status_seen.1.elapsed() < STATUS_HOLD;
        if fresh && !self.status.is_empty() {
            return self.status.clone();
        }
        let active = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.activity.as_ref().ok())
            .map_or(0, |activity| {
                activity
                    .page
                    .jobs
                    .iter()
                    .filter(|job| !job.status.is_terminal())
                    .count()
            });
        match active {
            0 => "Ready".to_owned(),
            1 => "1 job in progress".to_owned(),
            count => format!("{count} jobs in progress"),
        }
    }

    /// Whether any job is waiting or running, so Stop has something to stop.
    fn work_active(&self) -> bool {
        let activity = self
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.activity.as_ref().ok());
        activity.is_some_and(|activity| {
            activity
                .page
                .jobs
                .iter()
                .any(|job| !job.status.is_terminal())
        })
    }

    /// The toolbar title: the open video, otherwise the current page.
    fn window_title(&self) -> String {
        let video = self.selected_video.as_ref().and_then(|reference| {
            self.snapshot
                .as_ref()?
                .videos
                .iter()
                .find(|video| &video.reference == reference)
        });
        match (self.destination, video) {
            (Destination::Videos, Some(video)) => video.title.clone(),
            (destination, _) => destination.label().to_owned(),
        }
    }
}

/// A centered empty state: symbol, title, one line of guidance; callers append the action.
fn empty_state(icon: IconName, title: &str, description: &str, p: Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .py_10()
        .px_6()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .child(reveal(
            title,
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    gpui_kit::component::Icon::new(icon)
                        .size(px(40.))
                        .text_color(p.secondary),
                )
                .child(
                    div()
                        .text_size(rems(1.15))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_owned()),
                )
                .child(
                    div()
                        .max_w(px(420.))
                        .text_center()
                        .text_color(p.secondary)
                        .child(description.to_owned()),
                ),
            8.,
            260,
        ))
}

/// A segmented control: options in one track; the selected option is accent-tinted.
fn segmented(options: impl IntoIterator<Item = Button>, p: Palette) -> Div {
    div()
        .flex()
        .flex_none()
        .self_start()
        .gap_px()
        .p(px(2.))
        .rounded(px(7.))
        .bg(p.control)
        .border_1()
        .border_color(p.border)
        .children(options.into_iter().map(|option| option.ghost().small()))
}

/// A settings row: a fixed label column and its control.
fn setting_row(label: &str, control: Div, p: Palette) -> Div {
    div()
        .flex()
        .items_center()
        .gap_4()
        .py_1()
        .child(
            div()
                .w(rems(10.))
                .flex_none()
                .text_color(p.secondary)
                .child(label.to_owned()),
        )
        .child(control)
}

/// A path for display, with the home folder shown as `~`.
fn home_relative(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home.and_then(|home| {
        path.strip_prefix(&home)
            .ok()
            .map(std::path::Path::to_path_buf)
    }) {
        Some(relative) => format!("~/{}", relative.display()),
        None => path.display().to_string(),
    }
}

/// Page header: title and one-line description on the left, the page's actions on the right.
fn page_header(
    title: &str,
    description: &str,
    actions: impl IntoIterator<Item = gpui_kit::AnyElement>,
    p: Palette,
) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_start()
        .justify_between()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .flex_shrink(1.)
                .min_w_0()
                .gap_1()
                .child(heading(title))
                .when(!description.is_empty(), |text| {
                    text.child(muted(description, p))
                }),
        )
        .child(
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap_2()
                .pt_1()
                .children(actions),
        )
}

/// Icon-only refresh button with a tooltip.
fn refresh_button(id: &'static str, tooltip: &'static str, busy: bool) -> Button {
    Button::new(id)
        .ghost()
        .icon(IconName::RefreshCw)
        .tooltip(tooltip)
        .accessibility_label(tooltip)
        .disabled(busy)
}

/// A saved timestamp as local time: "Today, 9:30 PM", "Yesterday, 9:30 PM" or "Oct 12, 9:30 PM".
/// Text that is not a timestamp is shown unchanged.
pub(crate) fn when(stamp: &str) -> String {
    let Ok(instant) = stamp.parse::<jiff::Timestamp>() else {
        return stamp.to_owned();
    };
    let zone = jiff::tz::TimeZone::system();
    let local = instant.to_zoned(zone.clone());
    let today = jiff::Timestamp::now().to_zoned(zone).date();
    let clock = local.strftime("%-I:%M %p").to_string();
    let date = local.date();
    if date == today {
        return format!("Today, {clock}");
    }
    if today.yesterday().is_ok_and(|yesterday| yesterday == date) {
        return format!("Yesterday, {clock}");
    }
    format!("{}, {clock}", local.strftime("%b %-d"))
}

/// Page entrances: fade in while rising 6px, 200ms ease-out; instant under Reduce Motion.
const ENTRANCE: std::time::Duration = std::time::Duration::from_millis(200);
const ENTRANCE_RISE: f32 = 6.;

fn entrance(key: String, page: Div, cx: &gpui_kit::App) -> gpui_kit::AnyElement {
    use gpui_kit::{Animation, AnimationExt as _};
    if cx.reduce_motion() {
        return page.into_any_element();
    }
    page.relative()
        .with_animation(
            gpui_kit::ElementId::Name(key.into()),
            Animation::new(ENTRANCE).with_easing(gpui_kit::ease_out_quint()),
            |page, progress| {
                page.opacity(progress)
                    .top(px(ENTRANCE_RISE * (1. - progress)))
            },
        )
        .into_any_element()
}

/// List rows and cards arrive in a short cascade: each fades in while rising 4px over 180ms,
/// 25ms after the one before, capped at eight steps. Instant under Reduce Motion.
const RISE: f32 = 180.;
const RISE_STAGGER: f32 = 25.;
const RISE_STEPS: usize = 8;
const RISE_DISTANCE: f32 = 4.;

fn rise_in<E: Styled + IntoElement + 'static>(
    id: impl Into<gpui_kit::ElementId>,
    index: usize,
    element: E,
    cx: &gpui_kit::App,
) -> gpui_kit::AnyElement {
    use gpui_kit::{Animation, AnimationExt as _};
    if cx.reduce_motion() {
        return element.into_any_element();
    }
    #[expect(clippy::cast_precision_loss, reason = "a step count of at most eight")]
    let delay = RISE_STAGGER * index.min(RISE_STEPS) as f32;
    let total = RISE + delay;
    let ease = gpui_kit::ease_out_quint();
    element
        .relative()
        .with_animation(
            id,
            Animation::new(std::time::Duration::from_secs_f32(total / 1000.)),
            move |element, progress| {
                let local = ((progress * total - delay) / RISE).clamp(0., 1.);
                let eased = ease(local);
                element.opacity(eased).top(px(RISE_DISTANCE * (1. - eased)))
            },
        )
        .into_any_element()
}

/// The sidebar selection fill, fading in over 150ms each time the page changes.
fn selection_wash(key: &str, tint: gpui_kit::Hsla) -> gpui_kit::AnyElement {
    use gpui_kit::{Animation, AnimationExt as _};
    let wash = div().absolute().inset_0().rounded_md().bg(tint);
    if !MOTION.load(std::sync::atomic::Ordering::Relaxed) {
        return wash.into_any_element();
    }
    wash.with_animation(
        gpui_kit::ElementId::Name(format!("{key}-selection").into()),
        Animation::new(std::time::Duration::from_millis(150))
            .with_easing(gpui_kit::ease_out_quint()),
        |wash, progress| wash.opacity(progress),
    )
    .into_any_element()
}

/// Loops that only decorate (pulses) redraw at most this often, sparing the GPU and battery.
pub(super) const DECORATIVE_FPS: f32 = 30.;

/// Placeholder rows shaped like a list while it loads, pulsing gently in sync.
fn skeleton(rows: usize, p: Palette) -> gpui_kit::AnyElement {
    use gpui_kit::{Animation, AnimationExt as _};
    const WIDTHS: [f32; 4] = [0.72, 0.56, 0.84, 0.64];
    let bar = |width: f32, height: f32| {
        div()
            .h(px(height))
            .w(gpui_kit::relative(width))
            .rounded(px(4.))
            .bg(p.text.opacity(0.08))
    };
    div()
        .flex()
        .flex_col()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .children((0..rows).map(|row| {
            div()
                .flex()
                .flex_col()
                .justify_center()
                .gap_2()
                .h(px(56.))
                .px_4()
                .when(row + 1 < rows, |line| {
                    line.border_b_1().border_color(p.border)
                })
                .child(bar(WIDTHS[row % WIDTHS.len()] * 0.6, 12.))
                .child(bar(WIDTHS[row % WIDTHS.len()], 9.))
        }))
        .with_animation(
            "desktop-skeleton",
            Animation::new(std::time::Duration::from_millis(1600))
                .repeat()
                .with_max_fps(DECORATIVE_FPS)
                .with_easing(gpui_kit::pulsating_between(0.55, 1.)),
            |rows, opacity| rows.opacity(opacity),
        )
        .into_any_element()
}

/// "inciting incident" → "Inciting Incident", for saved keys shown as labels.
pub(super) fn title_case(text: &str) -> String {
    text.split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(letters).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Longest path shown in full; longer ones keep their start and end around an ellipsis.
const PATH_CHARACTERS: usize = 44;

fn middle_truncated(text: &str, limit: usize) -> String {
    let characters: Vec<char> = text.chars().collect();
    if characters.len() <= limit {
        return text.to_owned();
    }
    let head = limit / 3;
    let tail = limit - head - 1;
    let start: String = characters[..head].iter().collect();
    let end: String = characters[characters.len() - tail..].iter().collect();
    format!("{start}…{end}")
}

/// The three things Retold does for you, shown before a workspace exists.
const HOW_IT_WORKS: [(IconName, &str, &str); 3] = [
    (
        IconName::Search,
        "Pick a story",
        "Name a film or series; Retold researches it and drafts the narration.",
    ),
    (
        IconName::Sparkles,
        "Review every step",
        "Edit the script, direct scenes and approve characters before anything is spent.",
    ),
    (
        IconName::FolderOpen,
        "Export the recap",
        "Render narrated videos in every language and size you need.",
    ),
];

/// Page title: 22px semibold.
fn heading(text: &str) -> Div {
    div()
        .text_size(rems(1.7))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_owned())
}
/// Section title: 15px semibold.
fn heading_small(text: &str) -> Div {
    div()
        .mt_2()
        .text_size(rems(1.15))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_owned())
}
/// Secondary caption: 12px.
fn muted(text: &str, p: Palette) -> Div {
    div()
        .text_size(rems(0.92))
        .text_color(p.secondary)
        .child(text.to_owned())
}
fn empty(title: &str, description: &str, p: Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_5()
        .max_w(px(820.))
        .child(page_header(title, description, [], p))
}
/// Whether motion is allowed this frame; mirrors Reduce Motion for helpers without a context.
pub(super) static MOTION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Fade `element` in while it travels `rise` px into place (negative drops from above).
/// Plays once per `key`, so a new message animates and a repaint does not.
pub(super) fn reveal(key: &str, element: Div, rise: f32, millis: u64) -> Div {
    use gpui_kit::{Animation, AnimationExt as _};
    use std::hash::{Hash as _, Hasher as _};
    if !MOTION.load(std::sync::atomic::Ordering::Relaxed) {
        return element;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    let id = gpui_kit::ElementId::Name(format!("reveal-{}", hasher.finish()).into());
    div().child(
        element.relative().with_animation(
            id,
            Animation::new(std::time::Duration::from_millis(millis))
                .with_easing(gpui_kit::ease_out_quint()),
            move |element, progress| element.opacity(progress).top(px(rise * (1. - progress))),
        ),
    )
}

/// A callout with a coloured leading bar; the body text keeps full contrast.
fn callout(text: &str, p: Palette, tone: gpui_kit::Hsla) -> Div {
    reveal(text, callout_body(text, p, tone), 4., 180)
}

fn callout_body(text: &str, p: Palette, tone: gpui_kit::Hsla) -> Div {
    div()
        .flex()
        .gap_3()
        .px_3()
        .py_2()
        .rounded_md()
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .child(div().w(px(3.)).flex_none().rounded_sm().bg(tone))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(p.text)
                .child(text.to_owned()),
        )
}
/// A choice the user must make: amber-tinted banner with its message and the choices inside.
fn decision(text: &str, choices: impl IntoIterator<Item = Button>, p: Palette) -> Div {
    let banner = div()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_3()
        .p_3()
        .rounded_lg()
        .border_1()
        .border_color(p.border)
        .bg(p.surface)
        .child(
            div()
                .w(px(3.))
                .self_stretch()
                .flex_none()
                .rounded_sm()
                .bg(p.attention),
        )
        .child(div().flex_1().min_w(px(240.)).child(text.to_owned()))
        .child(div().flex().gap_2().children(choices));
    reveal(text, banner, -4., 220)
}

fn success_notice(text: &str, p: Palette) -> Div {
    callout(text, p, p.success)
}

fn callout_info(text: &str, p: Palette) -> Div {
    callout(text, p, p.secondary)
}

fn notice(text: &str, p: Palette) -> Div {
    callout(text, p, p.attention)
}
fn error_notice(text: &str, p: Palette) -> Div {
    callout(text, p, p.error)
}

fn card(title: &str, value: &str, p: Palette) -> Div {
    div()
        .p_4()
        .rounded_lg()
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .flex()
        .flex_col()
        .gap_2()
        .child(muted(title, p))
        .child(value.to_owned())
}
fn metric(title: &str, value: &str, p: Palette) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .px_4()
        .py_3()
        .rounded_lg()
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .flex()
        .flex_col()
        .gap_1()
        .child(muted(title, p))
        .child(
            div()
                .text_size(rems(1.7))
                .font_weight(FontWeight::SEMIBOLD)
                .child(value.to_owned()),
        )
}
