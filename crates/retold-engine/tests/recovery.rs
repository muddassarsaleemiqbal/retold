//! Failure/recovery checks run offline; no model, audio, image or editor calls.

#[cfg(test)]
mod tests {

    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use indexmap::IndexMap;
    use proptest::prelude::*;
    use retold_core::Tier;
    use retold_engine::pipeline::{Pipeline, PipelineError, Until};
    use retold_engine::store::{Receipt, Store, hash};
    use retold_engine::story::{self, Direction, Script, SectionText};
    use retold_engine::{NewRun, Quality, RunLayout, RunMeta, Stage, fsx, run};
    use retold_guide::{Guide, Origin, catalog::bundled, channel::init_from_template};
    use retold_providers::{
        Billing, ProviderError, ProvidersConfig, Router, TextProvider, TextRequest, TextResponse,
        Usage,
    };
    use serde_json::{Value, json};
    use tokio_util::sync::CancellationToken;

    fn guide() -> Guide {
        let original = bundled().unwrap().remove(0);
        let mut files = original.files().clone();
        let manifest = files.get_mut("guide.toml").unwrap();
        *manifest = manifest
            .replace("words = [230, 260]", "words = [1, 200]")
            .replace("subscription = 2", "subscription = 1");
        Guide::from_files(Origin::Inline, files).unwrap()
    }

    fn setup(g: &Guide) -> (tempfile::TempDir, RunLayout, RunMeta) {
        let tmp = tempfile::tempdir().unwrap();
        let c = init_from_template("test-studio", &tmp.path().join("config")).unwrap();
        let raw: IndexMap<_, _> = [
            ("title".into(), "Test story".into()),
            ("kind".into(), "film".into()),
        ]
        .into();
        let (layout, meta) = run::create(
            &tmp.path().join("library"),
            NewRun {
                guide: g,
                channel: &c,
                inputs: retold_guide::inputs::resolve(g.manifest(), &raw).unwrap(),
                variants: vec!["en".parse().unwrap()],
                quality: Quality::Standard,
                budget: retold_core::Usd::ZERO,
                skip_reviews: true,
                source_file: None,
            },
        )
        .unwrap();
        (tmp, layout, meta)
    }

    fn script() -> Script {
        let texts = [
            "A stranger buys the house across the narrow street. Nobody sleeps safely.",
            "Thomas leaves home to arrange the sale, carrying a book that warns him about creatures who cannot survive daylight. His wife waits.",
            "The visitor reaches town inside a coffin carried by ship.",
            "People die. Ellen reads the book and finds one possible escape. A woman must willingly keep the creature beside her until morning while everyone else mistakes his arrival for a spreading illness.",
            "She opens her window. Thomas goes to find a doctor, leaving her alone.",
            "The visitor feeds, forgetting the approaching sunrise as she holds him there through the last hours of the night. Daylight destroys him.",
            "Ellen dies too, but the town survives because she chooses to stay. Would you have opened that window?",
        ];
        let ids = [
            "hook",
            "setup",
            "inciting",
            "escalation",
            "twist",
            "climax",
            "ending",
        ];
        Script {
            sections: ids
                .into_iter()
                .zip(texts)
                .map(|(section, text)| SectionText {
                    section: section.into(),
                    text: text.into(),
                })
                .collect(),
        }
    }

    #[test]
    fn locks_release_and_crash_between_cache_and_state_is_repaired() {
        let g = guide();
        let (_tmp, layout, _) = setup(&g);
        let mut store = Store::open(&layout).unwrap();
        let locked = Store::open(&layout).unwrap_err();
        assert!(matches!(locked, retold_engine::EngineError::Locked { .. }));
        assert!(
            locked
                .to_string()
                .contains(&format!("pid {}", std::process::id())),
            "{locked}"
        );
        let key = hash(&"request").unwrap();
        let receipt = Receipt {
            provider: "subscription".into(),
            model: "test".into(),
            input_tokens: 10,
            output_tokens: 5,
            cost: retold_core::Usd::ZERO,
        };
        store
            .commit("draft", &key, &script(), Some(receipt))
            .unwrap();
        drop(store);
        std::fs::remove_file(layout.state_file()).unwrap(); // crash before the state rename
        let mut recovered = Store::open(&layout).unwrap();
        let value: Script = recovered.cached("draft", &key).unwrap().unwrap();
        assert_eq!(value.narration(), script().narration());
        assert_eq!(
            recovered.state.completed["draft"]
                .receipt
                .as_ref()
                .unwrap()
                .input_tokens,
            10
        );
        assert!(
            recovered
                .cached::<Script>("draft", &hash(&"new request").unwrap())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn corrupted_cache_is_reported_without_reusing_it() {
        let g = guide();
        let (_tmp, layout, _) = setup(&g);
        let mut store = Store::open(&layout).unwrap();
        let key = hash(&"request").unwrap();
        store.commit("draft", &key, &script(), None).unwrap();
        let path = layout
            .meta()
            .join("cache")
            .join(format!("draft-{key}.json"));
        let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["value"]["sections"][0]["text"] = json!("tampered");
        fsx::write_json_atomic(&path, &value).unwrap();
        assert!(
            store
                .cached::<Script>("draft", &key)
                .unwrap_err()
                .to_string()
                .contains("integrity")
        );
    }

    #[test]
    fn damaged_state_index_is_restored_from_the_previous_copy() {
        let g = guide();
        let (_tmp, layout, _) = setup(&g);
        let mut store = Store::open(&layout).unwrap();
        store.bump_revision("research").unwrap();
        store.bump_revision("research").unwrap();
        drop(store);
        std::fs::write(layout.state_file(), b"{ truncated").unwrap();

        let store = Store::open(&layout).unwrap();
        let note = store.restored.clone().expect("restore notice");
        assert!(note.contains("restored the previous copy"), "{note}");
        // The backup predates the last save, so it holds the first bump.
        assert_eq!(store.revision("research"), 1);
        let damaged = std::fs::read_dir(layout.meta())
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("state.json.damaged-")
            });
        assert!(damaged, "damaged index kept for inspection");
        drop(store);
        assert!(Store::open(&layout).unwrap().restored.is_none());
    }

