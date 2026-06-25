use crate::depth_mask_material::DepthMaskMaterial;
use crate::field::Team;
use crate::proto::remote::WorldState;
use crate::transform_filter::{TransformFilter, apply_filtered_transform};
use crate::{RenderSettings, RobotRenderSettings, proto};
use bevy::asset::{AssetServer, Assets, Handle};
use bevy::color::Color;
use bevy::math::{Affine3A, Quat, Vec3};
use bevy::mesh::{
    CylinderAnchor, CylinderMeshBuilder, Mesh, Mesh3d, MeshBuilder, SphereKind, SphereMeshBuilder,
};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::*;
use std::f32::consts::PI;
use std::time::{Duration, Instant};

pub fn field_robot_plugin(app: &mut App) {
    app.add_plugins(MaterialPlugin::<DepthMaskMaterial>::default());

    let world = app.world_mut();

    let mut meshes = world.resource_mut::<Assets<Mesh>>();
    let robot_mask_mesh = meshes.add(MeshBuilder::build(
        &CylinderMeshBuilder::new(0.09, 0.15, 32).anchor(CylinderAnchor::Bottom),
    ));
    // FIXME: Ball in the ground
    let ball_mesh = meshes.add(MeshBuilder::build(&SphereMeshBuilder::new(
        0.0215,
        SphereKind::Ico { subdivisions: 3 },
    )));

    let robot_mask_material = world
        .resource_mut::<Assets<DepthMaskMaterial>>()
        .add(DepthMaskMaterial {});
    let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
    let ball_material = materials.add(StandardMaterial::from_color(Color::srgb_u8(255, 136, 0)));

    app.insert_resource(RobotMaskMesh(robot_mask_mesh, robot_mask_material));
    app.insert_resource(BallMesh(ball_mesh, ball_material));

    app.add_systems(PostUpdate, apply_filtered_transform);
}

#[derive(Component, Debug, Clone, Copy)]
#[require(Team, Transform)]
pub struct Robot(pub u8);

#[derive(Component, Debug, Clone, Copy)]
#[require(Transform)]
pub struct Ball;

#[derive(Resource, Debug)]
pub(crate) struct RobotMaskMesh(Handle<Mesh>, Handle<DepthMaskMaterial>);

#[derive(Resource, Debug)]
pub(crate) struct BallMesh(Handle<Mesh>, Handle<StandardMaterial>);

/// Updates the robots and balls on a single field to a new WorldState. Has to be called manually using Commands::run_system_cached_with.
#[allow(clippy::type_complexity)]
pub(crate) fn update_world_state(
    In((field_entity, mut world_state, rx_time)): In<(Entity, WorldState, Instant)>,
    mut commands: Commands,
    render_settings: Res<RenderSettings>,
    asset_server: Res<AssetServer>,
    (ball_mesh, robot_mask_mesh): (Res<BallMesh>, Res<RobotMaskMesh>),
    (mut q_robots, q_balls): (
        Query<(&Robot, &Team, &mut TransformFilter, &ChildOf, Entity)>,
        Query<(&Transform, &ChildOf, Entity), (With<Ball>, Without<Robot>)>,
    ),
) {
    // Remap from the vision coordinate system (right-handed, z up, x towards blue goal, +x forward)
    // to bevy's coordinate system (right-handed, y up, x towards blue goal, -z forward) with y and z swapped
    for ball in &mut world_state.ball {
        ball.p_y = -ball.p_y;
    }
    for robot in &mut world_state.yellow_robot {
        robot.p_y = -robot.p_y;
        robot.phi -= PI / 2.0;
    }
    for robot in &mut world_state.blue_robot {
        robot.p_y = -robot.p_y;
        robot.phi -= PI / 2.0;
    }

    // TODO: Correlate new to old balls and move them instead of recreating everything. Don't forget to update handle_render_settings_change
    // Despawn old balls
    q_balls
        .iter()
        .map(|(_, c, e)| (c.parent(), e))
        .filter(|(p, _)| *p == field_entity)
        .for_each(|(_, e)| {
            commands.entity(field_entity).detach_child(e);
            commands.entity(e).despawn()
        });

    // Spawn new balls
    for new_ball in world_state.ball {
        let new_ball_pos = Vec3::new(new_ball.p_x, new_ball.p_z.unwrap_or(0.0), new_ball.p_y);

        let mut new_ball = commands.spawn((Ball, Transform::from_translation(new_ball_pos)));
        if render_settings.ball {
            new_ball.insert((
                Mesh3d(ball_mesh.0.clone()),
                MeshMaterial3d(ball_mesh.1.clone()),
            ));
        }
        let new_ball = new_ball.id();
        commands.entity(field_entity).add_child(new_ball);
    }

    // Update robots
    let mut leftover_robots = q_robots
        .iter_mut()
        .filter(|(_, _, _, c, _)| c.parent() == field_entity)
        .collect::<Vec<_>>();

    let mut update_robots = |team: Team, new_robots: Vec<proto::remote::Robot>| {
        for robot_update in new_robots {
            let leftover_index = leftover_robots
                .iter()
                .position(|(r, t, _, _, _)| **t == team && r.0 as u32 == robot_update.id);
            let new_robot_pos = Vec3::new(robot_update.p_x, 0.0, robot_update.p_y);

            if let Some(i) = leftover_index {
                // Robot already exists -> update transform
                let (_, _, mut t, _, _) = leftover_robots.remove(i);
                t.push_sample(
                    Affine3A::from_scale_rotation_translation(
                        Vec3::ONE,
                        Quat::from_rotation_y(robot_update.phi),
                        new_robot_pos,
                    ),
                    rx_time,
                );
            } else {
                // Add new robot
                let mut new_robot = commands.spawn((
                    Robot(robot_update.id as u8),
                    team,
                    TransformFilter::new_history(Duration::from_millis(500), true),
                    Transform {
                        translation: new_robot_pos,
                        rotation: Quat::from_rotation_y(robot_update.phi),
                        ..Transform::default()
                    },
                ));
                match render_settings.robots {
                    RobotRenderSettings::Detailed => todo!(),
                    RobotRenderSettings::Fallback => {
                        new_robot.insert(WorldAssetRoot(
                            asset_server.load("teams/robots/generic.glb#Scene0"),
                        ));
                    }
                    RobotRenderSettings::Cutout => {
                        new_robot.insert((
                            Mesh3d(robot_mask_mesh.0.clone()),
                            MeshMaterial3d(robot_mask_mesh.1.clone()),
                        ));
                    }
                    RobotRenderSettings::None => {}
                }
                let new_robot_id = new_robot.id();
                commands.entity(field_entity).add_child(new_robot_id);
            }
        }
    };

    update_robots(Team::Yellow, world_state.yellow_robot);
    update_robots(Team::Blue, world_state.blue_robot);

    // Despawn all remaining robots
    leftover_robots.into_iter().for_each(|(_, _, _, _, e)| {
        commands.entity(field_entity).detach_child(e);
        commands.entity(e).despawn()
    });
}
