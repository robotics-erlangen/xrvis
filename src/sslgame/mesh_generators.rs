use crate::sslgame::FieldGeometry;
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::color::Color;
use bevy::math::{Vec2, Vec3};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use earcut::Earcut;
use std::f32::consts::PI;

/// A builder for constructing 3D meshes programmatically.
///
/// A "selection" is a set of vertices that can be used by a followup operation.
///
/// # Example
///
/// ```rust
/// let cube = CustomMeshBuilder::new()
///     // Bottom face
///     .with_convex_polygon([
///         [0., 0., 0.],
///         [1., 0., 0.],
///         [1., 0., 1.],
///         [0., 0., 1.],
///     ])
///     // Loft side faces
///     .with_quad_loft([
///         [0., 1., 0.],
///         [ 1., 1., 0.],
///         [ 1., 1.,  1.],
///         [0., 1.,  1.],
///     ], true, true)
///     // Close top face
///     .with_closed_hole(true)
///     .build(false);
/// ```
struct CustomMeshBuilder {
    vertices: Vec<[f32; 3]>,
    indices: Vec<u32>,
    last_operation: usize,
    free_vertices: usize,
}

#[allow(dead_code)]
impl CustomMeshBuilder {
    fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            last_operation: 0,
            free_vertices: 0,
        }
    }

    // Not using bevy's MeshBuilder trait because taking ownership makes sense here
    fn build(self, double_sided: bool) -> Mesh {
        let mut normals = vec![Vec3::ZERO; self.vertices.len()];

        self.indices.chunks(3).for_each(|tri| {
            let a = Vec3::from_array(self.vertices[tri[0] as usize]);
            let b = Vec3::from_array(self.vertices[tri[1] as usize]);
            let c = Vec3::from_array(self.vertices[tri[2] as usize]);
            let normal = (b - a).cross(c - a);
            normals[tri[0] as usize] += normal;
            normals[tri[1] as usize] += normal;
            normals[tri[2] as usize] += normal;
        });

        normals.iter_mut().for_each(|n| *n = n.normalize());

        let u32_indices = self.indices.iter().rev().any(|i| *i > u16::MAX as u32);

        let indices = if double_sided {
            if u32_indices {
                Indices::U32(
                    self.indices
                        .iter()
                        .copied()
                        .chain(self.indices.iter().copied().rev())
                        .collect::<Vec<_>>(),
                )
            } else {
                Indices::U16(
                    self.indices
                        .iter()
                        .copied()
                        .chain(self.indices.iter().copied().rev())
                        .map(|i| i as u16)
                        .collect::<Vec<_>>(),
                )
            }
        } else if u32_indices {
            Indices::U32(self.indices)
        } else {
            Indices::U16(
                self.indices
                    .into_iter()
                    .map(|i| i as u16)
                    .collect::<Vec<_>>(),
            )
        };

        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_indices(indices)
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_POSITION,
            VertexAttributeValues::Float32x3(self.vertices),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            VertexAttributeValues::Float32x3(normals.into_iter().map(|n| n.to_array()).collect()),
        )
    }

    /// Insert raw vertex data without creating a new face. Mostly used as a starting point for [`Self::quad_loft`].
    ///
    /// The inserted vertices will be selected.
    fn insert_vertices(&mut self, vertices: impl IntoIterator<Item = [f32; 3]>) {
        let prev_len = self.vertices.len();
        self.vertices.extend(vertices);
        self.last_operation = self.vertices.len() - prev_len;
        self.free_vertices += self.last_operation;
    }
    /// Chainable version of [`Self::insert_vertices`].
    fn with_vertices(mut self, vertices: impl IntoIterator<Item = [f32; 3]>) -> Self {
        self.insert_vertices(vertices);
        self
    }

    /// Creates a new face out the currently selected vertices.
    /// See implementation for winding details, but it usually behaves as expected.
    ///
    /// The used vertices will stay selected.
    fn close_hole(&mut self, flat_shading: bool) {
        if flat_shading {
            self.vertices
                .extend_from_within((self.vertices.len() - self.last_operation)..);
        }

        let start_index = self.vertices.len() - self.last_operation;

        let mut vert_2d = Vec::new();
        earcut::utils3d::project3d_to_2d(
            &self.vertices[start_index..],
            self.vertices.len() - start_index,
            &mut vert_2d,
        );
        let mut indices_out: Vec<u32> = Vec::new();
        Earcut::new().earcut(vert_2d, &[], &mut indices_out);

        // Reverse winding if operating on used vertices.
        // This isn't always correct, but it's probably what the user intended:
        // .with_vertices().close_hole(): Behaves like insert_polygon()
        // .with_quad_loft().close_hole(): "Encloses" the space started by insert_polygon
        if self.free_vertices >= self.last_operation {
            self.indices
                .extend(indices_out.into_iter().map(|i| start_index as u32 + i))
        } else {
            self.indices.extend(
                indices_out
                    .into_iter()
                    .map(|i| start_index as u32 + i)
                    .rev(),
            )
        }

        self.free_vertices = 0;
    }
    /// Chainable version of [`Self::close_hole`]
    fn with_closed_hole(mut self, flat_shading: bool) -> Self {
        self.close_hole(flat_shading);
        self
    }

    /// Inserts a convex polygon into the mesh.
    ///
    /// The polygon is triangulated using a triangle fan, a simple and efficient method
    /// that works for convex shapes.
    ///
    /// Assumes the vertices are provided in counter-clockwise order and lie on the same 2D plane.
    /// If fewer than three vertices are given, they are still added to the mesh, but no face is created.
    ///
    /// The newly inserted vertices will be selected.
    fn insert_convex_polygon(&mut self, vertices: impl IntoIterator<Item = [f32; 3]>) {
        let vertices = vertices.into_iter().collect::<Vec<_>>();

        let vertex_count = vertices.len();
        let start_index = self.vertices.len();
        let indices = (2..vertex_count)
            .flat_map(move |i| [start_index, start_index + (i - 1), start_index + i])
            .map(|i| i as u32);

        self.vertices.extend(vertices);
        self.indices.extend(indices);

        self.last_operation = vertex_count;
        self.free_vertices = 0;
    }
    /// Chainable version of [`Self::insert_convex_polygon`].
    fn with_convex_polygon(mut self, vertices: impl IntoIterator<Item = [f32; 3]>) -> Self {
        self.insert_convex_polygon(vertices);
        self
    }

    /// Inserts an arbitrary polygon into the mesh.
    ///
    /// The polygon is triangulated using the ear clipping algorithm, which can handle non-convex shapes
    /// but is generally slower than [`Self::insert_convex_polygon`].
    ///
    /// Assumes the vertices are provided in counter-clockwise order and lie on the same 2D plane.
    /// If fewer than three vertices are given, they are still added to the mesh, but no face is created.
    ///
    /// The newly inserted vertices will be selected.
    fn insert_polygon(&mut self, vertices: impl IntoIterator<Item = [f32; 3]>) {
        self.insert_vertices(vertices);
        self.close_hole(true);
    }
    /// Chainable version of [`Self::insert_polygon`].
    fn with_polygon(mut self, vertices: impl IntoIterator<Item = [f32; 3]>) -> Self {
        self.insert_polygon(vertices);
        self
    }

    /// Inserts a filled circle into the mesh, pointing up.
    ///
    /// The newly inserted vertices will be selected.
    fn insert_filled_circle(&mut self, center: [f32; 3], radius: f32, n: u32) {
        self.insert_convex_polygon(circle_vertices(center, radius, n));
    }
    /// Chainable version of [`Self::insert_filled_circle`].
    fn with_filled_circle(mut self, center: [f32; 3], radius: f32, n: u32) -> Self {
        self.insert_filled_circle(center, radius, n);
        self
    }

    /// Inserts a quad going between a and b, pointing up.
    ///
    /// The newly inserted vertices will be selected.
    fn insert_path_quad(&mut self, a: [f32; 3], b: [f32; 3], width: f32) {
        let a = Vec3::from(a);
        let b = Vec3::from(b);

        let direction = (b - a).normalize();
        let perpendicular = direction.cross(Vec3::Y) * (width / 2.0);

        let vertices = [
            (a - perpendicular).to_array(),
            (a + perpendicular).to_array(),
            (b + perpendicular).to_array(),
            (b - perpendicular).to_array(),
        ];

        self.insert_convex_polygon(vertices);
    }
    /// Chainable version of [`Self::insert_path_quad`].
    fn with_path_quad(mut self, a: [f32; 3], b: [f32; 3], width: f32) -> Self {
        self.insert_path_quad(a, b, width);
        self
    }

    /// Joins a new vertex strip to the latest vertices of the existing model
    ///
    /// The provided vertices will be selected to allow for easy chaining.
    ///
    /// # Examples
    ///
    /// ```
    /// Current: 1 2 3 4, New: 5 6 7 8, Loop: false
    /// 5 - 6 - 7 - 8
    /// |   |   |   |
    /// 1 - 2 - 3 - 4
    ///
    /// Current: 1 2 3 4, New: 5 6 7 8, Loop: true
    /// 5 - 6 - 7 - 8 - 5
    /// |   |   |   |   | ...
    /// 1 - 2 - 3 - 4 - 1
    ///
    /// Current: 1 2 3 4 5 6, New: 7 8 9, Loop: false
    ///             7 - 8 - 9
    ///             |   |   |
    /// 1 - 2 - 3 - 4 - 5 - 6
    ///
    /// Current: 1 2 3, New: 4 5 6 7 8, Loop: false
    /// 4 - 5 - 6
    /// |   |   |
    /// 1 - 2 - 3
    /// ```
    fn quad_loft(
        &mut self,
        vertices: impl IntoIterator<Item = [f32; 3]>,
        close_loop: bool,
        flat_shading: bool,
    ) {
        let new_vertices = vertices
            .into_iter()
            .take(self.last_operation)
            .collect::<Vec<_>>();
        let segment_length = new_vertices.len();
        let old_segment_start = self.vertices.len() - segment_length;

        let left_old = if flat_shading && self.free_vertices <= segment_length {
            self.vertices
                .extend_from_within(old_segment_start..old_segment_start + segment_length);
            self.vertices.len() - segment_length
        } else {
            old_segment_start
        };
        let left_new = self.vertices.len();
        self.vertices.extend(&new_vertices);
        let right_old = if flat_shading {
            self.vertices
                .extend_from_within(left_old..left_old + segment_length);
            self.vertices.len() - segment_length
        } else {
            left_old
        };
        let right_new = if flat_shading {
            self.vertices
                .extend_from_within(left_new..left_new + segment_length);
            self.vertices.len() - segment_length
        } else {
            left_new
        };

        if close_loop {
            self.indices.extend((0..segment_length).flat_map(|i| {
                let lo = (left_old + i) as u32;
                let ln = (left_new + i) as u32;
                let ro = (right_old + ((i + 1) % segment_length)) as u32;
                let rn = (right_new + ((i + 1) % segment_length)) as u32;
                [lo, ln, rn, lo, rn, ro]
            }));
        } else {
            self.indices.extend((0..segment_length - 1).flat_map(|i| {
                let lo = (left_old + i) as u32;
                let ln = (left_new + i) as u32;
                let ro = (right_old + (i + 1)) as u32;
                let rn = (right_new + (i + 1)) as u32;
                [lo, ln, rn, lo, rn, ro]
            }));
        }

        self.last_operation = segment_length;
        self.free_vertices = 0;
    }
    /// Chainable version of [`Self::quad_loft`].
    fn with_quad_loft(
        mut self,
        vertices: impl IntoIterator<Item = [f32; 3]>,
        close_loop: bool,
        flat_shading: bool,
    ) -> Self {
        self.quad_loft(vertices, close_loop, flat_shading);
        self
    }
}

