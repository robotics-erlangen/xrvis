pub mod discovery;
pub mod game_state;
pub mod geometry;
pub mod hosts;
pub mod robots;
pub mod visualizations;

use bevy::prelude::*;
use std::cmp::PartialEq;

/// Initializes the host and field infrastructure, including networking and automated host discovery
///
/// System order overview:
/// ```
/// PreUpdate
///     receive_host_advertisements
///     receive_host_packets -> transfer_field_geometry transfer_game_state update_visualization_instances
/// PostUpdate
///     apply_filtered_transform
///     send_vis_selection
/// ```
pub fn field_plugin(app: &mut App) {
    app.add_plugins(hosts::host_plugin);
    app.add_plugins(discovery::discovery_plugin);

    app.add_plugins(geometry::field_geometry_plugin);
    app.add_plugins(game_state::game_state_plugin);
    app.add_plugins(robots::robot_plugin);
    app.add_plugins(visualizations::vis_plugin);
}

/// SSL team. Used both as a normal value and a component on robots.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    #[default]
    Yellow,
    Blue,
}

#[derive(Component, Debug)]
#[require(Visibility, Transform)]
pub struct Field;
