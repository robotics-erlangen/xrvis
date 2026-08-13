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
    q_hosts: Query<&GameState, (With<HostConnection>, Without<Field>)>,
    mut q_fields: Query<(Option<&mut GameState>, &GameStateHost, Entity), With<Field>>,
) {
    for (field_game_state, GameStateHost(host_entity), field_entity) in q_fields.iter_mut() {
        let host_game_state = match q_hosts.get(*host_entity) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to fetch host entity {host_entity:?} for transfer_game_state: {e}");
                return;
            }
        };
        if let Some(mut field_game_state) = field_game_state {
            field_game_state.set_if_neq(host_game_state.clone());
        } else {
            commands
                .entity(field_entity)
                .insert(host_game_state.clone());
        }
    }
}
