//! Embedded artwork and Kit icons; no checkout paths or network image loads.

use std::borrow::Cow;

use gpui_kit::{AssetSource, SharedString};

gpui_kit::assets::icon_assets!(
    AppIcons,
    [
        House,
        Film,
        Layers,
        Activity,
        Server,
        Wallet,
        Settings,
        CircleQuestionMark,
        Search,
        ChevronLeft,
        CircleStop,
        Ellipsis,
        Plus,
        SkipBack,
        SkipForward,
        Play,
        Pause,
        Sparkles,
        FolderOpen,
        Sun,
        Moon,
        Clapperboard,
        Music,
        Check,
        ChevronRight,
        RefreshCw,
        ChevronDown,
        Bookmark,
        Package,
        Lightbulb,
        Info,
        MessageSquare,
        AudioWaveform,
        MonitorPlay,
        Image,
        Captions,
        FileText
    ]
);

#[derive(Debug)]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if path == "coastline.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/coastline.svg"
            ))));
        }
        if path == "logo.png" {
            return Ok(Some(Cow::Borrowed(include_bytes!("../assets/logo.png"))));
        }
        if let Some(bytes) = AppIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(AppIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
