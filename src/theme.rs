//! OpenCode Desktop design tokens and small presentation helpers.
//!
//! Values come from the official frontend at `anomalyco/opencode`
//! `packages/ui/src/styles/tokens/theme.css` (dark palette) and the Inbox
//! components. Keep this module limited to tokens and pure formatting so UI
//! code stays readable and the reference is easy to update.

use crate::api::Project;
use gpui_kit::{Hsla, rgb, rgba};

pub const DEEP: u32 = 0x080808;
pub const BASE: u32 = 0x161616;
pub const LAYER_01: u32 = 0x242424;
pub const LAYER_02: u32 = 0x2e2e2e;

pub const TEXT_BASE: u32 = 0xfafafa;
pub const TEXT_MUTED: u32 = 0xaeaeae;
pub const TEXT_FAINT: u32 = 0x808080;
pub const ICON_MUTED: u32 = 0x808080;

pub const ACCENT: u32 = 0x3b5cf6;
pub const DANGER: u32 = 0xf1484f;
pub const WARNING: u32 = 0xf2cf76;
pub const SUCCESS: u32 = 0x49c970;

pub const HOVER: u32 = 0xffffff0f; // 6% white
pub const PRESSED: u32 = 0xffffff1a; // 10% white
pub const BORDER_MUTED: u32 = 0xffffff14; // 8% white
pub const BORDER_BASE: u32 = 0xffffff1a; // 10% white

pub fn deep() -> Hsla {
    rgb(DEEP).into()
}

pub fn base() -> Hsla {
    rgb(BASE).into()
}

pub fn layer_01() -> Hsla {
    rgb(LAYER_01).into()
}

pub fn layer_02() -> Hsla {
    rgb(LAYER_02).into()
}

pub fn text_base() -> Hsla {
    rgb(TEXT_BASE).into()
}

pub fn text_muted() -> Hsla {
    rgb(TEXT_MUTED).into()
}

pub fn text_faint() -> Hsla {
    rgb(TEXT_FAINT).into()
}

pub fn icon_muted() -> Hsla {
    rgb(ICON_MUTED).into()
}

pub fn hover() -> Hsla {
    rgba(HOVER).into()
}

pub fn pressed() -> Hsla {
    rgba(PRESSED).into()
}

pub fn border_muted() -> Hsla {
    rgba(BORDER_MUTED).into()
}

pub fn border_base() -> Hsla {
    rgba(BORDER_BASE).into()
}

pub fn accent() -> Hsla {
    rgb(ACCENT).into()
}

pub fn danger() -> Hsla {
    rgb(DANGER).into()
}

pub fn warning() -> Hsla {
    rgb(WARNING).into()
}

#[derive(Clone, Copy)]
pub struct ProjectTone {
    pub background: u32,
    pub border: u32,
}

/// Project avatar colours for the dark scheme. The official palette assigns a
/// fixed colour to each project; `gray` is the default for projects without a
/// configured icon colour.
pub fn project_tone(project: Option<&Project>) -> ProjectTone {
    let key = project
        .and_then(|project| project.icon.as_ref())
        .and_then(|icon| icon.color.as_deref())
        .unwrap_or("gray");

    match key {
        "orange" => ProjectTone {
            background: 0x723d22,
            border: 0xff8648,
        },
        "yellow" => ProjectTone {
            background: 0x68552b,
            border: 0xe7af36,
        },
        "cyan" | "mint" => ProjectTone {
            background: 0x005a6e,
            border: 0x0096b8,
        },
        "green" | "lime" => ProjectTone {
            background: 0x196130,
            border: 0x49c970,
        },
        "red" => ProjectTone {
            background: 0x7a1f23,
            border: 0xd92e3c,
        },
        "pink" => ProjectTone {
            background: 0x8c2d61,
            border: 0xe4429e,
        },
        "blue" => ProjectTone {
            background: 0x263fa9,
            border: 0x7698fd,
        },
        "purple" => ProjectTone {
            background: 0x361f83,
            border: 0x7152f4,
        },
        _ => ProjectTone {
            background: 0x5c5c5c,
            border: 0xaeaeae,
        },
    }
}

pub fn project_name(project: Option<&Project>, directory: &str) -> String {
    if let Some(name) = project.and_then(|project| project.name.as_deref())
        && !name.trim().is_empty()
    {
        return name.to_owned();
    }

    directory
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(directory)
        .to_owned()
}

pub fn initial(name: &str) -> String {
    let first = name.chars().next().unwrap_or('?');
    first.to_uppercase().collect()
}

pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as f64)
        .unwrap_or(0.)
}

/// Official Inbox age text: `now`, `41m`, `2h`, `3d`. A working session in its
/// first minute shows seconds instead.
pub fn age_label(now: f64, since: Option<f64>, working: bool) -> String {
    let Some(since) = since else {
        return String::new();
    };
    let seconds = ((now - since) / 1000.).max(0.).floor() as u64;

    if working && seconds < 60 {
        return format!("{seconds}s");
    }
    if seconds < 60 {
        return "now".into();
    }
    if seconds < 3_600 {
        return format!("{}m", seconds / 60);
    }
    if seconds < 86_400 {
        return format!("{}h", seconds / 3_600);
    }
    format!("{}d", seconds / 86_400)
}

