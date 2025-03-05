pub mod proto {
    pub mod amun_compact {
        include!(concat!(env!("OUT_DIR"), "/amun_compact.rs"));
    }
}
mod mesh_generators;
mod state_filter;
mod state_receiver;

use crate::sslgame::mesh_generators::*;
use crate::sslgame::proto::amun_compact;
use crate::sslgame::proto::amun_compact::vis_part::Geom;
use crate::sslgame::state_filter::StateFilter;
use crate::sslgame::state_receiver::{StatusUpdateReceiver, manage_rx_task};
use bevy::prelude::*;
use bevy::utils::HashMap;
use std::cmp::PartialEq;

pub fn ssl_game_plugin(app: &mut App) {
    app.world_mut().spawn((
        Field {
            game_state: None,
            geometry: FieldGeometry::div_a(),
        },
        StateFilter::default(),
    ));

    app.add_systems(
        Update,
        (
            receive_update_events,
            (
                update_game_state,
                update_field_geometry,
                update_world_state,
                update_visualizations,
            ),
        )
            .chain(),
    );

    app.add_systems(Update, manage_rx_task);
}

// ======== Field components ========

#[derive(Component, Default)]
pub struct GameInfo {
    yellow_team: String,
    blue_team: String,
}

#[derive(PartialEq)]
pub struct FieldGeometry {
    play_area_size: Vec2,
    boundary_width: f32,
    defense_size: Vec2,
    goal_width: f32,
}

impl FieldGeometry {
    pub fn div_a() -> Self {
        Self {
            play_area_size: Vec2::new(12.0, 9.0),
            boundary_width: 0.3,
            defense_size: Vec2::new(1.8, 3.6),
            goal_width: 1.8,
        }
    }

    pub fn div_b() -> Self {
        Self {
            play_area_size: Vec2::new(9.0, 6.0),
            boundary_width: 0.3,
            defense_size: Vec2::new(1.0, 2.0),
            goal_width: 1.0,
        }
    }
}

impl Default for FieldGeometry {
    fn default() -> Self {
        Self::div_a()
    }
}

#[derive(Component, Default)]
#[require(Transform, Visibility)]
pub struct Field {
    pub game_state: Option<GameInfo>,
    pub geometry: FieldGeometry,
}

// ======== Field content components =========

#[derive(Component, Default, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    #[default]
    Yellow,
    Blue,
}

#[derive(Component, Default)]
#[require(Team, Transform)]
pub struct Robot(u8);

#[derive(Component, Default)]
#[require(Transform)]
pub struct Ball;

#[derive(Component, Default)]
#[require(Transform)]
pub struct Visualization(String);

// ======== Systems ========

fn receive_update_events(
    receiver: Option<Res<StatusUpdateReceiver>>,
    mut state_filter: Single<&mut StateFilter>,
) {
    let Some(receiver) = receiver else {
        return;
    };
    while let Ok(new_status) = receiver.channel.try_recv() {
        state_filter.push_packet(new_status);
    }
}

fn update_game_state(q_field: Single<(&mut Field, &StateFilter)>) {
    let (mut field, state_filter) = q_field.into_inner();
    let new_game_state = state_filter.current_game_state();

    field.game_state = Some(GameInfo {
        yellow_team: new_game_state.yellow_team,
        blue_team: new_game_state.blue_team,
    });
}

#[allow(clippy::type_complexity)]
fn update_field_geometry(
    mut commands: Commands,
    (mut mesh_assets, mut material_assets, mut scene_assets): (
        ResMut<Assets<Mesh>>,
        ResMut<Assets<StandardMaterial>>,
        ResMut<Assets<Scene>>,
    ),
    q_field: Single<(&mut Field, &StateFilter, Entity)>,
) {
    let (mut field, state_filter, field_entity) = q_field.into_inner();
    let Some(new_field_geom) = state_filter.field_geometry_update() else {
        return;
    };

    if field.geometry != new_field_geom {
        commands.entity(field_entity).remove::<SceneRoot>();

        let field_model = field_mesh(&new_field_geom, &mut mesh_assets, &mut material_assets);
        commands
            .entity(field_entity)
            .insert(SceneRoot(scene_assets.add(field_model)));

        field.geometry = new_field_geom;
    }
}

