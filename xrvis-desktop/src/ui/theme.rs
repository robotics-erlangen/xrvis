use crate::ui::tokens::*;
use bevy::color::palettes::tailwind;
use bevy::feathers;
use bevy::feathers::palette;
use bevy::feathers::tokens::semantic as feathers_semantic;
use bevy::prelude::*;

pub fn extended_dark_theme() -> feathers::theme::ThemeProps {
    let mut feathers_dark = feathers::dark_theme::create_dark_theme();

    // Define every used semantic token, even the ones from feathers, to avoid breakages when updating
    feathers_dark.semantic_base.extend([
        (
            feathers_semantic::SURFACE_WINDOW,
            palette::GRAY_0.darker(0.1),
        ),
        (feathers_semantic::SURFACE_PANE_BODY, palette::GRAY_1),
        (feathers_semantic::TEXT_ON_ACCENT, palette::WHITE),
        (feathers_semantic::TEXT_DEFAULT, palette::LIGHT_GRAY_1),
        (semantic::BORDER_ACCENT, palette::ACCENT),
        (semantic::FILL_ACCENT_TINT, palette::ACCENT.with_alpha(0.1)),
        (semantic::FILL_ACCENT_WEAK, palette::ACCENT.with_alpha(0.5)),
        (semantic::GREEN, tailwind::GREEN_500.into()),
        (semantic::RED, tailwind::RED_500.into()),
        (semantic::YELLOW, Srgba::rgb_u8(250, 204, 21).into()),
        (semantic::YELLOW_1, Srgba::rgb_u8(223, 172, 51).into()),
        (semantic::BLUE, tailwind::BLUE_600.into()),
        (semantic::BLUE_1, tailwind::BLUE_600.darker(0.1).into()),
    ]);

    // Reuse feathers semantic tokens where possible
    feathers_dark.token_assignments.extend([
        (WINDOW_BG, feathers_semantic::SURFACE_WINDOW),
        (PANEL_BG, feathers_semantic::SURFACE_PANE_BODY),
        (TEXT_0, feathers_semantic::TEXT_ON_ACCENT),
        (TEXT_1, feathers_semantic::TEXT_DEFAULT),
        (SIDEBAR_EXPANDED_FIELD_BORDER, semantic::BORDER_ACCENT),
        (SIDEBAR_EXPANDED_FIELD_BG, semantic::FILL_ACCENT_TINT),
        (SIDEBAR_EXPANDED_FIELD_BUTTON, semantic::FILL_ACCENT_WEAK),
        (GREEN, semantic::GREEN),
        (RED, semantic::RED),
        (YELLOW, semantic::YELLOW),
        (YELLOW_1, semantic::YELLOW_1),
        (BLUE, semantic::BLUE),
        (BLUE_1, semantic::BLUE_1),
    ]);

    feathers_dark
}