    proptest! {
        // Every commit fsyncs several files; 32 cases keep this under a few seconds on macOS.
        #![proptest_config(ProptestConfig::with_cases(32))]
        #[test]
        fn arbitrary_commits_survive_lost_state(values in proptest::collection::vec("[^\\x00]{0,40}", 1..12)) {
            let g = guide();
            let (_tmp, layout, _) = setup(&g);
            let mut store = Store::open(&layout).unwrap();
            for (i, value) in values.iter().enumerate() {
                store.commit(&format!("step-{i}"), &hash(value).unwrap(), value, None).unwrap();
            }
            drop(store);
            std::fs::remove_file(layout.state_file()).unwrap();
            let mut recovered = Store::open(&layout).unwrap();
            for (i, value) in values.iter().enumerate() {
                let actual = recovered.cached::<String>(&format!("step-{i}"), &hash(value).unwrap()).unwrap();
                prop_assert_eq!(actual.as_ref(), Some(value));
            }
        }
    }

    #[derive(Debug)]
    struct Fake {
        calls: AtomicUsize,
        broken: bool,
        units: Vec<story::Unit>,
    }

    #[async_trait]
    impl TextProvider for Fake {
        fn id(&self) -> &'static str {
            "fixture"
        }
        fn billing(&self) -> Billing {
            Billing::Subscription
        }
        fn supports_images(&self) -> bool {
            bool::default()
        }
        fn resolve_model(&self, selector: &str) -> String {
            selector.into()
        }
        async fn generate(
            &self,
            req: &TextRequest,
            _: &str,
            _: &CancellationToken,
        ) -> Result<TextResponse, ProviderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let p = &req.schema["properties"];
            let name = [
                "beats",
                "candidates",
                "winner",
                "sections",
                "issues",
                "shots",
            ]
            .into_iter()
            .find(|k| p.get(*k).is_some());
            let value = match name {
                Some("beats") => {
                    json!({"storyline":"Ellen sacrifices herself to stop a visitor", "characters":["Thomas","Ellen","visitor"], "tone":"grave", "open_loop":"Can the town escape?", "emotional_peak":"Ellen stays", "omitted":[],
                "beats": script().sections.iter().map(|s| json!({"section":s.section,"facts":[s.text],"purpose":"retell"})).collect::<Vec<_>>()})
                }
                Some("candidates") => {
                    json!({"candidates":["The visitor arrives.","The town empties.","One window opens.","The clock runs down.","Only daylight can stop this."]})
                }
                Some("winner") => json!({"winner":0,"scores":[8,7,6,5,4],"reason":"specific"}),
                _ => match name {
                    Some("issues") => json!({"issues":[]}),
                    Some("shots") => {
                        json!({"shots": self.units.iter().map(|u| json!({"unit":u.id,"shot":if u.id % 2 == 0 {"wide"} else {"medium"},"location":"town","action":"The visitor crosses the street","mood":"grave","characters":[],"focus":[0.5,0.4],"hero":u.id == 0})).collect::<Vec<_>>()})
                    }
                    Some("sections") => {
                        let mut s = script();
                        if self.broken || req.prompt.contains("Write draft") {
                            s.sections[0].text.push_str(" Everything changes.");
                        }
                        serde_json::to_value(s).unwrap()
                    }
                    _ => return Err(ProviderError::Malformed("unexpected request schema".into())),
                },
            };
            Ok(TextResponse {
                value,
                usage: Usage::default(),
            })
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum DraftReply {
        Valid,
        Broken,
    }

