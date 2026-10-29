use image::{Rgba, RgbaImage};
use retold_parallax::{
    camera::{CameraMove, Pose},
    depth::DepthMap,
    dibr, inpaint,
    render::{self, Fallback, Quality, Request, Scene},
};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn broad_border_and_interior_reveals_are_rejected_instead_of_smeared() -> Result {
    let image = RgbaImage::from_pixel(100, 100, Rgba([90, 140, 190, 255]));
    let depth = DepthMap::new(100, 100, vec![0.5; 10_000])?;
    for interior in [false, true] {
        let mut warped = dibr::warp(&image, &depth, CameraMove::PushIn.pose(0.0, 0.5))?;
        for y in 0..100 {
            for x in 0..100 {
                let hole = if interior {
                    (45..55).contains(&x) && (40..60).contains(&y)
                } else {
                    x < 8
                };
                if hole {
                    warped.holes[(y * 100 + x) as usize] = true;
                    warped.image.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                }
            }
        }
        inpaint::fill(&mut warped);
        assert!(warped.audit.unresolved_holes > 0);
        assert!(warped.audit.fill_radius_ratio > 0.004);
        assert!(!Quality::default().passes(warped.audit));
        let unfilled = if interior { (50, 50) } else { (0, 50) };
        assert_eq!(warped.image.get_pixel(unfilled.0, unfilled.1)[3], 0);
    }
    Ok(())
}

#[test]
fn framing_is_constant_through_motion_and_flat_preserves_original() -> Result {
    let request = fixture(0);
    let scene = Scene::load(&request)?;
    let original = image::open(&request.image)?.into_rgba8();
    for motion in [
        CameraMove::PushIn,
        CameraMove::Orbit,
        CameraMove::Pedestal,
        CameraMove::DollyZoom,
    ] {
        let (_, start) = scene.frame(motion, 0.0, 0.35)?;
        assert!((1.0..=1.2).contains(&start.framing_scale));
        for step in 0..=24 {
            let (_, audit) = scene.frame(motion, f64::from(step) / 24.0, 0.35)?;
            assert_eq!(audit.framing_scale, start.framing_scale);
            assert!(
                Quality::default().passes(audit),
                "{motion:?} at {step}: {audit:?}"
            );
        }
        assert_eq!(scene.frame(motion, 1.0, 0.0)?.0, original);
    }
    Ok(())
}
fn fixture(index: usize) -> Request {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    Request {
        image: root.join(format!("scene-{index}.png")),
        depth: root.join(format!("scene-{index}.rgd")),
        background: None,
        motion: CameraMove::Orbit,
        strength: 0.35,
        frames: 12,
        fps: 12,
        attached: vec![],
    }
}
#[test]
fn five_recorded_real_depth_maps_and_all_moves_meet_quality_thresholds() -> Result {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let goldens: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("goldens.json"))?)?;
    assert_eq!(goldens.as_array().ok_or("missing goldens")?.len(), 5);
    for (i, golden) in goldens
        .as_array()
        .ok_or("missing goldens")?
        .iter()
        .enumerate()
    {
        let request = fixture(i);
        let bytes = std::fs::read(&request.depth)?;
        assert_eq!(
            blake3::hash(&bytes).to_hex().as_str(),
            golden["depth_hash"].as_str().ok_or("missing hash")?
        );
        let depth = DepthMap::load(&request.depth)?;
        assert_eq!(
            (depth.width, depth.height),
            image::image_dimensions(&request.image)?
        );
        let scene = Scene::load(&request)?;
        for motion in [
            CameraMove::PushIn,
            CameraMove::Orbit,
            CameraMove::Pedestal,
            CameraMove::DollyZoom,
        ] {
            for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let (frame, audit) = scene.frame(motion, progress, 0.35)?;
                assert!(
                    Quality::default().passes(audit),
                    "fixture {i}, {motion:?}, {progress}: {audit:?}"
                );
                assert_eq!(frame, scene.frame(motion, progress, 0.35)?.0);
            }
        }
    }
    Ok(())
}
#[test]
fn z_ordered_bilinear_splats_preserve_identity_and_foreground_occlusion() -> Result {
    let image = RgbaImage::from_fn(8, 4, |x, _| {
        if x < 4 {
            Rgba([0, 0, 255, 255])
        } else {
            Rgba([255, 0, 0, 255])
        }
    });
    let depth = DepthMap::new(
        8,
        4,
        (0..32).map(|i| if i % 8 < 4 { 0.0 } else { 1.0 }).collect(),
    )?;
    let mut identity = dibr::warp(&image, &depth, CameraMove::PushIn.pose(0.0, 0.35))?;
    assert_eq!(identity.image, image);
    assert_eq!(identity.audit.unresolved_holes, 0);
    let translated = Pose {
        x: 0.1,
        y: 0.0,
        z: 0.0,
        yaw: 0.0,
        focal_scale: 1.0,
    };
    let warped = dibr::warp(&image, &depth, translated)?;
    // Near red splats win the overlap with the blue background.
    assert_eq!(warped.image.get_pixel(3, 1)[0], 255);
    identity.image.put_pixel(4, 1, Rgba([0, 0, 0, 0]));
    identity.holes[12] = true;
    identity.depths[12] = 1.0;
    identity.depths[11] = 0.0;
    inpaint::fill(&mut identity);
    assert_eq!(identity.image.get_pixel(4, 1).0, [0, 0, 255, 255]);
    Ok(())
}
#[tokio::test]
async fn real_encoder_cache_repair_and_quality_fallback_are_durable() -> Result {
    let directory = tempfile::tempdir()?;
    let request = fixture(0);
    let token = CancellationToken::new();
    let first = render::render(&request, directory.path(), &token).await?;
    assert!(Quality::default().passes(first.audit));
    let modified = std::fs::metadata(&first.path)?.modified()?;
    let second = render::render(&request, directory.path(), &token).await?;
    assert_eq!(first.frame_hashes, second.frame_hashes);
    assert_eq!(modified, std::fs::metadata(&second.path)?.modified()?);
    std::fs::write(&first.path, b"corrupt")?;
    let repaired = render::render(&request, directory.path(), &token).await?;
    assert_eq!(first.frame_hashes, repaired.frame_hashes);
    let mut forced = fixture(0);
    forced.motion = CameraMove::Pedestal;
    forced.strength = 1.0;
    let fallback = render::render_with_quality(
        &forced,
        directory.path(),
        Quality {
            max_hole_ratio: 0.0,
            max_stretched_fraction: 0.0,
        },
        &token,
    )
    .await?;
    assert_eq!(fallback.fallback, Fallback::Flat);
    assert_eq!(fallback.rejected.len(), 2);
    assert!(fallback.frame_hashes.windows(2).all(|v| v[0] == v[1]));
    Ok(())
}
#[tokio::test]
async fn cancelled_partial_render_never_activates_a_clip() -> Result {
    let directory = tempfile::tempdir()?;
    let mut request = fixture(1);
    request.frames = 36_000;
    let token = CancellationToken::new();
    let cloned = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cloned.cancel();
    });
    assert!(
        render::render(&request, directory.path(), &token)
            .await
            .is_err()
    );
    assert!(
        !std::fs::read_dir(directory.path())?.any(|e| e.is_ok_and(|e| e
            .path()
            .extension()
            .is_some_and(|s| s == "mov" || s == "json")))
    );
    request.frames = 12;
    render::render(&request, directory.path(), &CancellationToken::new()).await?;
    Ok(())
}
#[test]
fn malformed_depth_and_requests_fail_closed() -> Result {
    assert!(DepthMap::new(1, 1, vec![f32::NAN]).is_err());
    assert!(DepthMap::new(0, 1, vec![]).is_err());
    let mut request = fixture(0);
    request.strength = f64::NAN;
    assert!(Scene::load(&request).is_err());
    let dir = tempfile::tempdir()?;
    let path: PathBuf = dir.path().join("corrupt.rgd");
    std::fs::write(&path, b"wrong")?;
    assert!(DepthMap::load(&path).is_err());
    Ok(())
}

