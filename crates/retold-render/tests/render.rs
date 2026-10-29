use image::{Rgba, RgbaImage};
use retold_render::{Compositor, Shot, Sprite};
use tokio_util::sync::CancellationToken;
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
fn shot(dir: &std::path::Path) -> Result<Shot> {
    let image = dir.join("image.png");
    RgbaImage::from_fn(32, 32, |x, y| {
        if x < 16 && y < 16 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 0, 0])
        }
    })
    .save(&image)?;
    let sprite = dir.join("mouth.png");
    RgbaImage::from_pixel(2, 2, Rgba([0, 255, 0, 255])).save(&sprite)?;
    Ok(Shot {
        image,
        depth: None,
        frames: 3,
        fps: 30,
        motion_x: 2.0,
        motion_y: 0.0,
        sprites: vec![Sprite {
            image: sprite,
            x: 4,
            y: 4,
            first_frame: 1,
            end_frame: 2,
        }],
        attached: vec![],
    })
}
#[test]
fn deterministic_frames_preserve_alpha_and_sprite_timing() -> Result {
    let dir = tempfile::tempdir()?;
    let spec = shot(dir.path())?;
    let compositor = Compositor::load(&spec)?;
    let frames = (0..3)
        .map(|n| compositor.frame(n))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert_eq!(frames[0].get_pixel(4, 4).0, [255, 0, 0, 255]);
    assert_eq!(frames[1].get_pixel(4, 4).0, [0, 255, 0, 255]);
    assert_eq!(frames[2].get_pixel(4, 4).0, [255, 0, 0, 255]);
    assert_eq!(frames[2].get_pixel(0, 0)[3], 0);
    for (index, frame) in frames.iter().enumerate() {
        assert_eq!(frame, &compositor.frame(u32::try_from(index)?)?);
    }
    let hashes =
        retold_render::png_sequence(&spec, &dir.path().join("png"), &CancellationToken::new())?;
    assert_eq!(
        hashes,
        frames
            .iter()
            .map(|f| blake3::hash(f.as_raw()).to_hex().to_string())
            .collect::<Vec<_>>()
    );
    Ok(())
}
#[tokio::test]
async fn real_encoder_cache_survives_missing_clip_and_changes_one_shot() -> Result {
    let dir = tempfile::tempdir()?;
    let spec = shot(dir.path())?;
    let cache = dir.path().join("cache");
    let cancel = CancellationToken::new();
    // The normal CI gate requires ffmpeg; no silent skip or substitute encoder.
    let first = retold_render::render(&spec, &cache, &cancel).await?;
    let modified = std::fs::metadata(&first.path)?.modified()?;
    let second = retold_render::render(&spec, &cache, &cancel).await?;
    assert_eq!(first.frame_hashes, second.frame_hashes);
    assert_eq!(modified, std::fs::metadata(&second.path)?.modified()?);
    std::fs::remove_file(&first.path)?;
    let repaired = retold_render::render(&spec, &cache, &cancel).await?;
    assert_eq!(first.frame_hashes, repaired.frame_hashes);
    let mut changed = spec.clone();
    changed.motion_x = 4.0;
    let other = retold_render::render(&changed, &cache, &cancel).await?;
    assert_ne!(other.key, first.key);
    assert!(first.path.exists());
    cancel.cancel();
    assert!(matches!(
        retold_render::render(&spec, &cache, &cancel).await,
        Err(retold_render::Error::Cancelled)
    ));
    Ok(())
}

#[tokio::test]
async fn cancellation_mid_encoder_leaves_no_completed_clip_and_resumes() -> Result {
    let dir = tempfile::tempdir()?;
    let mut spec = shot(dir.path())?;
    spec.frames = 36_000;
    let cache = dir.path().join("cache");
    let cancel = CancellationToken::new();
    let cloned = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cloned.cancel();
    });
    assert!(retold_render::render(&spec, &cache, &cancel).await.is_err());
    assert!(!std::fs::read_dir(&cache)?.any(|e| e.is_ok_and(|e| {
        e.path()
            .extension()
            .is_some_and(|s| s == "json" || s == "mov")
    })));
    spec.frames = 3;
    retold_render::render(&spec, &cache, &CancellationToken::new()).await?;
    Ok(())
}

#[test]
fn attached_sprites_move_with_the_camera() -> Result {
    let dir = tempfile::tempdir()?;
    let mut spec = shot(dir.path())?;
    let mark = dir.path().join("mark.png");
    RgbaImage::from_pixel(1, 1, Rgba([255, 0, 255, 255])).save(&mark)?;
    spec.sprites.clear();
    spec.motion_x = 2.0;
    spec.attached = vec![Sprite {
        image: mark,
        x: 3,
        y: 3,
        first_frame: 0,
        end_frame: 3,
    }];
    let compositor = Compositor::load(&spec)?;
    // Uniform depth: the whole image, and the mark with it, shifts 2 px by the last frame.
    assert_eq!(
        compositor.frame(0)?.get_pixel(3, 3),
        &Rgba([255, 0, 255, 255])
    );
    assert_eq!(
        compositor.frame(2)?.get_pixel(5, 3),
        &Rgba([255, 0, 255, 255])
    );
    assert_ne!(
        compositor.frame(2)?.get_pixel(3, 3),
        &Rgba([255, 0, 255, 255])
    );
    Ok(())
}
