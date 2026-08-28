use crate::field::Field;
use crate::field::hosts::{GameStateHost, HostConnection, UpdateHostDataSystemSet};
use crate::proto;
use bevy::prelude::*;

/// Manages [GameState] components
pub fn game_state_plugin(app: &mut App) {
    app.add_systems(
        PreUpdate,
        transfer_game_state.after(UpdateHostDataSystemSet),
    );
    app.register_required_components::<HostConnection, GameState>();
}

/// Component for holding the current game state of a host or field
#[derive(Component, Deref, Debug, Default, Clone, PartialEq, Eq)]
pub struct GameState(pub proto::remote::GameState);

/// Transfers the game state from hosts to all fields related via [GameStateHost]
fn transfer_game_state(
    mut commands: Commands,
    mut q_fields: Query<(&GameStateHost, Option<&mut GameState>, Entity), With<Field>>,
    q_hosts: Query<Ref<GameState>, (With<HostConnection>, Without<Field>)>,
) {
    for (GameStateHost(host_entity), field_game_state, field_entity) in q_fields.iter_mut() {
        let Ok(host_game_state) = q_hosts.get(*host_entity) else {
            // The HostConnection is probably missing, so it makes no sense to update the field
            continue;
        };

        if host_game_state.is_changed()
            && let Some(mut field_game_state) = field_game_state
        {
            field_game_state.set_if_neq(host_game_state.as_ref().clone());
        } else if field_game_state.is_none() {
            commands
                .entity(field_entity)
                .insert(host_game_state.as_ref().clone());
        }
    }
}
