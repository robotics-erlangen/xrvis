pub mod proto {
    pub mod remote {
        include!(concat!(env!("OUT_DIR"), "/remote.rs"));
    }
}
mod depth_mask_material;
pub mod field;
mod mesh_gen;
mod network_tasks;
pub mod panels;
mod transform_filter;

use crate::field::Field;
use crate::field::robots::{Ball, Robot};
use crate::field::visualizations::VisualizationInstance;
use bevy::prelude::*;

pub fn ssl_game_plugin(app: &mut App) {
    app.add_plugins(field::field_plugin);

    app.insert_resource(RenderSettings {
        field: true,
        robots: RobotRenderSettings::Fallback,
        ball: true,
        visualizations: true,
    });

    let world = app.world_mut();

    // Materials
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let white_mat_opaque = materials.add(StandardMaterial::from_color(Color::WHITE));
    let white_mat_translucent = materials.add({
        let mut tmp = StandardMaterial::from_color(Color::WHITE);
        tmp.alpha_mode = AlphaMode::Blend;
        tmp
    });

    app.insert_resource(DefaultMaterial {
        opaque: white_mat_opaque,
        translucent: white_mat_translucent,
    });

    // Systems
    app.add_systems(
        Update,
        handle_render_settings_change.run_if(resource_changed::<RenderSettings>),
    );
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RobotRenderSettings {
    #[default]
    Detailed,
    Fallback,
    Cutout,
    None,
}

/// Global settings for how to render the fields.
#[derive(Resource, Clone, Debug)]
pub struct RenderSettings {
    pub field: bool,
    pub robots: RobotRenderSettings,
    pub ball: bool,
    pub visualizations: bool,
}

impl RenderSettings {
    pub fn full() -> Self {
        RenderSettings {
            field: true,
            robots: RobotRenderSettings::Detailed,
            ball: true,
            visualizations: true,
        }
    }
    pub fn ar() -> Self {
        RenderSettings {
            field: false,
            robots: RobotRenderSettings::Cutout,
            ball: false,
            visualizations: true,
        }
    }
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            field: true,
            robots: RobotRenderSettings::default(),
            ball: true,
            visualizations: true,
        }
    }
}

#[derive(Resource, Debug)]
struct DefaultMaterial {
    pub opaque: Handle<StandardMaterial>,
    pub translucent: Handle<StandardMaterial>,
}

// ======== Systems ========

#[allow(clippy::type_complexity)]
fn handle_render_settings_change(
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    (q_fields, q_robots, q_balls, q_vis_instances): (
        Query<Entity, (With<Field>, With<Mesh3d>)>,
        Query<Entity, With<Robot>>,
        Query<Entity, With<Ball>>,
        Query<Entity, With<VisualizationInstance>>,
    ),
) {
    if !render_settings.field {
        for field_entity in q_fields {
            commands.entity(field_entity).remove::<Mesh3d>();
            commands
                .entity(field_entity)
                .remove::<MeshMaterial3d<StandardMaterial>>();
        }
    }
    // TODO: Retain robots when changing graphics settings
    q_robots.iter().for_each(|e| commands.entity(e).despawn());
    if !render_settings.ball {
        for ball_entity in q_balls {
            commands
                .entity(ball_entity)
                .remove::<Mesh3d>()
                .remove::<MeshMaterial3d<StandardMaterial>>();
        }
    }
    if !render_settings.visualizations {
        for vis_instance_entity in q_vis_instances {
            commands.entity(vis_instance_entity).remove::<Mesh3d>();
            commands
                .entity(vis_instance_entity)
                .remove::<MeshMaterial3d<StandardMaterial>>();
        }
    }
}
