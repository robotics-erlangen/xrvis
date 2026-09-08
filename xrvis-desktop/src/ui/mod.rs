pub mod theme;
pub mod tokens;

use bevy::feathers::constants::fonts;
use bevy::prelude::*;
use bevy::text::FontSourceTemplate;
use bevy::ui_widgets::Activate;

pub fn ui_plugin(app: &mut App) {
    app.add_systems(PostUpdate, handle_immediate_activate);
}

/// Reference to the [Text] component for this UI element. Useful to avoid traversing the internal hierarchy of premade components like [FeathersCheckbox].
#[derive(Component, Clone, Copy)]
#[relationship_target(relationship = TextOfComponent)]
pub struct ComponentText(Entity);
#[derive(Component, FromTemplate, Clone, Copy)]
#[relationship(relationship_target = ComponentText)]
pub struct TextOfComponent(pub Entity);

/// Triggers [Activate] when added, useful for simulate an immediate click when spawning a button.
#[derive(Component, Clone, Copy, Default)]
pub struct ImmediateActivate;

fn handle_immediate_activate(
    mut commands: Commands,
    q_buttons: Query<Entity, (With<ImmediateActivate>, Added<ImmediateActivate>)>, // With<> is here because Added<> is slow on its own
) {
    for entity in q_buttons {
        commands.entity(entity).remove::<ImmediateActivate>();
        commands.trigger(Activate { entity });
    }
}

pub fn regular_text(text: impl Into<String>) -> impl Scene {
    bsn! {
        Text({text.into()}) TextFont { font: FontSourceTemplate::Handle(fonts::REGULAR) }
    }
}
