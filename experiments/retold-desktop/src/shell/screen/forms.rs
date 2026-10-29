//! Operation forms, their structured results and each page's "More actions" list.
use gpui_kit::component::{
    Disableable as _, Selectable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::Input,
};
use gpui_kit::{Context, Div, FontWeight, div, prelude::*, px};
use retold_app::protocol::Payload;
use retold_cli::forms::FieldKind;

use super::{error_notice, heading, heading_small, muted, notice, segmented};
use crate::shell::{
    Destination, Shell,
    forms::{self, Step},
};

/// Rows shown for a list in a result; the count of the rest is stated.
const RESULT_ROWS: usize = 50;
/// Columns shown for a table in a result.
const RESULT_COLUMNS: usize = 6;
/// Widest a form control grows.
const FIELD_WIDTH: f32 = 480.;
/// Pick-list entries shown under a field.
const PICKS: usize = 12;
/// Nesting shown before values are summarised.
const RESULT_DEPTH: usize = 3;

/// Which operations belong on each page.
pub(super) fn domain(destination: Destination, video_open: bool) -> &'static [&'static str] {
    match destination {
        Destination::Videos if video_open => &["video "],
        Destination::Videos => &["new", "video new", "video list"],
        Destination::Series => &["series "],
        Destination::Activity => &["studio ", "queue "],
        Destination::Providers => &["providers "],
        Destination::Budget => &["account ", "spend"],
        Destination::Settings => &[
            "config ",
            "export-presets ",
            "views ",
            "guides ",
            "channels ",
            "workspace ",
            "storage ",
            "library ",
        ],
        Destination::Home => &["home", "setup", "assets "],
        Destination::Help => &[
            "doctor ",
            "diagnostics",
            "schema",
            "usage",
            "explain",
            "completions",
            "man",
            "dev ",
            "publish",
            "creative ",
        ],
    }
}

/// Operations meant for scripting and diagnosis, listed apart from everyday work.
const DEVELOPER: [&str; 8] = [
    "publish",
    "schema",
    "usage",
    "completions",
    "man",
    "dev ",
    "explain",
    "creative ",
];

fn belongs(path: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        path == prefix.trim_end() || (prefix.ends_with(' ') && path.starts_with(prefix))
    })
}

fn humanize(key: &str) -> String {
    retold_cli::forms::label(key)
}

fn scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => "—".into(),
        serde_json::Value::Array(items) => format!("{} items", items.len()),
        serde_json::Value::Object(map) => format!("{} fields", map.len()),
        other => other.to_string(),
    }
}