fn circle_vertices(
    center: [f32; 3],
    radius: f32,
    n: u32,
) -> impl DoubleEndedIterator<Item = [f32; 3]> {
    (0..n).map(move |i| {
        let phi = (2.0 * PI) * (i as f32 / n as f32);
        [
            center[0] + phi.sin() * radius,
            center[1],
            center[2] + phi.cos() * radius,
        ]
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
    static LINE_WIDTH: f32 = 0.01;
    static LINE_HALF: f32 = LINE_WIDTH / 2f32;

    // ==== Field ====

    let border_x = geom.play_area_size.x / 2.0;
    let border_y = geom.play_area_size.y / 2.0;
    let field_x = border_x + geom.boundary_width;
    let field_y = border_y + geom.boundary_width;

    let field_mesh = CustomMeshBuilder::new().with_convex_polygon([
        [-field_x, 0.0, -field_y],
        [-field_x, 0.0, field_y],
        [field_x, 0.0, field_y],
        [field_x, 0.0, -field_y],
    ]);

    // ==== Wall ====

    let wall_mesh = CustomMeshBuilder::new()
        .with_vertices([
            [-field_x, 0.0, -field_y],
            [-field_x, 0.0, field_y],
            [field_x, 0.0, field_y],
            [field_x, 0.0, -field_y],
        ])
        .with_quad_loft(
            [
                [-field_x, WALL_HEIGHT, -field_y],
                [-field_x, WALL_HEIGHT, field_y],
                [field_x, WALL_HEIGHT, field_y],
                [field_x, WALL_HEIGHT, -field_y],
            ],
            true,
            true,
        )
        .with_quad_loft(
            [
                [-field_x - WALL_WIDTH, WALL_HEIGHT, -field_y - WALL_WIDTH],
                [-field_x - WALL_WIDTH, WALL_HEIGHT, field_y + WALL_WIDTH],
                [field_x + WALL_WIDTH, WALL_HEIGHT, field_y + WALL_WIDTH],
                [field_x + WALL_WIDTH, WALL_HEIGHT, -field_y - WALL_WIDTH],
            ],
            true,
            true,
        )
        .with_quad_loft(
            [
                [-field_x - WALL_WIDTH, 0.0, -field_y - WALL_WIDTH],
                [-field_x - WALL_WIDTH, 0.0, field_y + WALL_WIDTH],
                [field_x + WALL_WIDTH, 0.0, field_y + WALL_WIDTH],
                [field_x + WALL_WIDTH, 0.0, -field_y - WALL_WIDTH],
            ],
            true,
            true,
        );

    // ==== Goal ====

    let goal_y = geom.goal_width / 2.0;

    let goal_yellow_mesh = CustomMeshBuilder::new()
        .with_vertices([
            // Inner
            [-border_x, 0.0, -goal_y + GOAL_WALL_HALF],
            [-field_x + GOAL_WALL, 0.0, -goal_y + GOAL_WALL_HALF],
            [-field_x + GOAL_WALL, 0.0, goal_y - GOAL_WALL_HALF],
            [-border_x, 0.0, goal_y - GOAL_WALL_HALF],
            // Outer
            [-border_x, 0.0, goal_y + GOAL_WALL_HALF],
            [-field_x, 0.0, goal_y + GOAL_WALL_HALF],
            [-field_x, 0.0, -goal_y - GOAL_WALL_HALF],
            [-border_x, 0.0, -goal_y - GOAL_WALL_HALF],
        ])
        .with_quad_loft(
            [
                // Inner
                [-border_x, WALL_HEIGHT, -goal_y + GOAL_WALL_HALF],
                [-field_x + GOAL_WALL, WALL_HEIGHT, -goal_y + GOAL_WALL_HALF],
                [-field_x + GOAL_WALL, WALL_HEIGHT, goal_y - GOAL_WALL_HALF],
                [-border_x, WALL_HEIGHT, goal_y - GOAL_WALL_HALF],
                // Outer
                [-border_x, WALL_HEIGHT, goal_y + GOAL_WALL_HALF],
                [-field_x, WALL_HEIGHT, goal_y + GOAL_WALL_HALF],
                [-field_x, WALL_HEIGHT, -goal_y - GOAL_WALL_HALF],
                [-border_x, WALL_HEIGHT, -goal_y - GOAL_WALL_HALF],
            ],
            true,
            true,
        )
        .with_closed_hole(true);

    let goal_blue_mesh = CustomMeshBuilder::new()
        .with_vertices([
            // Inner
            [border_x, 0.0, goal_y - GOAL_WALL_HALF],
            [field_x - GOAL_WALL, 0.0, goal_y - GOAL_WALL_HALF],
            [field_x - GOAL_WALL, 0.0, -goal_y + GOAL_WALL_HALF],
            [border_x, 0.0, -goal_y + GOAL_WALL_HALF],
            // Outer
            [border_x, 0.0, -goal_y - GOAL_WALL_HALF],
            [field_x, 0.0, -goal_y - GOAL_WALL_HALF],
            [field_x, 0.0, goal_y + GOAL_WALL_HALF],
            [border_x, 0.0, goal_y + GOAL_WALL_HALF],
        ])
        .with_quad_loft(
            [
                // Inner
                [border_x, WALL_HEIGHT, goal_y - GOAL_WALL_HALF],
                [field_x - GOAL_WALL, WALL_HEIGHT, goal_y - GOAL_WALL_HALF],
                [field_x - GOAL_WALL, WALL_HEIGHT, -goal_y + GOAL_WALL_HALF],
                [border_x, WALL_HEIGHT, -goal_y + GOAL_WALL_HALF],
                // Outer
                [border_x, WALL_HEIGHT, -goal_y - GOAL_WALL_HALF],
                [field_x, WALL_HEIGHT, -goal_y - GOAL_WALL_HALF],
                [field_x, WALL_HEIGHT, goal_y + GOAL_WALL_HALF],
                [border_x, WALL_HEIGHT, goal_y + GOAL_WALL_HALF],
            ],
            true,
            true,
        )
        .with_closed_hole(true);

    // ==== Lines ====

    let mut line_mesh = CustomMeshBuilder::new();

    // Center circle
    line_mesh.insert_vertices(circle_vertices(
        [0.0, 0.0001, 0.0],
        CENTER_CIRCLE_RADIUS - LINE_HALF,
        128,
    ));
    line_mesh.quad_loft(
        circle_vertices([0.0, 0.0001, 0.0], CENTER_CIRCLE_RADIUS + LINE_HALF, 128),
        true,
        false,
    );

    // Center line
    line_mesh.insert_path_quad(
        [0.0, 0.0001, -border_y],
        [0.0, 0.0001, border_y],
        LINE_WIDTH,
    );

    // Border
    line_mesh.insert_vertices([
        [-border_x + LINE_HALF, 0.0001, -border_y + LINE_HALF],
        [-border_x + LINE_HALF, 0.0001, border_y - LINE_HALF],
        [border_x - LINE_HALF, 0.0001, border_y - LINE_HALF],
        [border_x - LINE_HALF, 0.0001, -border_y + LINE_HALF],
    ]);
    line_mesh.quad_loft(
        [
            [-border_x - LINE_HALF, 0.0001, -border_y - LINE_HALF],
            [-border_x - LINE_HALF, 0.0001, border_y + LINE_HALF],
            [border_x + LINE_HALF, 0.0001, border_y + LINE_HALF],
            [border_x + LINE_HALF, 0.0001, -border_y - LINE_HALF],
        ],
        true,
        false,
    );

    let defense_x = border_x - geom.defense_size.x;
    let defense_y = geom.defense_size.y / 2.0;

    // Defense area yellow
    line_mesh.insert_vertices([
        [-border_x, 0.0001, defense_y - LINE_HALF],
        [-defense_x - LINE_HALF, 0.0001, defense_y - LINE_HALF],
        [-defense_x - LINE_HALF, 0.0001, -defense_y + LINE_HALF],
        [-border_x, 0.0001, -defense_y + LINE_HALF],
    ]);
    line_mesh.quad_loft(
        [
            [-border_x, 0.0001, defense_y + LINE_HALF],
            [-defense_x + LINE_HALF, 0.0001, defense_y + LINE_HALF],
            [-defense_x + LINE_HALF, 0.0001, -defense_y - LINE_HALF],
            [-border_x, 0.0001, -defense_y - LINE_HALF],
        ],
        true,
        false,
    );

    // Defense area blue
    line_mesh.insert_vertices([
        [border_x, 0.0001, -defense_y + LINE_HALF],
        [defense_x + LINE_HALF, 0.0001, -defense_y + LINE_HALF],
        [defense_x + LINE_HALF, 0.0001, defense_y - LINE_HALF],
        [border_x, 0.0001, defense_y - LINE_HALF],
    ]);
    line_mesh.quad_loft(
        [
            [border_x, 0.0001, -defense_y - LINE_HALF],
            [defense_x - LINE_HALF, 0.0001, -defense_y - LINE_HALF],
            [defense_x - LINE_HALF, 0.0001, defense_y + LINE_HALF],
            [border_x, 0.0001, defense_y + LINE_HALF],
        ],
        true,
        false,
    );

    let mut world = World::new();

    world.spawn_batch([
        (
            Mesh3d(mesh_assets.add(field_mesh.build(false))),
            MeshMaterial3d(material_assets.add(field_mat)),
        ),
        (
            Mesh3d(mesh_assets.add(wall_mesh.build(false))),
            MeshMaterial3d(material_assets.add(wall_mat)),
        ),
        (
            Mesh3d(mesh_assets.add(goal_yellow_mesh.build(false))),
            MeshMaterial3d(material_assets.add(goal_y_mat)),
        ),
        (
            Mesh3d(mesh_assets.add(goal_blue_mesh.build(false))),
            MeshMaterial3d(material_assets.add(goal_b_mat)),
        ),
        (
            Mesh3d(mesh_assets.add(line_mesh.build(false))),
            MeshMaterial3d(material_assets.add(line_mat)),
        ),
    ]);

    Scene::new(world)
}

pub fn circle_visualization_mesh(n: u32, radius: f32) -> Mesh {
    CustomMeshBuilder::new()
        .with_filled_circle([0.0, 0.0, 0.0], radius, n)
        .build(false)
}

pub fn polygon_visualization_mesh(vertices: &[Vec2]) -> Mesh {
    assert!(vertices.len() >= 3);

    let is_ccw = vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .map(|(a, b)| (b.x - a.x) * (b.y + a.y))
        .sum::<f32>()
        > 0.0;

    if is_ccw {
        CustomMeshBuilder::new()
            .with_polygon(vertices.iter().map(|p| [p.x, 0.0, p.y]))
            .build(false)
    } else {
        CustomMeshBuilder::new()
            .with_polygon(vertices.iter().map(|p| [p.x, 0.0, p.y]).rev())
            .build(false)
    }
}

pub fn path_visualization_mesh(path: &[Vec2], width: f32) -> Mesh {
    assert!(!path.is_empty());
    assert!(width >= 0.0);

    let mut mesh = CustomMeshBuilder::new();

    for point in path {
        mesh.insert_filled_circle([point.x, 0.0, point.y], width / 2.0, 16);
    }
    for edge in path.windows(2).filter(|e| e[0] != e[1]) {
        let a = [edge[0].x, 0.0, edge[0].y];
        let b = [edge[1].x, 0.0, edge[1].y];
        mesh.insert_path_quad(a, b, width);
    }

    mesh.build(false)
}
