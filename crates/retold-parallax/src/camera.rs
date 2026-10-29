//! Bounded perspective moves; camera space uses a near plane of 0.5 and far plane of 2.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CameraMove {
    #[default]
    PushIn,
    Orbit,
    Pedestal,
    DollyZoom,
}
#[derive(Debug, Clone, Copy)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f64,
    pub focal_scale: f64,
}
impl CameraMove {
    #[must_use]
    pub fn pose(self, progress: f64, strength: f64) -> Pose {
        let t = progress.clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        let a = t * strength;
        let mut pose = Pose {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw: 0.0,
            focal_scale: 1.0,
        };
        match self {
            Self::PushIn => pose.z = 0.10 * a,
            Self::Orbit => {
                pose.x = 0.08 * a;
                pose.yaw = -(pose.x / 1.25).atan();
            }
            Self::Pedestal => pose.y = 0.055 * a,
            Self::DollyZoom => {
                pose.z = 0.10 * a;
                pose.focal_scale = 1.0 - pose.z / 1.25;
            }
        }
        pose
    }
}
#[must_use]
pub fn project(x: u32, y: u32, depth: f32, size: (u32, u32), pose: Pose) -> (f64, f64, f64) {
    let cx = f64::from(size.0 - 1) / 2.0;
    let cy = f64::from(size.1 - 1) / 2.0;
    let focal = f64::from(size.0.max(size.1)) * 0.8;
    let z = 2.0 - 1.5 * f64::from(depth);
    let wx = (f64::from(x) - cx) * z / focal - pose.x;
    let wy = (f64::from(y) - cy) * z / focal - pose.y;
    let wz = z - pose.z;
    let (sin, cos) = pose.yaw.sin_cos();
    let rx = cos * wx - sin * wz;
    let rz = sin * wx + cos * wz;
    (
        cx + focal * pose.focal_scale * rx / rz,
        cy + focal * pose.focal_scale * wy / rz,
        rz,
    )
}
