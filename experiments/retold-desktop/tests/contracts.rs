//! Regression proof for draft isolation, cancellation, and existing-file preview boundaries.

use std::time::{Duration, Instant};

use retold_desktop::{
    media,
    model::{Edition, ReviewSession},
    work::Cancellation,
};

#[test]
fn palette_text_and_focus_pairs_keep_their_contrast_in_all_appearance_modes() {
    for light in [false, true] {
        for high_contrast in [false, true] {
            let p = retold_desktop::theme::Palette::with_contrast(light, high_contrast);
            assert!(contrast(p.text, p.control) >= 4.5);
            if high_contrast {
                assert!(contrast(p.control, p.canvas) >= 3.);
            }
            for background in [p.canvas, p.surface] {
                assert!(contrast(p.text, background) >= 4.5);
                assert!(contrast(p.secondary, background) >= 4.5);
                assert!(contrast(p.attention, background) >= 4.5);
                assert!(contrast(p.error, background) >= 4.5);
                assert!(contrast(p.success, background) >= 4.5);
                assert!(contrast(p.accent, background) >= 3.);
            }
        }
    }
}

fn contrast(foreground: gpui_kit::Hsla, background: gpui_kit::Hsla) -> f32 {
    let first = luminance(foreground);
    let second = luminance(background);
    (first.max(second) + 0.05) / (first.min(second) + 0.05)
}

fn luminance(color: gpui_kit::Hsla) -> f32 {
    let color = color.to_rgb();
    let linear = |channel: f32| {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

#[test]
fn edition_and_scene_changes_keep_independent_drafts_and_approvals() {
    let mut session = ReviewSession::default();
    session.save("English revision".into(), "English note".into());
    session.approve();
    session.edition = Edition::Hindi;
    assert!(!session.draft().approved);
    assert!(!session.draft().narration.contains("English revision"));
    session.save("हिन्दी 123".into(), "हिन्दी note".into());
    session.select(2);
    session.save("Other scene".into(), String::new());
    session.select(1);
    assert_eq!(session.draft().narration, "हिन्दी 123");
    session.edition = Edition::English;
    assert_eq!(session.draft().narration, "English revision");
    assert_eq!(session.draft().note, "English note");
    assert!(session.draft().approved);
}

#[test]
fn only_narration_changes_revoke_the_current_approval() {
    let mut session = ReviewSession::default();
    session.approve();
    let narration = session.draft().narration.clone();
    session.save(narration, "A reviewer note".into());
    assert!(session.draft().approved);
    session.save("Changed narration".into(), "A reviewer note".into());
    assert!(!session.draft().approved);
    assert_eq!(session.remaining(), session.scenes().len());
}

#[test]
fn cancellation_does_not_wait_for_the_slow_query_to_finish() -> anyhow::Result<()> {
    let cancellation = Cancellation::default();
    let worker_cancel = cancellation.clone();
    let worker = std::thread::spawn(move || worker_cancel.slow_query());
    let start = Instant::now();
    cancellation.cancel();
    let result = worker
        .join()
        .map_err(|_| anyhow::anyhow!("worker failed"))?;
    assert!(result.contains("stopped"));
    assert!(start.elapsed() < Duration::from_millis(250));
    Ok(())
}

#[test]
fn inspect_existing_audio_without_changing_source_bytes() -> anyhow::Result<()> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/retold-voice/tests/fixtures/speech.wav");
    let before = std::fs::read(&source)?;
    let inspection = media::inspect(&source, &Cancellation::default())?;
    assert!(inspection.media.audio);
    assert!(!inspection.media.video);
    assert!(inspection.media.duration > 0.0);
    assert!(inspection.frame.is_none());
    assert_eq!(std::fs::read(source)?, before);
    Ok(())
}

#[test]
fn decode_existing_image_is_bounded_and_bgra() -> anyhow::Result<()> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/retold-parallax/tests/fixtures/scene-3.png");
    let inspection = media::inspect(&source, &Cancellation::default())?;
    assert!(inspection.media.video);
    let frame = inspection
        .frame
        .ok_or_else(|| anyhow::anyhow!("Missing decoded frame"))?;
    assert_eq!(frame.len(), media::FRAME_BYTES);
    assert!(
        frame
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[0] != pixel[2])
    );
    Ok(())
}

#[test]
fn canceled_inspection_is_rejected_before_process_dispatch() -> anyhow::Result<()> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/retold-voice/tests/fixtures/speech.wav");
    let cancel = Cancellation::default();
    cancel.cancel();
    let error = media::inspect(&source, &cancel)
        .err()
        .ok_or_else(|| anyhow::anyhow!("Canceled inspection succeeded"))?;
    assert!(error.to_string().contains("canceled"));
    Ok(())
}

#[test]
fn worker_probe_refuses_existing_state() -> anyhow::Result<()> {
    let scratch = tempfile::tempdir()?;
    let sentinel = scratch.path().join("keep.txt");
    std::fs::write(&sentinel, "existing state")?;
    assert!(retold_desktop::worker_probe(scratch.path()).is_err());
    assert_eq!(std::fs::read_to_string(sentinel)?, "existing state");
    assert_eq!(std::fs::read_dir(scratch.path())?.count(), 1);
    Ok(())
}

#[test]
fn canceled_playback_never_launches_a_player() -> anyhow::Result<()> {
    let media = media::Media {
        path: "/missing/file.wav".into(),
        duration: 10.0,
        video: false,
        audio: true,
    };
    let cancel = Cancellation::default();
    cancel.cancel();
    media::play(&media, 0.0, &cancel, std::sync::Arc::default())
}

#[test]
fn stopping_existing_audio_waits_for_the_owned_player_to_exit() -> anyhow::Result<()> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/retold-voice/tests/fixtures/speech.wav");
    let media = media::inspect(&source, &Cancellation::default())?.media;
    let cancel = Cancellation::default();
    let control = cancel.clone();
    let worker =
        std::thread::spawn(move || media::play(&media, 0.0, &cancel, std::sync::Arc::default()));
    std::thread::sleep(Duration::from_millis(50));
    let started = Instant::now();
    control.cancel();
    worker
        .join()
        .map_err(|_| anyhow::anyhow!("player worker failed"))??;
    assert!(started.elapsed() < Duration::from_secs(1));
    Ok(())
}

#[test]
fn worker_process_runs_display_free_without_dispatch() -> anyhow::Result<()> {
    let scratch = tempfile::tempdir()?;
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_retold-studio"))
        .arg("--worker-probe")
        .arg(scratch.path())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&result.stdout)?;
    assert_eq!(payload["worker_probe"], "passed");
    assert_eq!(payload["dispatched"], false);
    assert_eq!(payload["protocol"], retold_app::protocol::PROTOCOL_VERSION);
    Ok(())
}
