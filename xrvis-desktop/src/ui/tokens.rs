use bevy::feathers::theme::ThemeToken;

/// Semantic tokens to complement [feathers::tokens::semantic](bevy::feathers::tokens::semantic)
pub mod semantic {
    use bevy::feathers::theme::SemanticToken;

    pub const BORDER_ACCENT: SemanticToken = SemanticToken::new_static("xrvis.border.accent");
    pub const FILL_ACCENT_TINT: SemanticToken = SemanticToken::new_static("xrvis.fill.accent.tint");
    pub const FILL_ACCENT_WEAK: SemanticToken = SemanticToken::new_static("xrvis.fill.accent.weak");

    pub const GREEN: SemanticToken = SemanticToken::new_static("xrvis.green");
    pub const RED: SemanticToken = SemanticToken::new_static("xrvis.red");
    pub const YELLOW: SemanticToken = SemanticToken::new_static("xrvis.yellow");
    pub const YELLOW_1: SemanticToken = SemanticToken::new_static("xrvis.yellow.1");
    pub const BLUE: SemanticToken = SemanticToken::new_static("xrvis.blue");
    pub const BLUE_1: SemanticToken = SemanticToken::new_static("xrvis.blue.1");
}

// Theme tokens are fully separate from feathers

pub const WINDOW_BG: ThemeToken = ThemeToken::new_static("xrvis.window.bg");
pub const PANEL_BG: ThemeToken = ThemeToken::new_static("xrvis.pabel.bg");

pub const TEXT_0: ThemeToken = ThemeToken::new_static("xrvis.text.0");
pub const TEXT_1: ThemeToken = ThemeToken::new_static("xrvis.text.1");

pub const SIDEBAR_EXPANDED_FIELD_BORDER: ThemeToken =
    ThemeToken::new_static("xrvis.sidebar.expanded.border");

pub const SIDEBAR_EXPANDED_FIELD_BG: ThemeToken =
    ThemeToken::new_static("xrvis.sidebar.expanded.bg");

pub const SIDEBAR_EXPANDED_FIELD_BUTTON: ThemeToken =
    ThemeToken::new_static("xrvis.sidebar.expanded.button");

pub const GREEN: ThemeToken = ThemeToken::new_static("xrvis.green");
pub const RED: ThemeToken = ThemeToken::new_static("xrvis.red");
pub const YELLOW: ThemeToken = ThemeToken::new_static("xrvis.yellow");
pub const YELLOW_1: ThemeToken = ThemeToken::new_static("xrvis.yellow.1");
pub const BLUE: ThemeToken = ThemeToken::new_static("xrvis.blue");
pub const BLUE_1: ThemeToken = ThemeToken::new_static("xrvis.blue.1");
