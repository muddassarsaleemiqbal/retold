//! Narrow windows use a modal navigation drawer; document drafts stay in their owning shell.
use gpui_kit::component::{
    Disableable as _, Selectable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
};
use gpui_kit::{
    Context, InteractiveElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::{Destination, Shell};

impl Destination {
    pub const ALL: [Self; 8] = [
        Self::Home,
        Self::Videos,
        Self::Series,
        Self::Activity,
        Self::Providers,
        Self::Budget,
        Self::Settings,
        Self::Help,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Home => "Home",
            Self::Videos => "Videos",
            Self::Series => "Series",
            Self::Activity => "Activity",
            Self::Providers => "Providers",
            Self::Budget => "Budget",
            Self::Settings => "Settings",
            Self::Help => "Help",
        }
    }

    /// The source-list symbol for this page.
    pub const fn icon(self) -> gpui_kit::assets::IconName {
        use gpui_kit::assets::IconName;
        match self {
            Self::Home => IconName::House,
            Self::Videos => IconName::Film,
            Self::Series => IconName::Layers,
            Self::Activity => IconName::Activity,
            Self::Providers => IconName::Server,
            Self::Budget => IconName::Wallet,
            Self::Settings => IconName::Settings,
            Self::Help => IconName::CircleQuestionMark,
        }
    }

    pub const fn identifier(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Videos => "videos",
            Self::Series => "series",
            Self::Activity => "activity",
            Self::Providers => "providers",
            Self::Budget => "budget",
            Self::Settings => "settings",
            Self::Help => "help",
        }
    }

    pub const fn command_label(self) -> &'static str {
        match self {
            Self::Home => "Go to Home",
            Self::Videos => "Browse videos",
            Self::Series => "Plan and run series parts",
            Self::Activity => "Go to Activity",
            Self::Providers => "Configure providers and routing",
            Self::Budget => "Inspect budget and spending",
            Self::Settings => "Open settings",
            Self::Help => "Help and diagnostics",
        }
    }
}

pub fn compact(window: &Window) -> bool {
    window.viewport_size().width < window.rem_size() * 60.
}

impl Shell {
    pub(super) fn show_navigation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let weak = cx.entity().downgrade();
        let destination = self.destination;
        let blocked = self.busy || self.control_busy || self.has_unsaved_setup(cx);
        window.open_dialog(cx, move |dialog, window, _| {
            let open = weak.clone();
            let create = weak.clone();
            dialog.title("Navigate workspace").w(px(400.)).child(
                div()
                    .id("desktop-navigation-items")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .max_h(window.viewport_size().height * 0.6)
                    .overflow_y_scroll()
                    .children(Destination::ALL.into_iter().map(|page| {
                        let weak = weak.clone();
                        Button::new(SharedString::from(format!(
                            "desktop-nav-{}",
                            page.identifier()
                        )))
                        .ghost()
                        .label(page.label())
                        .selected(destination == page)
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let _ = weak.update(cx, |this, cx| {
                                this.go_to(page, cx);
                                cx.notify();
                            });
                        })
                    }))
                    .child(
                        Button::new("desktop-navigation-open")
                            .self_start()
                            .label("Open workspace…")
                            .disabled(blocked)
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                let _ = open.update(cx, |this, cx| this.choose_workspace(cx));
                            }),
                    )
                    .child(
                        Button::new("desktop-navigation-new")
                            .self_start()
                            .label("New video")
                            .disabled(blocked)
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                let _ = create.update(cx, |this, cx| this.begin_draft(window, cx));
                            }),
                    ),
            )
        });
    }
}