impl Shell {
    /// The list of this page's operations; opening one shows its form.
    pub(super) fn more_actions(&self, destination: Destination, cx: &mut Context<Self>) -> Div {
        let prefixes = domain(destination, self.selected_video.is_some());
        let operations: Vec<&retold_cli::forms::Operation> = crate::shell::forms::all_operations()
            .iter()
            .filter(|operation| {
                belongs(&operation.path, prefixes) && operation.path != "queue attach"
            })
            .collect();
        if operations.is_empty() {
            return div();
        }
        if !self.show_actions {
            return div().flex().child(
                Button::new("desktop-more-actions")
                    .ml(px(-10.))
                    .self_start()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::Ellipsis)
                    .label("More actions…")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_actions = true;
                        cx.notify();
                    })),
            );
        }
        let (developer, everyday): (Vec<_>, Vec<_>) = operations
            .iter()
            .partition(|operation| belongs(&operation.path, &DEVELOPER));
        let developer_start = everyday.len();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(heading_small("More actions"))
                    .child(
                        Button::new("desktop-more-actions-hide")
                            .ghost()
                            .small()
                            .label("Hide")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_actions = false;
                                cx.notify();
                            })),
                    ),
            )
            .child(self.action_list(everyday, 0, cx))
            .when(!developer.is_empty(), |body| {
                body.child(
                    Button::new("desktop-developer-tools")
                        .ml(px(-10.))
                        .self_start()
                        .ghost()
                        .icon(if self.show_developer {
                            gpui_kit::assets::IconName::ChevronDown
                        } else {
                            gpui_kit::assets::IconName::ChevronRight
                        })
                        .label("Developer tools")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_developer = !this.show_developer;
                            cx.notify();
                        })),
                )
                .when(self.show_developer, |body| {
                    body.child(self.action_list(developer, developer_start, cx))
                })
            })
    }

    /// One grouped list of operations; `first` keeps control ids unique across groups.
    fn action_list(
        &self,
        operations: Vec<&retold_cli::forms::Operation>,
        first: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.palette();
        let count = operations.len();
        let hover = p.text.opacity(0.04);
        div()
            .rounded_lg()
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden()
            .children(
                operations
                    .into_iter()
                    .enumerate()
                    .map(|(offset, operation)| {
                        let path = operation.path.clone();
                        let row = div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .px_3()
                            .py_2()
                            .hover(move |style| style.bg(hover))
                            .when(offset + 1 < count, |row| {
                                row.border_b_1().border_color(p.border)
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .child(
                                        Button::new(("desktop-action", first + offset))
                                            .self_start()
                                            .ghost()
                                            .label(humanize(&operation.path))
                                            .disabled(self.busy)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.open_form(&path, window, cx)
                                            })),
                                    )
                                    .child(div().px_3().child(muted(&operation.summary, p))),
                            )
                            .child(
                                gpui_kit::component::Icon::new(
                                    gpui_kit::assets::IconName::ChevronRight,
                                )
                                .size(px(14.))
                                .text_color(p.secondary),
                            );
                        super::rise_in(("desktop-action-rise", first + offset), offset, row, cx)
                    }),
            )
    }

    /// The open form, replacing the page content until closed.
    pub(super) fn form_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(form) = &self.form else {
            return div();
        };
        let operation = &form.operation;
        let fields = forms::shown(operation)
            .enumerate()
            .map(|(index, field)| {
                let label = if field.required || field.kind == FieldKind::Toggle {
                    field.label.clone()
                } else {
                    format!("{} (optional)", field.label)
                };
                let context_video = matches!(field.id.as_str(), "reference" | "video")
                    && self.selected_video.is_some();
                if context_video {
                    let title = self.window_title_for_forms();
                    return div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(div().w(px(120.)).text_color(p.secondary).child("Video"))
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(p.control)
                                .border_1()
                                .border_color(p.border)
                                .child(title),
                        );
                }
                let control = match &field.kind {
                    FieldKind::Toggle => {
                        let on = form.toggles.get(&field.id).copied().unwrap_or_default();
                        let id = field.id.clone();
                        let shell = cx.entity().downgrade();
                        div().flex().child(
                            gpui_kit::component::switch::Switch::new((
                                "desktop-form-toggle",
                                index,
                            ))
                            .checked(on)
                            .accessibility_label(field.label.clone())
                            .on_click(move |_, _, app| {
                                let _ = shell.update(app, |this, cx| this.toggle_field(&id, cx));
                            }),
                        )
                    }
                    FieldKind::Choice(options) => div().flex().child(segmented(
                        options.iter().enumerate().map(|(choice_index, option)| {
                            let (id, value) = (field.id.clone(), option.clone());
                            let selected = form.choices.get(&field.id) == Some(option);
                            Button::new(("desktop-form-choice", index * 64 + choice_index))
                                .self_start()
                                .label(humanize(option))
                                .selected(selected)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.choose_field(&id, &value, cx)
                                }))
                        }),
                        p,
                    )),
                    FieldKind::Path => {
                        let id = field.id.clone();
                        div()
                            .flex()
                            .gap_2()
                            .when_some(form.inputs.get(&field.id), |row, input| {
                                row.child(
                                    div()
                                        .flex_1()
                                        .child(Input::new(input).aria_label(field.label.clone())),
                                )
                            })
                            .child(
                                Button::new(("desktop-form-browse", index))
                                    .self_start()
                                    .label("Choose…")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.choose_path(id.clone(), window, cx)
                                    })),
                            )
                    }
                    FieldKind::Number | FieldKind::Text => self.picker(index, field, cx),
                };
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .max_w(px(FIELD_WIDTH))
                    .child(div().font_weight(FontWeight::MEDIUM).child(label))
                    .child(control)
                    .when(!field.help.is_empty(), |body| {
                        body.child(muted(&field.help, p))
                    })
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap_5()
            .max_w(px(720.))
            .child(
                Button::new("desktop-form-close")
                    .self_start()
                    .ghost()
                    .icon(gpui_kit::assets::IconName::ChevronLeft)
                    .ml(gpui_kit::px(-10.))
                    .label("Back")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.form = None;
                        cx.notify();
                    })),
            )
            .child(heading(&humanize(&operation.path)))
            .when(!operation.summary.is_empty(), |body| {
                body.child(muted(&operation.summary, p))
            })
            .when(fields.is_empty(), |body| {
                body.child(muted("This action has no options.", p))
            })
            .children(fields)
            .child(self.form_actions(cx))
            .when_some(form.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            })
            .when_some(form.result.as_ref(), |body, payload| {
                body.child(self.result_view(payload))
            })
    }

    /// A text field with up to [`PICKS`] choices from the workspace, such as its videos.
    fn picker(
        &self,
        index: usize,
        field: &retold_cli::forms::Field,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(form) = &self.form else {
            return div();
        };
        let choices = form.choices_for.get(&field.id).cloned().unwrap_or_default();
        let picks = choices
            .into_iter()
            .take(PICKS)
            .enumerate()
            .map(|(pick, (label, value))| {
                let id = field.id.clone();
                Button::new(("desktop-form-pick", index * PICKS + pick))
                    .self_start()
                    .ghost()
                    .label(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pick_value(&id, value.clone(), window, cx)
                    }))
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .when_some(form.inputs.get(&field.id), |body, input| {
                body.child(Input::new(input).aria_label(field.label.clone()))
            })
            .when(!picks.is_empty(), |body| {
                body.child(div().flex().flex_wrap().gap_1().children(picks))
            })
    }

    /// The open video's title, for the read-only video token in forms.
    fn window_title_for_forms(&self) -> String {
        let reference = self.selected_video.clone().unwrap_or_default();
        self.snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .videos
                    .iter()
                    .find(|video| video.reference == reference)
            })
            .map_or(reference, |video| video.title.clone())
    }

    fn form_actions(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let Some(form) = &self.form else {
            return div();
        };
        let step = |id: &'static str, label: &'static str, step: Step| {
            Button::new(id)
                .self_start()
                .label(label)
                .disabled(self.busy)
                .on_click(cx.listener(move |this, _, _, cx| this.form_step(step, cx)))
        };
        let operation = &form.operation;
        if form.reads_only() {
            return div()
                .flex()
                .child(step("desktop-form-run", "Show results", Step::Run).primary());
        }
        if operation.has_dry_run() {
            return div()
                .flex()
                .flex_col()
                .gap_2()
                .child(muted("Preview first to see exactly what changes.", p))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(step("desktop-form-preview", "Preview", Step::Preview))
                        .when(form.previewed, |row| {
                            row.child(
                                step("desktop-form-apply", "Apply previewed change", Step::Apply)
                                    .primary(),
                            )
                        }),
                );
        }
        if form.reviewing {
            let summary = form.summary(cx);
            return div()
                .flex()
                .flex_col()
                .gap_2()
                .child(notice(
                    "This changes saved work. Check the values, then confirm.",
                    p,
                ))
                .children(summary.iter().map(|line| muted(line, p)))
                .when(summary.is_empty(), |body| {
                    body.child(muted("No options set.", p))
                })
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(step("desktop-form-confirm", "Confirm", Step::Apply).primary())
                        .child(
                            Button::new("desktop-form-cancel")
                                .self_start()
                                .ghost()
                                .label("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(form) = &mut this.form {
                                        form.reviewing = false;
                                    }
                                    cx.notify();
                                })),
                        ),
                );
        }
        div()
            .flex()
            .child(step("desktop-form-review", "Review changes…", Step::Review).primary())
    }

    fn result_view(&self, payload: &Payload) -> Div {
        let p = self.palette();
        match payload {
            Payload::Error { diagnostic, .. } => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(error_notice(&diagnostic.message, p))
                .children(
                    diagnostic
                        .suggested_actions
                        .iter()
                        .map(|action| muted(&format!("Try: {}", action.join(" ")), p)),
                ),
            Payload::Result { contents, .. } => div()
                .flex()
                .flex_col()
                .gap_2()
                .child(heading_small("Result"))
                .child(self.value_view(contents, 0)),
            Payload::Event(_) => div(),
        }
    }

    fn value_view(&self, value: &serde_json::Value, depth: usize) -> Div {
        let p = self.palette();
        match value {
            serde_json::Value::Object(map) => {
                let text = map.get("text").and_then(serde_json::Value::as_str);
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .when_some(text, |body, text| body.child(div().child(text.to_owned())))
                    .children(map.iter().filter(|(key, _)| key.as_str() != "text").map(
                        |(key, item)| {
                            let nested = matches!(
                                item,
                                serde_json::Value::Object(_) | serde_json::Value::Array(_)
                            );
                            if nested && depth < RESULT_DEPTH {
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .pl_3()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(humanize(key)),
                                    )
                                    .child(self.value_view(item, depth + 1))
                            } else {
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(muted(&humanize(key), p))
                                    .child(scalar(item))
                            }
                        },
                    ))
            }
            serde_json::Value::Array(items) => self.table_view(items, depth),
            other => div().child(scalar(other)),
        }
    }

    fn table_view(&self, items: &[serde_json::Value], depth: usize) -> Div {
        let p = self.palette();
        if items.is_empty() {
            return muted("None.", p);
        }
        let columns: Vec<String> = items[0]
            .as_object()
            .map(|map| map.keys().take(RESULT_COLUMNS).cloned().collect())
            .unwrap_or_default();
        if columns.is_empty() {
            return div().flex().flex_col().gap_1().children(
                items
                    .iter()
                    .take(RESULT_ROWS)
                    .map(|item| self.value_view(item, depth + 1)),
            );
        }
        let header = div().flex().gap_3().children(columns.iter().map(|column| {
            div()
                .w(px(140.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(humanize(column))
        }));
        let rows = items.iter().take(RESULT_ROWS).map(|item| {
            div().flex().gap_3().children(
                columns
                    .iter()
                    .map(|column| div().w(px(140.)).child(scalar(&item[column.as_str()]))),
            )
        });
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(header)
            .children(rows)
            .when(items.len() > RESULT_ROWS, |body| {
                body.child(muted(
                    &format!("{} more not shown.", items.len() - RESULT_ROWS),
                    p,
                ))
            })
    }
}
