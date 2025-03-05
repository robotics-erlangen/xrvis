use crate::sslgame::FieldGeometry;
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::color::Color;
use bevy::math::{Quat, Vec2, Vec3};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::*;
use procedural_modelling::extensions::bevy::{BevyMesh3d, BevyVertexPayload3d};
use procedural_modelling::mesh::MeshBuilder;
use procedural_modelling::prelude::*;
use std::f32::consts::PI;

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

pub fn field_mesh(
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

pub fn circle_visualization_mesh(n: u32, radius: f32) -> Mesh {
    let mut mesh = BevyMesh3d::new();

    mesh.insert_polygon(circle_vertices(n, radius, Vec3::new(0.0, 0.0, 0.0)));

    mesh.to_bevy(RenderAssetUsages::default())
}

pub fn polygon_visualization_mesh(vertices: &[(i32, i32)]) -> Mesh {
    assert!(!vertices.is_empty());

    let mut mesh = BevyMesh3d::new();

    mesh.insert_polygon(vertices.iter().map(|p| {
        BevyVertexPayload3d::from_pos(Vec3::new(p.0 as f32 / 1000.0, 0.0, p.1 as f32 / 1000.0))
    }));

    mesh.to_bevy(RenderAssetUsages::default())
}

pub fn path_visualization_mesh(path: &[Vec2], width: f32) -> Mesh {
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
