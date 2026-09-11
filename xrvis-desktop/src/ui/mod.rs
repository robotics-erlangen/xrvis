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

/// Inserts a child entity sorted by its [ComponentText].
pub fn insert_child_sorted(child: Entity) -> impl EntityCommand {
    move |mut parent: EntityWorldMut| -> Result<(), BevyError> {
        let world = parent.world();

        let new_text = world.get::<ComponentText>(child).and_then(|t_ref| {
            world
                .get::<Text>(*t_ref.collection())
                .map(|t| t.0.to_lowercase())
        });

        let index = parent
            .get::<Children>()
            .into_iter()
            .flatten()
            .enumerate()
            .find_map(|(i, e)| {
                let this_text = world.get::<ComponentText>(*e).and_then(|t_ref| {
                    world
                        .get::<Text>(*t_ref.collection())
                        .map(|t| t.0.to_lowercase())
                });
                (new_text < this_text).then_some(i)
            });

        if let Some(i) = index {
            parent.insert_child(i, child);
        } else {
            parent.add_child(child);
        }

        Ok(())
    }
}

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