#[allow(clippy::type_complexity)]
fn update_world_state(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    (q_field, mut q_robots, mut q_balls): (
        Single<(&StateFilter, Entity), With<Field>>,
        Query<(&Robot, &Team, &mut Transform, &Parent, Entity), Without<Ball>>,
        Query<(&mut Transform, &Parent), With<Ball>>,
    ),
) {
    let (state_filter, field_entity) = q_field.into_inner();
    let world_state = state_filter.current_world_state();

    // Update ball
    if let Some(ball_update) = world_state.ball {
        let new_ball_pos = Vec3::new(
            ball_update.p_x,
            ball_update.p_z.unwrap_or(0.0),
            ball_update.p_y,
        );

        if let Some((mut ball_pos, _)) = q_balls.iter_mut().find(|(_, p)| p.get() == field_entity) {
            ball_pos.translation = new_ball_pos;
        } else {
            let new_ball = commands
                .spawn((
                    Ball,
                    Transform::from_translation(new_ball_pos),
                    SceneRoot(asset_server.load("ball.glb#Scene0")),
                ))
                .id();
            commands.entity(field_entity).add_child(new_ball);
        }
    } else {
        q_balls
            .iter()
            .map(|(_, p)| p.get())
            .filter(|p| *p == field_entity)
            .for_each(|e| {
                commands.entity(field_entity).remove_children(&[e]);
                commands.entity(e).despawn()
            })
    }

    // Update robots
    let mut leftover_robots = q_robots
        .iter_mut()
        .filter(|(_, _, _, p, _)| p.get() == field_entity)
        .collect::<Vec<_>>();

    let mut update_robots = |team: Team, new_robots: Vec<amun_compact::Robot>| {
        for robot_update in new_robots {
            let leftover_index = leftover_robots
                .iter()
                .position(|(r, t, _, _, _)| **t == team && r.0 as u32 == robot_update.id);
            let new_robot_pos = Vec3::new(robot_update.p_x, 0.0, robot_update.p_y);

            if let Some(i) = leftover_index {
                // Robot already exists, update transform
                let (_, _, mut t, _, _) = leftover_robots.remove(i);
                t.translation = new_robot_pos;
                t.rotation = Quat::from_rotation_y(robot_update.phi);
            } else {
                // Add new robot
                let new_robot = commands
                    .spawn((
                        Robot(robot_update.id as u8),
                        team,
                        Transform {
                            // Remap coordinates into bevy's coordinate conventions
                            translation: new_robot_pos,
                            rotation: Quat::from_rotation_y(robot_update.phi),
                            ..Transform::default()
                        },
                        SceneRoot(asset_server.load("robots/generic.glb#Scene0")),
                    ))
                    .id();
                commands.entity(field_entity).add_child(new_robot);
            }
        }
    };

    update_robots(Team::Yellow, world_state.yellow_robot);
    update_robots(Team::Blue, world_state.blue_robot);

    // Despawn all remaining robots
    leftover_robots.into_iter().for_each(|(_, _, _, _, e)| {
        commands.entity(field_entity).remove_children(&[e]);
        commands.entity(e).despawn()
    });
}

#[derive(Default)]
pub struct VisualizationModels {
    // TODO: Replace with AssetId (weak handle)
    pub circle_vis: HashMap<u32, Handle<Mesh>>,
    pub polygon_vis: HashMap<Vec<(i32, i32)>, Handle<Mesh>>,
}

