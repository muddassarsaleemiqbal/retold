//! Dedicated provider setup, routing and offline discovery with a separate save review.
use super::{card, error_notice, heading_small, muted, notice, page_header, refresh_button};
use crate::shell::{Shell, providers::Mode};
use gpui_kit::component::{
    Disableable as _, Selectable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState, Textarea},
};
use gpui_kit::{Context, Div, Entity, div, prelude::*, px, rems};
use retold_app::desktop::providers::Kind;

impl Shell {
    pub(super) fn providers_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let panel = &self.providers;
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(page_header(
                "Providers",
                "Connect your AI tools and choose which one each task uses first.",
                [
                    refresh_button(
                        "desktop-providers-refresh",
                        "Reload provider settings",
                        panel.busy,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.load_providers(cx)))
                    .into_any_element(),
                    Button::new("desktop-providers-scan")
                        .icon(gpui_kit::assets::IconName::Search)
                        .label("Find installed tools")
                        .disabled(panel.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.scan_providers(cx)))
                        .into_any_element(),
                ]
                .into_iter()
                .chain(
                    (panel.snapshot.is_some() && panel.review.is_none() && panel.form.is_none())
                        .then(|| {
                            Button::new("desktop-provider-add")
                                .primary()
                                .icon(gpui_kit::assets::IconName::Plus)
                                .label("Add a provider")
                                .disabled(panel.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.begin_provider_form(Mode::Add, window, cx)
                                }))
                                .into_any_element()
                        }),
                ),
                p,
            ))
            .when_some(panel.error.as_ref(), |body, error| {
                body.child(error_notice(error, p))
            });
        let Some(snapshot) = &panel.snapshot else {
            return body.child(notice("Load provider settings to begin. Missing settings stay untouched until you confirm a change.", p));
        };
        if let Some(review) = &panel.review {
            body = body.child(heading_small(review.summary()))
                .child(notice("Saving changes settings only. Existing routes are preserved unless this review explicitly changes one. Models and authentication have not been tested.", p))
                .child(heading_small("Before"))
                .child(review_text("desktop-provider-review-before", review.before(), p))
                .child(heading_small("After"))
                .child(review_text("desktop-provider-review-after", review.after(), p))
                .child(div().flex().gap_2()
                    .child(Button::new("desktop-provider-back").label("Back to editing").disabled(panel.busy)
                        .on_click(cx.listener(|this, _, _, cx| { this.providers.review = None; cx.notify(); })))
                    .child(Button::new("desktop-provider-apply").primary().label("Confirm and save settings").disabled(panel.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.apply_provider(cx)))));
        } else if panel.form.is_some() {
            body = body.child(self.provider_form(cx));
        } else {
            body = body
                .when(snapshot.providers.is_empty(), |body| body.child(div().flex().flex_col().gap_1().p_4().rounded_lg().border_1().border_color(p.border).bg(p.surface)
                    .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child("Your tools, your choice"))
                    .child(muted("Add an app on this Mac, a signed-in account, or an online service. You can browse and write drafts before connecting one.", p))))
                .children(snapshot.providers.iter().enumerate().map(|(index, provider)| {
                    let edit_id = provider.id.clone(); let remove_id = provider.id.clone();
                    div().p_4().rounded_lg().bg(p.surface).flex().flex_col().gap_2()
                        .child(heading_small(&provider.id))
                        .child(muted(&format!("{} · up to {} concurrent requests", provider.kind, provider.max_parallel), p))
                        .child(muted(&format!("Supports: {}", provider.supported_tiers.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")), p))
                        .child(muted("Not tested yet.", p))
                        .child(div().flex().flex_wrap().gap_2()
                            .child(Button::new(("desktop-provider-edit", index)).self_start().label("Edit definition").disabled(panel.busy)
                                .on_click(cx.listener(move |this, _, window, cx| this.begin_provider_form(Mode::Replace(edit_id.clone()), window, cx))))
                            .child(Button::new(("desktop-provider-remove", index)).self_start().label("Remove…").disabled(panel.busy)
                                .on_click(cx.listener(move |this, _, window, cx| this.begin_provider_form(Mode::Remove(remove_id.clone()), window, cx)))))
                }))
                .child(heading_small("Task routing"))
                .child(muted("Each task tries its providers in order, first one first.", p))
                .when(snapshot.routing.is_empty(), |body| body.child(notice("No task has a provider yet. Add a provider above, then choose one for Script and story writing first — videos can't be drafted without it.", p)))
                .child(div().rounded_lg().border_1().border_color(p.border).bg(p.surface).overflow_hidden().children(retold_app::providers::routing_tiers().into_iter().enumerate().filter(|(_, tier)| panel.show_all_routes || *tier == "text.smart" || snapshot.routing.contains_key(*tier)).map(|(index, tier)| {
                    let chain = snapshot.routing.get(tier).map(|chain| chain.join("  →  ")).unwrap_or_else(|| "No route configured".into());
                    let configured = snapshot.routing.contains_key(tier);
                    div().flex().items_center().gap_3().h(px(48.)).px_4().border_b_1().border_color(p.border.opacity(0.5))
                        .child(div().flex().flex_col().flex_1().min_w_0()
                            .child(div().font_weight(gpui_kit::FontWeight::MEDIUM).child(task_label(tier)))
                            .child(div().text_sm().text_color(p.secondary).child(if configured { chain } else { "Not set".into() })))
                        .child(Button::new(("desktop-provider-route", index)).ghost().icon(gpui_kit::assets::IconName::ChevronRight).tooltip("Choose providers for this task").accessibility_label("Edit route").disabled(panel.busy)
                            .on_click(cx.listener(move |this, _, window, cx| this.begin_provider_form(Mode::Route(tier.into()), window, cx))))
                })))
                .child(Button::new("desktop-provider-show-all").self_start().ghost().ml(px(-10.))
                    .icon(if panel.show_all_routes { gpui_kit::assets::IconName::ChevronDown } else { gpui_kit::assets::IconName::ChevronRight })
                    .label(if panel.show_all_routes { "Show fewer tasks" } else { "Show all tasks" })
                    .on_click(cx.listener(|this, _, _, cx| { this.providers.show_all_routes = !this.providers.show_all_routes; cx.notify(); })));
        }
        body.child(self.provider_observations(cx))
            .child(self.provider_inventory())
    }

    fn provider_form(&self, cx: &mut Context<Self>) -> Div {
        let Some(form) = &self.providers.form else {
            return div();
        };
        let p = self.palette();
        let blocked = self.providers.busy;
        let mut body = div().flex().flex_col().gap_3().max_w(rems(48.));
        match &form.mode {
            Mode::Add => {
                body = body.child(heading_small("Add a provider"))
                    .child(provider_field("Provider ID", &form.id, blocked, p))
                    .child(muted("Choose a short name for this account or tool, such as personal-codex or local-speech.", p));
                if !form.advanced {
                    body = body.child(div().flex().flex_wrap().gap_2().children(Kind::ALL.into_iter().enumerate().map(|(index, kind)| {
                        Button::new(("desktop-provider-kind", index)).self_start().label(kind.label()).selected(form.kind == kind).disabled(blocked)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(form) = &mut this.providers.form { form.kind = kind; }
                                this.providers.review = None; cx.notify();
                            }))
                    })));
                }
            }
            Mode::Replace(id) => body = body.child(heading_small(&format!("Edit {id}"))),
            Mode::Remove(id) => body = body.child(heading_small(&format!("Remove {id}"))).child(notice("Removal preserves credentials, files and other providers. A provider still used by a route cannot be removed.", p)),
            Mode::ClearRoute(tier) => body = body.child(heading_small(&format!("Remove {tier} route"))).child(notice("New work needing this task will be blocked until you configure another route. Providers and saved output are kept.", p)),
            Mode::Route(tier) => body = body.child(heading_small(&format!("Routing for {tier}")))
                .child(provider_field("Fallback order", &form.order, blocked, p))
                .child(muted("Providers in order of preference, separated by commas, such as personal:smart, backup:smart.", p))
                .when(self.providers.snapshot.as_ref().is_some_and(|snapshot| snapshot.routing.contains_key(tier)), |body| {
                    let tier = tier.clone();
                    body.child(Button::new("desktop-provider-clear-route").self_start().label("Remove this route…").disabled(blocked)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(form) = &mut this.providers.form { form.mode = Mode::ClearRoute(tier.clone()); }
                            this.providers.review = None;
                            cx.notify();
                        })))
                }),
        }
        if matches!(form.mode, Mode::Add | Mode::Replace(_)) {
            if form.advanced {
                body = body.child(muted("Advanced definition in TOML. Refer to API keys by environment variable name.", p))
                    .child(Textarea::new(&form.definition).aria_label("Provider definition").h(px(260.)).disabled(blocked));
            } else {
                body = body.child(provider_field(
                    "Parallel requests",
                    &form.parallel,
                    blocked,
                    p,
                ));
                if form.kind == Kind::OpenRouter {
                    body = body.child(provider_field(
                        "API key environment variable",
                        &form.key,
                        blocked,
                        p,
                    ));
                } else {
                    body = body.child(provider_field(
                        "Executable path (optional when on PATH)",
                        &form.binary,
                        blocked,
                        p,
                    ));
                }
                if matches!(form.kind, Kind::Codex | Kind::Claude) {
                    body = body.child(provider_field(
                        "Profile folder (optional)",
                        &form.profile,
                        blocked,
                        p,
                    ));
                }
                if matches!(
                    form.kind,
                    Kind::Codex | Kind::Claude | Kind::Gemini | Kind::LocalText
                ) {
                    body = body
                        .child(provider_field("Model selector", &form.selector, blocked, p))
                        .child(provider_field(
                            if form.kind == Kind::LocalText {
                                "GGUF model path"
                            } else {
                                "Model name (optional for CLI default)"
                            },
                            &form.model,
                            blocked,
                            p,
                        ));
                }
                if form.kind == Kind::LocalWhisper {
                    body = body
                        .child(provider_field(
                            "Whisper model path",
                            &form.model,
                            blocked,
                            p,
                        ))
                        .child(provider_field(
                            "FFmpeg executable",
                            &form.ffmpeg,
                            blocked,
                            p,
                        ));
                }
                body = body.child(
                    Button::new("desktop-provider-advanced")
                        .self_start()
                        .label("Edit advanced definition…")
                        .disabled(blocked)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.provider_advanced(window, cx)),
                        ),
                );
            }
        }
        body.child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(
                    Button::new("desktop-provider-preview")
                        .self_start()
                        .primary()
                        .label("Review change")
                        .disabled(blocked)
                        .on_click(cx.listener(|this, _, _, cx| this.review_provider(cx))),
                )
                .child(
                    Button::new("desktop-provider-cancel")
                        .self_start()
                        .label("Cancel")
                        .disabled(blocked)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.providers.form = None;
                            this.providers.review = None;
                            this.providers.error = None;
                            cx.notify();
                        })),
                ),
        )
    }

    fn provider_observations(&self, cx: &mut Context<Self>) -> Div {
        let p = self.palette();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(heading_small("Connection status"))
            .child(muted(
                "Check which providers are reachable and signed in.",
                p,
            ))
            .child(
                Button::new("desktop-provider-observe")
                    .self_start()
                    .label("Check connections")
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.observe_providers(cx))),
            );
        let Some(observations) = &self.providers.observations else {
            return body;
        };
        if !self
            .providers
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.version == observations.version)
        {
            return body.child(notice(
                "Settings changed. Inspect status again for the current configuration.",
                p,
            ));
        }
        body = body.children(observations.health.iter().map(|provider| {
            let detail = provider
                .last_result
                .as_ref()
                .map_or_else(String::new, |result| {
                    format!(
                        "Last result: {result} · observed {} · retry {} · {} consecutive failures",
                        provider.observed_at.as_deref().unwrap_or("unknown"),
                        provider.retry_at.as_deref().unwrap_or("not scheduled"),
                        provider.consecutive_failures
                    )
                });
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_3()
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
                                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                                .child(provider.id.clone()),
                        )
                        .child(
                            div()
                                .px_2()
                                .rounded_full()
                                .text_xs()
                                .bg(p.text.opacity(0.08))
                                .text_color(p.secondary)
                                .child(status_label(provider.status)),
                        ),
                )
                .when(!detail.is_empty(), |card| card.child(muted(&detail, p)))
        }));
        body.children(
            observations
                .profiles
                .iter()
                .map(|profile| match &profile.status {
                    Ok(status) => card(
                        &format!("{}: {}", profile.id, status_label(&status.status)),
                        &status.detail,
                        p,
                    ),
                    Err(error) => error_notice(&format!("{}: {}", profile.id, error.message), p),
                }),
        )
    }

    fn provider_inventory(&self) -> Div {
        let Some(inventory) = &self.providers.inventory else {
            return div();
        };
        let p = self.palette();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(heading_small("Installed tool discovery"))
            .child(muted("Found on this Mac. Not signed in or tested yet.", p))
            .children(inventory.tools.iter().map(|tool| {
                card(
                    tool.name,
                    &tool.path.as_ref().map_or_else(
                        || "Not found on PATH".into(),
                        |path| path.display().to_string(),
                    ),
                    p,
                )
            }))
            .children(
                inventory
                    .profiles
                    .iter()
                    .map(|path| card("Profile folder", &path.display().to_string(), p)),
            )
            .children(inventory.credential_variables.iter().map(|variable| {
                muted(
                    &format!(
                        "{}: {}",
                        variable.name,
                        if variable.present {
                            "present"
                        } else {
                            "not set"
                        }
                    ),
                    p,
                )
            }))
    }
}
fn status_label(status: &str) -> &str {
    match status {
        "not_checked" => "Not checked",
        "cooldown" => "Waiting before retry",
        "cooldown_elapsed" => "Retry delay elapsed; access unverified",
        "logged_out" => "Login required",
        "failing" => "Last request failed",
        "ok" => "Last request succeeded",
        "unavailable" => "Unavailable at last check",
        "profile_found" => "Profile folder found; login unverified",
        "missing_profile" => "Profile folder missing",
        "configured" => "Credential variable present; access unverified",
        "missing_credential" => "Credential variable missing",
        _ => status,
    }
}
fn provider_field(
    label: &str,
    input: &Entity<InputState>,
    disabled: bool,
    p: crate::theme::Palette,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(muted(label, p))
        .child(
            Input::new(input)
                .aria_label(label.to_owned())
                .disabled(disabled),
        )
}
fn review_text(id: &'static str, text: &str, p: crate::theme::Palette) -> Div {
    div().child(
        div()
            .id(id)
            .max_h(px(220.))
            .overflow_y_scroll()
            .p_4()
            .bg(p.surface)
            .rounded_lg()
            .child(text.to_owned()),
    )
}

/// What each routed task is used for, in plain words.
fn task_label(tier: &str) -> &'static str {
    match tier {
        "text.fast" => "Quick writing — titles, tags, checks",
        "text.smart" => "Script and story writing",
        "text.best" => "Highest-quality writing",
        "text.local" => "On-device writing",
        "vision.fast" => "Quick image checks",
        "vision.smart" => "Careful image review",
        "image" => "Scene images",
        "image.no_refs" => "Images without reference sheets",
        "tts" => "Narration voices",
        "transcribe" => "Speech timing",
        _ => "Other task",
    }
}
