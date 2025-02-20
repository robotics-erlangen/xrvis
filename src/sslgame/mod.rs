pub mod proto {
    pub mod amun_compact {
        include!(concat!(env!("OUT_DIR"), "/amun_compact.rs"));
    }
}
mod state_filter;
mod state_receiver;

use crate::sslgame::proto::amun_compact;
use crate::sslgame::proto::amun_compact::vis_part::Geom;
use crate::sslgame::state_filter::StateFilter;
use crate::sslgame::state_receiver::{manage_rx_task, StatusUpdateReceiver};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::utils::HashMap;
use procedural_modelling::{extensions::bevy::*, mesh::MeshBuilder, prelude::*};
use std::cmp::PartialEq;
use std::f32::consts::PI;

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

// ======== Mesh generation ========

fn circle_vertices(n: u32, radius: f32, center: Vec3) -> impl Iterator<Item = BevyVertexPayload3d> {
    (0..n).map(move |i| {
        let phi = (2.0 * f32::PI) * (i as f32 / n as f32);
        BevyVertexPayload3d::from_pos(Vec3::new(
            center.x + phi.cos() * radius,
            center.y,
            center.z + phi.sin() * radius,
        ))
    })
}

fn field_mesh(
    geom: &FieldGeometry,
    mesh_assets: &mut ResMut<Assets<Mesh>>,
    material_assets: &mut ResMut<Assets<StandardMaterial>>,
) -> Scene {
    let field_mat = StandardMaterial::from(Color::srgba_u8(0, 135, 0, 255));
    let wall_mat = StandardMaterial::from(Color::srgba_u8(0, 0, 0, 255));
    let goal_y_mat = StandardMaterial::from(Color::srgba_u8(255, 255, 0, 255));
    let goal_b_mat = StandardMaterial::from(Color::srgba_u8(0, 0, 255, 255));
    let line_mat = StandardMaterial::from(Color::srgba_u8(255, 255, 255, 255));

    static WALL_WIDTH: f32 = 0.04;
    static WALL_HEIGHT: f32 = 0.16;
    static GOAL_WALL: f32 = 0.03;
    static GOAL_WALL_HALF: f32 = GOAL_WALL / 2f32;
    static CENTER_CIRCLE_RADIUS: f32 = 0.5;
    static LINE_HALF: f32 = 0.01 / 2f32;

    // ==== Field ====

    let border_x = geom.play_area_size.x / 2.0;
    let border_y = geom.play_area_size.y / 2.0;
    let field_x = border_x + geom.boundary_width;
    let field_y = border_y + geom.boundary_width;

    let mut field_mesh = BevyMesh3d::new();

    field_mesh.insert_polygon(
        [
            Vec3::new(field_x, 0.0, field_y),
            Vec3::new(-field_x, 0.0, field_y),
            Vec3::new(-field_x, 0.0, -field_y),
            Vec3::new(field_x, 0.0, -field_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );

    // ==== Wall ====

    let mut wall_mesh = BevyMesh3d::new();

    let wall_bottom_inner = wall_mesh.insert_loop(
        [
            Vec3::new(field_x, 0.0, field_y),
            Vec3::new(-field_x, 0.0, field_y),
            Vec3::new(-field_x, 0.0, -field_y),
            Vec3::new(field_x, 0.0, -field_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    let wall_bottom_inner = wall_mesh.edge(wall_bottom_inner).twin_id();
    let wall_top_inner = wall_mesh.loft_polygon(
        wall_bottom_inner,
        2,
        2,
        [
            Vec3::new(field_x, WALL_HEIGHT, field_y),
            Vec3::new(-field_x, WALL_HEIGHT, field_y),
            Vec3::new(-field_x, WALL_HEIGHT, -field_y),
            Vec3::new(field_x, WALL_HEIGHT, -field_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    let wall_top_outer = wall_mesh.loft_polygon(
        wall_top_inner,
        2,
        2,
        [
            Vec3::new(field_x + WALL_WIDTH, WALL_HEIGHT, field_y + WALL_WIDTH),
            Vec3::new(-field_x - WALL_WIDTH, WALL_HEIGHT, field_y + WALL_WIDTH),
            Vec3::new(-field_x - WALL_WIDTH, WALL_HEIGHT, -field_y - WALL_WIDTH),
            Vec3::new(field_x + WALL_WIDTH, WALL_HEIGHT, -field_y - WALL_WIDTH),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    wall_mesh.loft_polygon(
        wall_top_outer,
        2,
        2,
        [
            Vec3::new(field_x + WALL_WIDTH, 0.0, field_y + WALL_WIDTH),
            Vec3::new(-field_x - WALL_WIDTH, 0.0, field_y + WALL_WIDTH),
            Vec3::new(-field_x - WALL_WIDTH, 0.0, -field_y - WALL_WIDTH),
            Vec3::new(field_x + WALL_WIDTH, 0.0, -field_y - WALL_WIDTH),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );

    // ==== Goal ====

    let goal_x = geom.goal_width / 2.0;

    let mut goal_y_mesh = BevyMesh3d::new();

    let goal_y_bottom = goal_y_mesh.insert_loop(
        [
            // Inner
            Vec3::new(-goal_x + GOAL_WALL_HALF, 0.0, -border_y),
            Vec3::new(-goal_x + GOAL_WALL_HALF, 0.0, -field_y + GOAL_WALL),
            Vec3::new(goal_x - GOAL_WALL_HALF, 0.0, -field_y + GOAL_WALL),
            Vec3::new(goal_x - GOAL_WALL_HALF, 0.0, -border_y),
            // Outer
            Vec3::new(goal_x + GOAL_WALL_HALF, 0.0, -border_y),
            Vec3::new(goal_x + GOAL_WALL_HALF, 0.0, -field_y),
            Vec3::new(-goal_x - GOAL_WALL_HALF, 0.0, -field_y),
            Vec3::new(-goal_x - GOAL_WALL_HALF, 0.0, -border_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    let goal_y_bottom = goal_y_mesh.edge(goal_y_bottom).twin_id();
    let goal_y_top = goal_y_mesh.loft_polygon(
        goal_y_bottom,
        2,
        2,
        [
            // Inner
            Vec3::new(-goal_x + GOAL_WALL_HALF, WALL_HEIGHT, -border_y),
            Vec3::new(-goal_x + GOAL_WALL_HALF, WALL_HEIGHT, -field_y + GOAL_WALL),
            Vec3::new(goal_x - GOAL_WALL_HALF, WALL_HEIGHT, -field_y + GOAL_WALL),
            Vec3::new(goal_x - GOAL_WALL_HALF, WALL_HEIGHT, -border_y),
            // Outer
            Vec3::new(goal_x + GOAL_WALL_HALF, WALL_HEIGHT, -border_y),
            Vec3::new(goal_x + GOAL_WALL_HALF, WALL_HEIGHT, -field_y),
            Vec3::new(-goal_x - GOAL_WALL_HALF, WALL_HEIGHT, -field_y),
            Vec3::new(-goal_x - GOAL_WALL_HALF, WALL_HEIGHT, -border_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    goal_y_mesh.close_hole_default(goal_y_top);

    let mut goal_b_mesh = BevyMesh3d::new();

    let goal_b_bottom = goal_b_mesh.insert_loop(
        [
            // Inner
            Vec3::new(goal_x - GOAL_WALL_HALF, 0.0, border_y),
            Vec3::new(goal_x - GOAL_WALL_HALF, 0.0, field_y - GOAL_WALL),
            Vec3::new(-goal_x + GOAL_WALL_HALF, 0.0, field_y - GOAL_WALL),
            Vec3::new(-goal_x + GOAL_WALL_HALF, 0.0, border_y),
            // Outer
            Vec3::new(-goal_x - GOAL_WALL_HALF, 0.0, border_y),
            Vec3::new(-goal_x - GOAL_WALL_HALF, 0.0, field_y),
            Vec3::new(goal_x + GOAL_WALL_HALF, 0.0, field_y),
            Vec3::new(goal_x + GOAL_WALL_HALF, 0.0, border_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    let goal_b_bottom = goal_b_mesh.edge(goal_b_bottom).twin_id();
    let goal_b_top = goal_b_mesh.loft_polygon(
        goal_b_bottom,
        2,
        2,
        [
            // Inner
            Vec3::new(goal_x - GOAL_WALL_HALF, WALL_HEIGHT, border_y),
            Vec3::new(goal_x - GOAL_WALL_HALF, WALL_HEIGHT, field_y - GOAL_WALL),
            Vec3::new(-goal_x + GOAL_WALL_HALF, WALL_HEIGHT, field_y - GOAL_WALL),
            Vec3::new(-goal_x + GOAL_WALL_HALF, WALL_HEIGHT, border_y),
            // Outer
            Vec3::new(-goal_x - GOAL_WALL_HALF, WALL_HEIGHT, border_y),
            Vec3::new(-goal_x - GOAL_WALL_HALF, WALL_HEIGHT, field_y),
            Vec3::new(goal_x + GOAL_WALL_HALF, WALL_HEIGHT, field_y),
            Vec3::new(goal_x + GOAL_WALL_HALF, WALL_HEIGHT, border_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    goal_b_mesh.close_hole_default(goal_b_top);

    // ==== Lines ====

    let mut line_mesh = BevyMesh3d::new();

    // Center circle
    let line_circle_inner_verts = circle_vertices(
        128,
        CENTER_CIRCLE_RADIUS - LINE_HALF,
        Vec3::new(0.0, 0.0001, 0.0),
    );
    let line_circle_inner = line_mesh.insert_loop(line_circle_inner_verts);
    let line_circle_inner = line_mesh.edge(line_circle_inner).twin_id();
    let line_circle_outer_verts = circle_vertices(
        128,
        CENTER_CIRCLE_RADIUS + LINE_HALF,
        Vec3::new(0.0, 0.0001, 0.0),
    );
    line_mesh.loft_polygon(line_circle_inner, 2, 2, line_circle_outer_verts);

    // Center line
    line_mesh.insert_polygon(
        [
            Vec3::new(border_x, 0.0001, LINE_HALF),
            Vec3::new(-border_x, 0.0001, LINE_HALF),
            Vec3::new(-border_x, 0.0001, -LINE_HALF),
            Vec3::new(border_x, 0.0001, -LINE_HALF),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );

    // Border
    let line_border_inner = line_mesh.insert_loop(
        [
            Vec3::new(border_x - LINE_HALF, 0.0001, border_y - LINE_HALF),
            Vec3::new(-border_x + LINE_HALF, 0.0001, border_y - LINE_HALF),
            Vec3::new(-border_x + LINE_HALF, 0.0001, -border_y + LINE_HALF),
            Vec3::new(border_x - LINE_HALF, 0.0001, -border_y + LINE_HALF),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    let line_border_inner = line_mesh.edge(line_border_inner).twin_id();
    line_mesh.loft_polygon(
        line_border_inner,
        2,
        2,
        [
            Vec3::new(border_x + LINE_HALF, 0.0001, border_y + LINE_HALF),
            Vec3::new(-border_x - LINE_HALF, 0.0001, border_y + LINE_HALF),
            Vec3::new(-border_x - LINE_HALF, 0.0001, -border_y - LINE_HALF),
            Vec3::new(border_x + LINE_HALF, 0.0001, -border_y - LINE_HALF),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );

    let defense_x = geom.defense_size.x / 2.0;
    let defense_y = border_y - geom.defense_size.y;

    // Defense area yellow
    let (line_defense_y_inner, _) = line_mesh.insert_path(
        [
            Vec3::new(defense_x - LINE_HALF, 0.0001, -border_y),
            Vec3::new(defense_x - LINE_HALF, 0.0001, -defense_y - LINE_HALF),
            Vec3::new(-defense_x + LINE_HALF, 0.0001, -defense_y - LINE_HALF),
            Vec3::new(-defense_x + LINE_HALF, 0.0001, -border_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    let line_defense_y_inner = line_mesh.edge(line_defense_y_inner).twin_id();
    line_mesh.loft_polygon(
        line_defense_y_inner,
        2,
        2,
        [
            Vec3::new(defense_x + LINE_HALF, 0.0001, -border_y),
            Vec3::new(defense_x + LINE_HALF, 0.0001, -defense_y + LINE_HALF),
            Vec3::new(-defense_x - LINE_HALF, 0.0001, -defense_y + LINE_HALF),
            Vec3::new(-defense_x - LINE_HALF, 0.0001, -border_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );

    // Defense area blue
    let (line_defense_b_inner, _) = line_mesh.insert_path(
        [
            Vec3::new(-defense_x + LINE_HALF, 0.0001, border_y),
            Vec3::new(-defense_x + LINE_HALF, 0.0001, defense_y + LINE_HALF),
            Vec3::new(defense_x - LINE_HALF, 0.0001, defense_y + LINE_HALF),
            Vec3::new(defense_x - LINE_HALF, 0.0001, border_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );
    let line_defense_b_inner = line_mesh.edge(line_defense_b_inner).twin_id();
    line_mesh.loft_polygon(
        line_defense_b_inner,
        2,
        2,
        [
            Vec3::new(-defense_x - LINE_HALF, 0.0001, border_y),
            Vec3::new(-defense_x - LINE_HALF, 0.0001, defense_y - LINE_HALF),
            Vec3::new(defense_x + LINE_HALF, 0.0001, defense_y - LINE_HALF),
            Vec3::new(defense_x + LINE_HALF, 0.0001, border_y),
        ]
        .map(BevyVertexPayload3d::from_pos),
    );

    let mut world = World::new();

    world.spawn_batch([
        (
            Transform::default(),
            Mesh3d(mesh_assets.add(field_mesh.to_bevy(RenderAssetUsages::default()))),
            MeshMaterial3d(material_assets.add(field_mat)),
        ),
        (
            Transform::default(),
            Mesh3d(mesh_assets.add(wall_mesh.to_bevy(RenderAssetUsages::default()))),
            MeshMaterial3d(material_assets.add(wall_mat)),
        ),
        (
            Transform::default(),
            Mesh3d(mesh_assets.add(goal_y_mesh.to_bevy(RenderAssetUsages::default()))),
            MeshMaterial3d(material_assets.add(goal_y_mat)),
        ),
        (
            Transform::default(),
            Mesh3d(mesh_assets.add(goal_b_mesh.to_bevy(RenderAssetUsages::default()))),
            MeshMaterial3d(material_assets.add(goal_b_mat)),
        ),
        (
            Transform::default(),
            Mesh3d(mesh_assets.add(line_mesh.to_bevy(RenderAssetUsages::default()))),
            MeshMaterial3d(material_assets.add(line_mat)),
        ),
    ]);

    Scene::new(world)
}

fn circle_visualization_mesh(n: u32, radius: f32) -> Mesh {
    let mut mesh = BevyMesh3d::new();

    mesh.insert_polygon(circle_vertices(n, radius, Vec3::new(0.0, 0.0, 0.0)));

    mesh.to_bevy(RenderAssetUsages::default())
}

fn polygon_visualization_mesh(vertices: &[(i32, i32)]) -> Mesh {
    assert!(!vertices.is_empty());

    let mut mesh = BevyMesh3d::new();

    mesh.insert_polygon(vertices.iter().map(|p| {
        BevyVertexPayload3d::from_pos(Vec3::new(p.0 as f32 / 1000.0, 0.0, p.1 as f32 / 1000.0))
    }));

    mesh.to_bevy(RenderAssetUsages::default())
}

fn path_visualization_mesh(path: &[Vec2], width: f32) -> Mesh {
    assert!(!path.is_empty());
    assert!(width >= 0.0);

    let mut mesh = BevyMesh3d::new();
    let radius = width / 2.0;

    for point in path {
        mesh.insert_polygon(circle_vertices(
            16,
            radius,
            Vec3::new(point.x, 0.0, point.y),
        ));
    }
    for edge in path.windows(2).filter(|e| e[0] != e[1]) {
        let a = Vec3::new(edge[0].x, 0.0, edge[0].y);
        let b = Vec3::new(edge[1].x, 0.0, edge[1].y);
        let half = (b - a)
            .rotated(&Quat::from_rotation_y(PI / -2.0))
            .clamp_length(radius, radius);
        mesh.insert_polygon([
            BevyVertexPayload3d::from_pos(a - half),
            BevyVertexPayload3d::from_pos(b - half),
            BevyVertexPayload3d::from_pos(b + half),
            BevyVertexPayload3d::from_pos(a + half),
        ]);
    }

    mesh.to_bevy_ex(
        RenderAssetUsages::default(),
        TriangulationAlgorithm::Fan, // All the generated meshes are convex (circle, rect), so fan is the fastest option
        false,
    )
}