#[allow(clippy::type_complexity)]
fn update_visualizations(
    mut commands: Commands,
    mut vis_models: Local<VisualizationModels>,
    (mut mesh_assets, mut material_assets): (
        ResMut<Assets<Mesh>>,
        ResMut<Assets<StandardMaterial>>,
    ),
    (q_field, q_visualizations): (
        Single<(&StateFilter, Entity), With<Field>>,
        Query<(&Visualization, &Parent, Entity)>,
    ),
) {
    let (state_filter, field_entity) = q_field.into_inner();
    let (group_count, updated_groups, new_visualizations) = state_filter.visualization_updates();

    // Despawn old visualization meshes
    q_visualizations
        .iter()
        .filter(|(_, p, _)| p.get() == field_entity)
        .for_each(|(v, _, e)| {
            let curr_group =
                v.0.chars()
                    .fold(0u32, |acc, c| (acc + c as u32) % group_count);
            if updated_groups.contains(&curr_group) {
                commands.entity(field_entity).remove_children(&[e]);
                commands.entity(e).despawn();
            }
        });

    // Generate + Spawn new visualization meshes
    for visualization in new_visualizations {
        for part in &visualization.part {
            let border_color = part
                .border_style
                .and_then(|s| s.color)
                .map_or(Color::srgba_u8(0, 0, 0, 255), |c| {
                    Color::srgba_u8(c.red as u8, c.green as u8, c.blue as u8, c.alpha as u8)
                });

            let mat = StandardMaterial::from(border_color);

            let (mesh, pos) = match part.geom.as_ref() {
                Some(Geom::Circle(c)) => {
                    // Convert to integers to get more stable hashing
                    let mm_radius = (c.radius * 1000.0) as u32;
                    if let Some(handle) = vis_models.circle_vis.get(&mm_radius) {
                        (handle.clone(), Vec2::new(c.p_x, c.p_y))
                    } else {
                        let new_handle = mesh_assets.add(circle_visualization_mesh(32, c.radius));
                        vis_models.circle_vis.insert(mm_radius, new_handle.clone());
                        (new_handle, Vec2::new(c.p_x, c.p_y))
                    }
                }
                Some(Geom::Polygon(poly)) if !poly.point.is_empty() => {
                    let s = Vec2::new(poly.point[0].x, poly.point[0].y);
                    // Convert to integers to get more stable hashing
                    let mm_points = poly
                        .point
                        .iter()
                        .map(|p| (((p.x - s.x) * 1000.0) as i32, ((p.y - s.y) * 1000.0) as i32))
                        .collect::<Vec<_>>();
                    if let Some(handle) = vis_models.polygon_vis.get(&mm_points) {
                        (handle.clone(), s)
                    } else {
                        let new_handle = mesh_assets.add(polygon_visualization_mesh(&mm_points));
                        vis_models.polygon_vis.insert(mm_points, new_handle.clone());
                        (new_handle, s)
                    }
                }
                Some(Geom::Path(path)) if !path.point.is_empty() => {
                    let s = Vec2::new(path.point[0].x, path.point[0].y);
                    // Don't cache path meshes, they usually change with every frame
                    let new_handle = mesh_assets.add(path_visualization_mesh(
                        &path
                            .point
                            .iter()
                            .map(|p| Vec2::new(p.x - s.x, p.y - s.y))
                            .collect::<Vec<_>>(),
                        0.01,
                    ));
                    (new_handle, s)
                }
                None => {
                    warn!(
                        "Invalid visualization part in {}: No geometry",
                        &visualization.name
                    );
                    continue;
                }
                _ => {
                    warn!(
                        "Invalid visualization part in {}: Empty geometry",
                        &visualization.name
                    );
                    continue;
                }
            };

            let new_vis = commands
                .spawn((
                    Visualization(visualization.name.clone()),
                    Transform::from_xyz(pos.x, 0.001, pos.y),
                    Mesh3d(mesh),
                    MeshMaterial3d(material_assets.add(mat)),
                ))
                .id();

            commands.entity(field_entity).add_child(new_vis);
        }
    }

    // Forget unused meshes that were unloaded by the bevy asset system
    vis_models
        .circle_vis
        .retain(|_, asset_id| mesh_assets.contains(asset_id));
}
