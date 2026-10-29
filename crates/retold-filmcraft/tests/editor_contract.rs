//! Exercises editor assembly against a JSON-lines server, never an actual editor/export.
#[cfg(test)]
mod tests {
    use retold_core::{Fps, Frames};
    use retold_filmcraft::{
        Client, Command, editor,
        timeline::{Animation, Clip, Easing, Effect, Key, Source, Timeline},
    };
    use serde_json::{Value, json};
    use std::{collections::BTreeMap, time::Duration};
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::TcpListener,
    };
    use tokio_util::sync::CancellationToken;
    fn test_fixture_timeline() -> Timeline {
        let motion = Effect {
            id: "motion".into(),
            params: BTreeMap::new(),
            animations: vec![Animation {
                param: "scale".into(),
                keys: vec![
                    Key {
                        offset: Frames(0),
                        value: json!(100),
                        easing: Easing::Linear,
                    },
                    Key {
                        offset: Frames(29),
                        value: json!(110),
                        easing: Easing::EaseOut,
                    },
                ],
            }],
        };
        Timeline {
            name: "en".into(),
            width: 1080,
            height: 1920,
            fps: Fps::new(30).unwrap(),
            duration: Frames(30),
            clips: vec![
                Clip {
                    track: "V1".into(),
                    start: Frames(0),
                    duration: Frames(30),
                    source: Source::Media {
                        path: "fixture.png".into(),
                    },
                    effects: vec![motion],
                    transition: None,
                    audio_gain_db: None,
                },
                Clip {
                    track: "V2".into(),
                    start: Frames(0),
                    duration: Frames(30),
                    source: Source::Text {
                        text: "Caption".into(),
                        font: "Helvetica Neue".into(),
                        font_style: "Bold".into(),
                        size: 70.0,
                        position: [540.0, 1500.0],
                        props: json!({}),
                        highlight: None,
                        emphasis: vec![],
                    },
                    effects: vec![],
                    transition: None,
                    audio_gain_db: None,
                },
            ],
            video_tracks: None,
            audio_tracks: None,
        }
    }

    #[tokio::test]
    async fn builds_and_verifies_every_clip_without_export() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(serve(listener, json!({})));
        let cancel = CancellationToken::new();
        let client = Client::connect(port, Duration::from_secs(2), &cancel)
            .await
            .unwrap();
        let timeline = test_fixture_timeline();
        let built = editor::build(&client, &timeline, &cancel).await.unwrap();
        assert_eq!(built.clip_ids, vec![json!(100), json!(101)]);
        assert!(matches!(
            client
                .send_command(
                    &Command::ExportMedia {
                        loudness_lufs: -14.0,
                        path: "never.mp4".into(),
                        sequence: built.sequence,
                        width: 1080,
                        height: 1920,
                        fps: timeline.fps
                    },
                    &cancel
                )
                .await,
            Err(retold_filmcraft::Error::Permission)
        ));
        drop(client);
        let commands = server.await.unwrap();
        assert!(commands.contains(&"effects.setInterpolation".into()));
        assert_eq!(
            commands.iter().filter(|c| *c == "effects.setParam").count(),
            2
        );
    }
    #[tokio::test]
    async fn native_finished_jobs_require_successful_result_metadata() {
        for (result, succeeds) in [
            (json!({"path":"saved.mp4","bytes":100,"frames":3}), true),
            (json!({"error":"encoder failed"}), false),
            (Value::Null, false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let server = tokio::spawn(serve(listener, result));
            let cancel = CancellationToken::new();
            let client = Client::connect(port, Duration::from_secs(2), &cancel)
                .await
                .unwrap();
            let outcome = editor::wait_job(&client, &json!(1), &cancel).await;
            assert_eq!(outcome.is_ok(), succeeds);
            drop(client);
            server.await.unwrap();
        }
    }
    async fn serve(listener: TcpListener, job_result: Value) -> Vec<String> {
        let (socket, _) = listener.accept().await.unwrap();
        let mut socket = BufReader::new(socket);
        let mut line = String::new();
        let mut commands = Vec::new();
        let mut clips = Vec::new();
        loop {
            line.clear();
            if socket.read_line(&mut line).await.unwrap() == 0 {
                break;
            }
            let req: Value = serde_json::from_str(&line).unwrap();
            let command = req["params"]["command"].as_str().unwrap();
            commands.push(command.to_string());
            assert_ne!(command, "file.exportMedia");
            let result = match command {
                "command.list" => serde_json::from_str::<Value>(include_str!(
                    "../../../docs/filmcraft-commands.json"
                ))
                .unwrap(),
                "jobs.list" => {
                    json!([{"id":1,"finished":true,"status":"Done in 0.1s","result":job_result}])
                }
                "file.newSequence" => {
                    assert_eq!(req["params"]["params"]["video"], 2);
                    assert_eq!(req["params"]["params"]["audio"], 1);
                    json!({"sequence":9})
                }
                _ => match command {
                    "file.import" => json!({"items":[7],"errors":[]}),
                    "timeline.place" => {
                        let clips_count = clips.len();
                        let id = clips_count + 100;
                        clips.push(id);
                        json!({"clips":[id]})
                    }
                    "graphics.newText" => {
                        let clips_count = clips.len();
                        let id = clips_count + 100;
                        clips.push(id);
                        assert_eq!(
                            req["params"]["params"]["track"], 1,
                            "graphics tracks use zero-based indices"
                        );
                        json!({"clip":id})
                    }
                    _ => match command {
                        "file.newAdjustmentLayer" => json!({"item":8}),
                        "sequence.inspect" => {
                            json!({"tracks":[{"clips":clips.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}]})
                        }
                        _ => json!({}),
                    },
                },
            };
            let response = json!({"id":req["id"],"ok":true,"result":result});
            socket
                .get_mut()
                .write_all(format!("{response}\n").as_bytes())
                .await
                .unwrap();
        }
        commands
    }
}

#[cfg(test)]
mod native {
    // Acceptance: requires the installed macOS editor; opens an empty private instance only
    #[cfg(feature = "acceptance")]
    #[tokio::test]
    async fn empty_private_session_has_the_required_registry() {
        let temp = tempfile::tempdir().unwrap();
        let cancel = tokio_util::sync::CancellationToken::new();
        let session = retold_filmcraft::session::Session::launch(
            std::path::Path::new("/Applications/FilmCraft.app/Contents/MacOS/FilmCraft"),
            temp.path(),
            &cancel,
        )
        .await
        .unwrap();
        let registry = session
            .client
            .send_command(&retold_filmcraft::Command::List, &cancel)
            .await
            .unwrap();
        retold_filmcraft::editor::verify_registry(&registry).unwrap();
        assert!(session.process.is_some());
        assert!(!session.resumed);
    }
}