pub const ICON_GRID_PLUS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M13.9948 11.668H9.32812M11.6641 9.33203V13.9987M6.66667 9.33203V13.9987H2V9.33203H6.66667ZM6.66667 2V6.66667H2V2H6.66667ZM13.9948 2V6.66667H9.32812V2H13.9948Z" stroke="currentColor" stroke-miterlimit="10" stroke-linecap="square"/></svg>"#;

pub const ICON_SEARCH: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M14 14L10.3454 10.3454M6.88889 11.7778C9.58889 11.7778 11.7778 9.58889 11.7778 6.88889C11.7778 4.18889 9.58889 2 6.88889 2C4.18889 2 2 4.18889 2 6.88889C2 9.58889 4.18889 11.7778 6.88889 11.7778Z" stroke="currentColor"/></svg>"#;

pub const ICON_TRIANGLE_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M4 5.5H12L8 11.5L4 5.5Z" fill="currentColor"/></svg>"#;

pub const ICON_ACTIVITY_RING: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><circle cx="8" cy="8" r="5.5" stroke="currentColor" stroke-opacity="0.25"/><path d="M8 2.5C11.0376 2.5 13.5 4.96243 13.5 8" stroke="currentColor" stroke-linecap="round"/></svg>"#;

pub const ICON_ELLIPSIS: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M5 8C5 8.55228 4.55228 9 4 9C3.44772 9 3 8.55228 3 8C3 7.44772 3.44772 7 4 7C4.55228 7 5 7.44772 5 8Z" fill="currentColor"/><path d="M9 8C9 8.55228 8.55228 9 8 9C7.44772 9 7 8.55228 7 8C7 7.44772 7.44772 7 8 7C8.55228 7 9 7.44772 9 8Z" fill="currentColor"/><path d="M13 8C13 8.55228 12.5523 9 12 9C11.4477 9 11 8.55228 11 8C11 7.44772 11.4477 7 12 7C12.5523 7 13 7.44772 13 8Z" fill="currentColor"/></svg>"#;

pub const ICON_ARROW_UP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M8 12.5V3.5M11.5 7L8 3.5L4.5 7" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

pub const ICON_STOP: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><rect x="3.5" y="3.5" width="9" height="9" rx="1" fill="currentColor"/></svg>"#;

pub const ICON_ARCHIVE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M2.5 5.5V13.5H13.5V5.5M2.5 5.5H1.5V2.5H14.5V5.5H13.5M2.5 5.5H13.5M6.5 7.5H9.5" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

pub const ICON_CHECK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M3.53613 8.17857L6.39328 11.75L12.4647 4.25" stroke="currentColor"/></svg>"#;

pub const ICON_CHEVRON_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M5 6.5L8 9.5L11 6.5" stroke="currentColor"/></svg>"#;

pub const ICON_WARNING: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M8 2L14.5 13.5H1.5L8 2Z" stroke="currentColor"/><path d="M8 6V9.5" stroke="currentColor" stroke-linecap="round"/><path d="M8 11.5V11.6" stroke="currentColor" stroke-linecap="round"/></svg>"#;

pub const ICON_TERMINAL: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M1.5 2.5H14.5V13.5H1.5V2.5Z" stroke="currentColor" stroke-linejoin="round"/><path d="M3.5 5L5.5 7L3.5 9M7 9H10" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

pub const ICON_EYE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M1.5 8C2.5 6.5 4.5 4 8 4C11.5 4 13.5 6.5 14.5 8C13.5 9.5 11.5 12 8 12C4.5 12 2.5 9.5 1.5 8Z" stroke-linecap="round" stroke-linejoin="round" stroke="currentColor"/><path d="M9.5 8C9.5 8.82843 8.82843 9.5 8 9.5C7.17157 9.5 6.5 8.82843 6.5 8C6.5 7.17157 7.17157 6.5 8 6.5C8.82843 6.5 9.5 7.17157 9.5 8Z" fill="currentColor"/></svg>"#;

pub const ICON_FILE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M3.5 1.5H9.5L12.5 4.5V14.5H3.5V1.5Z" stroke-linecap="round" stroke-linejoin="round" stroke="currentColor"/><path d="M9.5 1.5V4.5H12.5M5.5 8H10.5M5.5 11H10.5" stroke-linecap="round" stroke-linejoin="round" stroke="currentColor"/></svg>"#;

pub const ICON_DIFF: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none"><path d="M4.5 6.5H8M8 6.5H11.5M8 6.5V3M8 6.5V10" stroke-linecap="round" stroke-linejoin="round" stroke="currentColor"/><path d="M4.5 12.5H11.5" stroke-linecap="round" stroke-linejoin="round" stroke="currentColor"/></svg>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_age_matches_the_official_compaction() {
        let now = 10_000_000.0;
        assert_eq!(age_label(now, Some(now - 5_000.0), false), "now");
        assert_eq!(age_label(now, Some(now - 5_000.0), true), "5s");
        assert_eq!(age_label(now, Some(now - 41.0 * 60_000.0), false), "41m");
        assert_eq!(age_label(now, Some(now - 2.0 * 3_600_000.0), false), "2h");
        assert_eq!(age_label(now, Some(now - 3.0 * 86_400_000.0), false), "3d");
        assert_eq!(age_label(now, None, false), "");
    }

    #[test]
    fn project_names_fall_back_to_the_directory() {
        assert_eq!(project_name(None, "/home/zce/Coding/opencode/"), "opencode");
    }
}
