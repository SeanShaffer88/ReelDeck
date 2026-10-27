//! Operator preferences for the shell.
//!
//! Everything here is presentation: density, accent, motion, contrast,
//! and how time is written. Nothing in this module changes what a recipe
//! records or what an export does — a preference that could alter an
//! output would be a decision, and decisions belong in the recipe where
//! they are recorded.
//!
//! Preferences serialize with serde and carry defaults, so a file written
//! by an older build loads without a migration step.

use serde::{Deserialize, Serialize};

/// Row height and control padding. Compact is the default: an
/// operations board shows more rows than a consumer editor needs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// Tight rows, 12px type — the default for long queues.
    Compact,
    /// Roomier rows for touch or low-vision use.
    Comfortable,
}

/// The restrained accent set. Each value is a hue name only; the exact
/// color comes from the theme tokens so contrast is tested in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    /// Warm amber — the studio default.
    Amber,
    /// Cool teal, for operators who work beside a broadcast monitor.
    Teal,
    /// Slate blue, the quietest of the three.
    Slate,
}

impl Accent {
    pub const ALL: [Accent; 3] = [Accent::Amber, Accent::Teal, Accent::Slate];

    pub fn label(self) -> &'static str {
        match self {
            Accent::Amber => "Amber",
            Accent::Teal => "Teal",
            Accent::Slate => "Slate",
        }
    }

    /// The CSS custom property name the shell sets for this accent.
    pub fn token(self) -> &'static str {
        match self {
            Accent::Amber => "--accent-amber",
            Accent::Teal => "--accent-teal",
            Accent::Slate => "--accent-slate",
        }
    }
}

/// How a time position is written next to the viewer and in the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeDisplay {
    /// `01:02:03:12` — hours:minutes:seconds:frames.
    Timecode,
    /// `01:02:03.500` — wall clock, to the millisecond.
    Clock,
    /// `1860912` — a frame number, for frame-accurate notes.
    Frames,
}

impl TimeDisplay {
    pub const ALL: [TimeDisplay; 3] = [TimeDisplay::Timecode, TimeDisplay::Clock, TimeDisplay::Frames];

    pub fn label(self) -> &'static str {
        match self {
            TimeDisplay::Timecode => "Timecode",
            TimeDisplay::Clock => "Clock",
            TimeDisplay::Frames => "Frames",
        }
    }
}

/// The shell's persisted preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePrefs {
    #[serde(default = "default_density")]
    pub density: Density,
    #[serde(default = "default_accent")]
    pub accent: Accent,
    #[serde(default = "default_time_display")]
    pub time_display: TimeDisplay,
    /// Honour the OS "reduce motion" setting in addition to this flag;
    /// the shell never animates because a preference was left unset.
    #[serde(default)]
    pub reduce_motion: bool,
    /// A higher-contrast palette; also applied automatically when the OS
    /// asks for `prefers-contrast: more`.
    #[serde(default)]
    pub high_contrast: bool,
    /// Draw title/action-safe guides over the picture.
    #[serde(default)]
    pub safe_areas: bool,
    /// Show the measurement lanes under the timeline by default.
    #[serde(default = "default_true")]
    pub timeline_graphs: bool,
    /// Collapse the right-hand inspector; the chain stays one click away.
    #[serde(default)]
    pub inspector_collapsed: bool,
}

fn default_density() -> Density {
    Density::Compact
}

fn default_accent() -> Accent {
    Accent::Amber
}

fn default_time_display() -> TimeDisplay {
    TimeDisplay::Timecode
}

fn default_true() -> bool {
    true
}

impl Default for WorkspacePrefs {
    fn default() -> Self {
        WorkspacePrefs {
            density: default_density(),
            accent: default_accent(),
            time_display: default_time_display(),
            reduce_motion: false,
            high_contrast: false,
            safe_areas: false,
            timeline_graphs: true,
            inspector_collapsed: false,
        }
    }
}

impl WorkspacePrefs {
    /// A preferences document from JSON, with defaults for every field
    /// not present — an older or hand-edited file must not lock the
    /// shell out of starting.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Reset to the defaults, for the preferences panel's reset control.
    pub fn reset(&mut self) {
        *self = WorkspacePrefs::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_conservative_shell() {
        let prefs = WorkspacePrefs::default();
        assert_eq!(prefs.density, Density::Compact);
        assert_eq!(prefs.accent, Accent::Amber);
        assert_eq!(prefs.time_display, TimeDisplay::Timecode);
        assert!(!prefs.reduce_motion);
        assert!(!prefs.high_contrast);
        assert!(prefs.timeline_graphs);
        assert!(!prefs.inspector_collapsed);
    }

    #[test]
    fn round_trip_preserves_prefs() {
        let mut prefs = WorkspacePrefs::default();
        prefs.accent = Accent::Teal;
        prefs.density = Density::Comfortable;
        prefs.time_display = TimeDisplay::Frames;
        prefs.safe_areas = true;
        let json = prefs.to_json().expect("serialize");
        assert_eq!(WorkspacePrefs::from_json(&json).expect("deserialize"), prefs);
    }

    #[test]
    fn partial_documents_load_with_defaults() {
        // An older build wrote only the density; everything else must
        // fall back rather than fail.
        let prefs = WorkspacePrefs::from_json(r#"{"density":"comfortable"}"#).expect("load");
        assert_eq!(prefs.density, Density::Comfortable);
        assert_eq!(prefs.accent, Accent::default_accent_for_test());
        assert!(prefs.timeline_graphs);

        // An empty object is the defaults.
        let prefs = WorkspacePrefs::from_json("{}").expect("load");
        assert_eq!(prefs, WorkspacePrefs::default());

        // Unknown fields from a newer build are ignored, not fatal.
        let prefs = WorkspacePrefs::from_json(r#"{"density":"compact","future":1}"#)
            .expect("unknown fields are tolerated");
        assert_eq!(prefs.density, Density::Compact);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut prefs = WorkspacePrefs {
            accent: Accent::Slate,
            high_contrast: true,
            ..Default::default()
        };
        prefs.reset();
        assert_eq!(prefs, WorkspacePrefs::default());
    }

    #[test]
    fn accents_name_their_tokens_apart() {
        let tokens: Vec<&str> = Accent::ALL.iter().map(|a| a.token()).collect();
        assert_eq!(tokens.len(), 3);
        let mut sorted = tokens.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 3, "each accent has its own token");
        for accent in Accent::ALL {
            assert!(!accent.label().is_empty());
        }
    }

    // Test-only helper so the assertion above reads as intent rather than
    // as a hard-coded duplicate of the default.
    impl Accent {
        fn default_accent_for_test() -> Accent {
            default_accent()
        }
    }
}
