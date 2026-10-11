//! All the components that store data received from a host connection.

use crate::proto;
use bevy::prelude::*;

/// Component for holding the current game state of a host or fields
#[derive(Component, Deref, Debug, Default, Clone, PartialEq, Eq)]
pub struct GameState(pub proto::remote::GameState);

/// Simplified SSL fields geometry, a more ergonomic version of [proto::FieldGeometry](crate::proto::remote::FieldGeometry).
/// The network protocol marks most values as optional, so they will be filled in with reasonable defaults when converting with .into().
#[derive(Component, Debug, Clone, PartialEq)]
pub struct FieldGeometry {
    pub play_area_size: Vec2,
    pub boundary_width: f32,
    pub defense_size: Vec2,
    pub goal_width: f32,
}

impl FieldGeometry {
    const DIV_A: Self = Self {
        play_area_size: Vec2::new(12.0, 9.0),
        boundary_width: 0.3,
        defense_size: Vec2::new(1.8, 3.6),
        goal_width: 1.8,
    };
    const DIV_B: Self = Self {
        play_area_size: Vec2::new(9.0, 6.0),
        boundary_width: 0.3,
        defense_size: Vec2::new(1.0, 2.0),
        goal_width: 1.0,
    };
}

impl Default for FieldGeometry {
    fn default() -> Self {
        Self::DIV_A
    }
}

impl From<proto::remote::FieldGeometry> for FieldGeometry {
    fn from(proto: proto::remote::FieldGeometry) -> Self {
        FieldGeometry {
            play_area_size: Vec2::new(proto.field_size_x, proto.field_size_y),
            boundary_width: proto.boundary_width.unwrap_or(0.0),
            defense_size: Vec2::new(
                proto.defense_size_x.unwrap_or(proto.field_size_x / 6.),
                proto.defense_size_y.unwrap_or(proto.field_size_y / 3.),
            ),
            goal_width: proto.goal_width.unwrap_or(proto.field_size_y / 5.),
        }
    }
}
