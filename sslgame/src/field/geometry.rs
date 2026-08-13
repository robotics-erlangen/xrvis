use crate::field::Field;
use crate::field::hosts::{GeometryHost, Host, HostConnection, UpdateHostDataSystemSet};
use crate::mesh_gen::field::field_mesh;
use crate::{DefaultMaterial, RenderSettings};
use bevy::asset::Assets;
use bevy::math::Vec2;
use bevy::mesh::{Mesh, Mesh3d};
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;

/// Manages the 3d model of fields based on their [GeometryHost]'s [FieldGeometry]
pub fn field_geometry_plugin(app: &mut App) {
    app.add_systems(
        PreUpdate,
        transfer_field_geometry.after(UpdateHostDataSystemSet),
    );
    app.register_required_components::<HostConnection, FieldGeometry>();
}

/// Simplified SSL field geometry, a more ergonomic version of [proto::FieldGeometry](crate::proto::remote::FieldGeometry).
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

impl From<crate::proto::remote::FieldGeometry> for FieldGeometry {
    fn from(proto: crate::proto::remote::FieldGeometry) -> Self {
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

/// Updates the actual field mesh based on the [FieldGeometry] of the field's [GeometryHost]
#[allow(clippy::type_complexity)]
fn transfer_field_geometry(
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    white_material: Res<DefaultMaterial>,
    mut mesh_assets: ResMut<Assets<Mesh>>,
    mut q_fields: Query<(&GeometryHost, Option<&Mesh3d>, Entity), (With<Field>, Without<Host>)>,
    q_hosts: Query<Ref<FieldGeometry>, (With<Host>, Without<Field>)>,
) {
    for (geometry_host, mesh_component, entity) in &mut q_fields {
        let geometry = match q_hosts.get(geometry_host.0) {
            Ok(g) => g,
            Err(e) => {
                error!(
                    "Failed to fetch host entity {geometry_host:?} for transfer_field_geometry: {e}"
                );
                return;
            }
        };

        if render_settings.field && (geometry.is_changed() || mesh_component.is_none()) {
            commands.entity(entity).insert((
                Mesh3d(mesh_assets.add(field_mesh(&geometry))),
                MeshMaterial3d(white_material.opaque.clone()),
            ));
        }
    }
}