#[test]
fn killed_encoder_helper() -> Result {
    let Ok(root) = std::env::var("RETOLD_PARALLAX_KILL") else {
        return Ok(());
    };
    let root = Path::new(&root);
    tokio::runtime::Runtime::new()?.block_on(async {
        render::render(&fixture(0), root, &CancellationToken::new()).await?;
        std::fs::write(root.join("ready"), b"ready")?;
        let mut second = fixture(1);
        second.frames = 600;
        render::render(&second, root, &CancellationToken::new()).await?;
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}
#[tokio::test]
async fn real_kill_during_second_shot_resumes_without_rendering_finished_first() -> Result {
    let root = tempfile::tempdir()?;
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "killed_encoder_helper"])
        .env("RETOLD_PARALLAX_KILL", root.path())
        .stdout(std::process::Stdio::null())
        .spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let partial = std::fs::read_dir(root.path())?
            .filter_map(std::result::Result::ok)
            .any(|e| {
                e.file_name().to_string_lossy().contains(".partial-")
                    && e.metadata().is_ok_and(|m| m.len() > 10_000)
            });
        if root.path().join("ready").exists() && partial {
            break;
        }
        if std::time::Instant::now() > deadline {
            child.kill()?;
            child.wait()?;
            return Err("second encoder did not become active".into());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    child.kill()?;
    child.wait()?;
    let first_record = std::fs::read_dir(root.path())?
        .filter_map(std::result::Result::ok)
        .find(|e| e.path().extension().is_some_and(|e| e == "json"))
        .ok_or("first record missing")?
        .path();
    let saved: render::Rendered = serde_json::from_slice(&std::fs::read(first_record)?)?;
    let modified = std::fs::metadata(&saved.path)?.modified()?;
    let first = render::render(&fixture(0), root.path(), &CancellationToken::new()).await?;
    assert_eq!(first.frame_hashes, saved.frame_hashes);
    assert_eq!(modified, std::fs::metadata(first.path)?.modified()?);
    let mut second = fixture(1);
    second.frames = 600;
    let resumed = render::render(&second, root.path(), &CancellationToken::new()).await?;
    assert_eq!(resumed.frame_hashes.len(), 600);
    Ok(())
}
