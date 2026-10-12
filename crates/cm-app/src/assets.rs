//! Icons: GPUI Kit's default component set, the extra Lucide icons the design uses, and our logo.

use std::borrow::Cow;

use gpui_kit::assets::{Assets, IconName, icon_assets};
use gpui_kit::{AssetSource, Result, SharedString};

icon_assets!(
    ExtraIcons,
    [
        Sparkles,
        Briefcase,
        House,
        Code,
        Rocket,
        Star,
        Heart,
        Globe,
        Settings,
        SquareTerminal,
        Folder,
        Play,
        Square,
        ExternalLink,
        Ellipsis,
        CircleCheck,
        TriangleAlert,
        RefreshCw,
        Link,
        User,
        Keyboard,
        Package,
        Info,
        AppWindow,
        Power,
        Plug,
        Search,
        Plus,
        Check,
        X,
        Minus,
        Copy,
        ChevronRight,
    ]
);

pub const LOGO: &str = "brand/logo.svg";
const LOGO_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><rect x="3" y="7" width="12" height="12" rx="3.5"/><rect x="9" y="3" width="12" height="12" rx="3.5" fill="currentColor" fill-opacity=".3"/></svg>"#;

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == LOGO {
            return Ok(Some(Cow::Borrowed(LOGO_SVG)));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

/// The Lucide icon for each profile icon choice.
pub fn profile_icon(icon: cm_core::ProfileIcon) -> IconName {
    use cm_core::ProfileIcon::*;
    match icon {
        Sparkle => IconName::Sparkles,
        Briefcase => IconName::Briefcase,
        Home => IconName::House,
        Code => IconName::Code,
        Rocket => IconName::Rocket,
        Star => IconName::Star,
        Heart => IconName::Heart,
        Globe => IconName::Globe,
    }
}
