//! Writing checks run the CLI's craft audit on the open script section, in process.
use gpui_kit::Context;
use retold_app::protocol::Payload;

use super::Shell;

/// What the audit found in one section.
#[derive(Debug, Clone, Default)]
pub(super) struct WritingCheck {
    pub score: u64,
    pub cliches: Vec<String>,
    pub advice: Vec<String>,
    pub rhythm: Vec<String>,
    pub sensory_verbs: u64,
    pub words: u64,
    /// The section and text that were checked, to name it and spot later edits.
    pub section: String,
    pub text: String,
}

fn strings(value: &serde_json::Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn parse(payload: &Payload) -> Option<WritingCheck> {
    let Payload::Result { contents, .. } = payload else {
        return None;
    };
    let audit = &contents["audit"];
    Some(WritingCheck {
        score: audit["script_score"].as_u64()?,
        cliches: strings(audit, "banned_cliches_found"),
        advice: strings(audit, "recommendations"),
        rhythm: strings(audit, "rhythm_warnings"),
        sensory_verbs: audit["sensory_verb_count"].as_u64().unwrap_or_default(),
        words: audit["total_words"].as_u64().unwrap_or_default(),
        section: String::new(),
        text: String::new(),
    })
}

impl Shell {
    /// The open section's latest writing score, if it has been checked.
    #[must_use]
    pub fn editor_writing_score(&self) -> Option<u64> {
        self.editor
            .as_ref()?
            .writing
            .as_ref()
            .map(|check| check.score)
    }

    /// Audit the text in the open section without saving it.
    pub(super) fn check_writing(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else {
            return;
        };
        let text = editor.text.read(cx).value().to_string();
        let section = editor
            .content()
            .and_then(|content| content.script.sections.get(editor.section))
            .map(|section| super::screen::title_case(&section.section))
            .unwrap_or_default();
        let checked = text.clone();
        if text.trim().is_empty() {
            return;
        }
        self.busy = true;
        let work = cx.background_executor().spawn(async move {
            let arguments = ["creative".to_owned(), "audit".to_owned(), text];
            retold_cli::run_operation(&arguments).0
        });
        cx.spawn(async move |this, cx| {
            let payload = work.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let check = parse(&payload).map(|check| WritingCheck {
                    section,
                    text: checked,
                    ..check
                });
                if check.is_none() {
                    this.status = "The writing check could not run.".into();
                }
                if let Some(editor) = &mut this.editor {
                    editor.writing = check;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