    fn router(g: &Guide, reply: DraftReply) -> (Arc<Fake>, Router, ProvidersConfig) {
        let fake = Arc::new(Fake {
            calls: AtomicUsize::new(0),
            broken: reply == DraftReply::Broken,
            units: story::segment(&script(), g.manifest(), Quality::Standard),
        });
        let routing = [Tier::TextFast, Tier::TextSmart]
            .into_iter()
            .map(|t| (t, vec!["fixture:test".parse().unwrap()]))
            .collect();
        let r = Router::from_providers(
            vec![(Arc::clone(&fake) as Arc<dyn TextProvider>, 1)],
            routing,
        )
        .with_retry_backoff(std::time::Duration::ZERO);
        (
            fake,
            r,
            ProvidersConfig::suggested(retold_providers::config::Found {
                claude: false,
                codex: false,
            }),
        )
    }

    #[tokio::test]
    async fn whole_text_pipeline_repairs_and_resumes_without_provider_calls() {
        let g = guide();
        assert!(
            story::gates(&script(), g.manifest()).is_empty(),
            "{:?}",
            story::gates(&script(), g.manifest())
        );
        let (_tmp, layout, meta) = setup(&g);
        fsx::write_atomic(
            &layout.stage(Stage::Research).join("source-file.txt"),
            include_bytes!("fixtures/nosferatu-source.txt"),
        )
        .unwrap();
        let (fake, r, _cfg) = router(&g, DraftReply::Valid);
        let token = CancellationToken::new();
        let p = Pipeline {
            scope: "",
            layout: &layout,
            meta: &meta,
            guide: &g,
            router: &r,
            cancel: &token,
            progress: &|_, _| {},
            paid: None,
        };
        p.run(Until::Storyboard).await.unwrap();
        assert!(layout.review_page().exists());
        assert!(layout.stage(Stage::Script).join("checks-1.json").exists());
        let calls = fake.calls.load(Ordering::SeqCst);
        std::fs::remove_file(layout.state_file()).unwrap();
        p.run(Until::Storyboard).await.unwrap();
        assert_eq!(fake.calls.load(Ordering::SeqCst), calls);
        let storyboard: Value = serde_json::from_slice(
            &std::fs::read(layout.stage(Stage::Storyboard).join("storyboard.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            storyboard["units"].as_array().unwrap().len(),
            storyboard["direction"]["shots"].as_array().unwrap().len()
        );
        assert!(!layout.deliverables().exists());
        token.cancel();
        assert!(matches!(
            p.run(Until::Storyboard).await,
            Err(PipelineError::Cancelled)
        ));
        assert_eq!(fake.calls.load(Ordering::SeqCst), calls);
    }

    #[tokio::test]
    async fn exhausted_repairs_stop_before_storyboard() {
        let g = guide();
        let (_tmp, layout, meta) = setup(&g);
        fsx::write_atomic(
            &layout.stage(Stage::Research).join("source-file.txt"),
            include_bytes!("fixtures/nosferatu-source.txt"),
        )
        .unwrap();
        let (fake, r, _cfg) = router(&g, DraftReply::Broken);
        let token = CancellationToken::new();
        let p = Pipeline {
            scope: "",
            layout: &layout,
            meta: &meta,
            guide: &g,
            router: &r,
            cancel: &token,
            progress: &|_, _| {},
            paid: None,
        };
        assert!(matches!(
            p.run(Until::Storyboard).await,
            Err(PipelineError::Gates(_))
        ));
        assert_eq!(fake.calls.load(Ordering::SeqCst), 9);
        assert!(
            layout
                .stage(Stage::Script)
                .join("rejected-script.json")
                .exists()
        );
        assert!(!layout.stage(Stage::Storyboard).exists());
    }

    #[test]
    fn scripts_and_directions_fail_for_missing_or_invalid_content() {
        let g = guide();
        let mut s = script();
        s.sections.remove(3);
        assert!(
            story::gates(&s, g.manifest())
                .iter()
                .any(|v| v.gate == "structure")
        );
        let units = story::segment(&script(), g.manifest(), Quality::Standard);
        let plan = story::Plan {
            settings: vec![],
            props: vec![],
            iconic_images: vec![],
            storyline: "test".into(),
            characters: vec![],
            tone: "grave".into(),
            open_loop: "?".into(),
            emotional_peak: "test".into(),
            omitted: vec![],
            beats: vec![],
            outline: vec![],
            iconic: vec![],
            cast: vec![],
        };
        assert_ne!(
            story::direction_gates(&Direction { shots: vec![] }, &units, &plan, g.manifest()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn source_parsing_keeps_plot_and_episode_facts_and_unicode() {
        use retold_engine::research::*;
        assert_eq!(season_number("Show (season 12)"), Some(12));
        let plot = plot_sections(
            "Lead\n== Plot ==\nFacts\n=== Part two ===\nMore facts\n== Reception ==\nReviews",
            &["plot".into()],
        );
        assert!(plot.contains("More facts"));
        assert!(!plot.contains("Reviews"));
        assert_eq!(
            episode_summaries(
                "{{Episode list\n|Title=X\n|ShortSummary=He meets [[Ellen|his wife]].<ref>citation</ref>\n|LineColor=fff\n}}"
            ),
            "Episode 1: He meets his wife."
        );
        assert_eq!(truncate("वह आता है। फिर चला जाता है।", 12), "वह आता है।");
    }
}
