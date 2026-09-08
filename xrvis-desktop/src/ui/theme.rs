use crate::ui::tokens::*;
use bevy::color::palettes::tailwind;
use bevy::feathers;
use bevy::feathers::theme::ThemeProps;
use bevy::prelude::*;

pub fn extended_dark_theme() -> ThemeProps {
    let mut feathers_dark = feathers::dark_theme::create_dark_theme();

    feathers_dark
        .color
        .insert(PANEL_BG, feathers::palette::GRAY_1);
    feathers_dark
        .color
        .insert(PANEL_BORDER, feathers::palette::WARM_GRAY_1);

    feathers_dark.color.insert(TEXT_0, feathers::palette::WHITE);
    feathers_dark
        .color
        .insert(TEXT_1, feathers::palette::LIGHT_GRAY_1);

    feathers_dark
        .color
        .insert(SIDEBAR_EXPANDED_FIELD_BORDER, feathers::palette::ACCENT);
    feathers_dark.color.insert(
        SIDEBAR_EXPANDED_FIELD_BG,
        feathers::palette::ACCENT.with_alpha(0.1),
    );
    feathers_dark.color.insert(
        SIDEBAR_EXPANDED_FIELD_BUTTON,
        feathers::palette::ACCENT.with_alpha(0.5),
    );

    feathers_dark
        .color
        .insert(GREEN, tailwind::GREEN_500.into());
    feathers_dark.color.insert(RED, tailwind::RED_500.into());

    feathers_dark
}
