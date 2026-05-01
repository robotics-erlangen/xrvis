pub mod field;
pub mod vis;

use crate::proto;
use bevy::asset::RenderAssetUsages;
use bevy::color::{Color, ColorToComponents};
use bevy::math::Vec3;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};
use earcut::Earcut;
use std::f32::consts::PI;
use std::iter;

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
///         [1., 1., 0.],
///         [1., 1., 1.],
///         [0., 1., 1.],
///     ], true, true)
///     // Close top face
///     .with_closed_hole(true)
///     .build(false);
/// ```
pub struct CustomMeshBuilder {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
    last_operation: usize,
    free_vertices: usize,
}

#[allow(dead_code)]
impl CustomMeshBuilder {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            colors: Vec::new(),
            indices: Vec::new(),
            last_operation: 0,
            free_vertices: 0,
        }
    }

    // Not using bevy's MeshBuilder trait because taking ownership makes sense here
    fn build(self, double_sided: bool) -> Mesh {
        let mut normals = vec![Vec3::ZERO; self.positions.len()];

        self.indices.chunks(3).for_each(|tri| {
            let a = Vec3::from_array(self.positions[tri[0] as usize]);
            let b = Vec3::from_array(self.positions[tri[1] as usize]);
            let c = Vec3::from_array(self.positions[tri[2] as usize]);
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
            VertexAttributeValues::Float32x3(self.positions),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            VertexAttributeValues::Float32x3(normals.into_iter().map(|n| n.to_array()).collect()),
        )
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            VertexAttributeValues::Float32x4(self.colors),
        )
    }

    /// Insert raw vertex data without creating a new face. Mostly used as a starting point for [`Self::quad_loft`].
    ///
    /// The inserted vertices will be selected.
    fn insert_vertices(&mut self, vertices: impl IntoIterator<Item = ([f32; 3], [f32; 4])>) {
        let (positions, colors): (Vec<_>, Vec<_>) = vertices.into_iter().unzip();

        let count = positions.len();
        self.positions.extend(positions);
        self.colors.extend(colors);

        self.last_operation = count;
        self.free_vertices += count;
    }
    /// Chainable version of [`Self::insert_vertices`].
    fn with_vertices(mut self, vertices: impl IntoIterator<Item = ([f32; 3], [f32; 4])>) -> Self {
        self.insert_vertices(vertices);
        self
    }

    /// Creates a new face out the currently selected vertices.
    /// See implementation for winding details, but it usually behaves as expected.
    ///
    /// The used vertices will stay selected.
    fn close_hole(&mut self, flat_shading: bool) {
        if flat_shading {
            self.positions
                .extend_from_within((self.positions.len() - self.last_operation)..);
            self.colors
                .extend_from_within((self.colors.len() - self.last_operation)..);
        }

        let start_index = self.positions.len() - self.last_operation;

        let mut vert_2d = Vec::new();
        earcut::utils3d::project3d_to_2d(
            &self.positions[start_index..],
            self.positions.len() - start_index,
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
    fn insert_convex_polygon(&mut self, vertices: impl IntoIterator<Item = ([f32; 3], [f32; 4])>) {
        let (positions, colors): (Vec<_>, Vec<_>) = vertices.into_iter().unzip();

        let vertex_count = positions.len();
        let start_index = self.positions.len();
        let indices = (2..vertex_count)
            .flat_map(move |i| [start_index, start_index + (i - 1), start_index + i])
            .map(|i| i as u32);

        self.positions.extend(positions);
        self.colors.extend(colors);
        self.indices.extend(indices);

        self.last_operation = vertex_count;
        self.free_vertices = 0;
    }
    /// Chainable version of [`Self::insert_convex_polygon`].
    fn with_convex_polygon(
        mut self,
        vertices: impl IntoIterator<Item = ([f32; 3], [f32; 4])>,
    ) -> Self {
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
    fn insert_polygon(&mut self, vertices: impl IntoIterator<Item = ([f32; 3], [f32; 4])>) {
        self.insert_vertices(vertices);
        self.close_hole(true);
    }
    /// Chainable version of [`Self::insert_polygon`].
    fn with_polygon(mut self, vertices: impl IntoIterator<Item = ([f32; 3], [f32; 4])>) -> Self {
        self.insert_polygon(vertices);
        self
    }

    /// Inserts a filled circle into the mesh, pointing up.
    ///
    /// The newly inserted vertices will be selected.
    fn insert_filled_circle(&mut self, center: [f32; 3], radius: f32, n: u32, color: Color) {
        self.insert_convex_polygon(with_col(circle_vertices(center, radius, n), color));
    }
    /// Chainable version of [`Self::insert_filled_circle`].
    fn with_filled_circle(mut self, center: [f32; 3], radius: f32, n: u32, color: Color) -> Self {
        self.insert_filled_circle(center, radius, n, color);
        self
    }

    /// Inserts a quad going between a and b, pointing up.
    ///
    /// The newly inserted vertices will be selected.
    fn insert_path_quad(&mut self, a: [f32; 3], b: [f32; 3], width: f32, color: Color) {
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

        self.insert_convex_polygon(with_col(vertices, color));
    }
    /// Chainable version of [`Self::insert_path_quad`].
    fn with_path_quad(mut self, a: [f32; 3], b: [f32; 3], width: f32, color: Color) -> Self {
        self.insert_path_quad(a, b, width, color);
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
        vertices: impl IntoIterator<Item = ([f32; 3], [f32; 4])>,
        close_loop: bool,
        flat_shading: bool,
    ) {
        let (new_positions, new_colors) = vertices
            .into_iter()
            .take(self.last_operation)
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let len = new_positions.len();
        if len == 0 {
            return;
        }

        // Start index of the last existing vertices
        let old = self.positions.len() - len;

        // Handle vertex duplication for flat shading to ensure unique normals per quad face
        // Selects the start indices for each quad corner to allow reusing or separating
        // vertices depending on if that transition should be blended or not.
        let (first_old, first_new, second_old, second_new) = if flat_shading {
            // Flat shading -> Each quad needs fully unique vertices -> Each position gets its own duplicated segment

            // Reuse old vertices if they are free
            let f_old = if self.free_vertices >= len {
                old
            } else {
                let f_old = self.positions.len();
                self.positions.extend_from_within(old..old + len);
                self.colors.extend_from_within(old..old + len);
                f_old
            };

            let f_new = self.positions.len();
            self.positions.extend(&new_positions);
            self.colors.extend(&new_colors);

            let s_old = self.positions.len();
            self.positions.extend_from_within(f_old..f_old + len);
            self.colors.extend_from_within(f_old..f_old + len);

            let s_new = self.positions.len();
            self.positions.extend_from_within(f_new..f_new + len);
            self.colors.extend_from_within(f_new..f_new + len);

            (f_old, f_new, s_old, s_new)
        } else {
            // Smooth shading -> Quads can share vertices
            let new = self.positions.len();
            self.positions.extend(new_positions);
            self.colors.extend(new_colors);
            (old, new, old, new)
        };

        let num_quads = if close_loop { len } else { len - 1 };

        for i in 0..num_quads {
            let next_i = (i + 1) % len;

            let curr_old = (first_old + i) as u32;
            let curr_new = (first_new + i) as u32;
            let next_old = (second_old + next_i) as u32;
            let next_new = (second_new + next_i) as u32;

            self.indices
                .extend([curr_old, curr_new, next_new, curr_old, next_new, next_old]);
        }

        self.last_operation = len;
        self.free_vertices = 0;
    }
    /// Chainable version of [`Self::quad_loft`].
    fn with_quad_loft(
        mut self,
        vertices: impl IntoIterator<Item = ([f32; 3], [f32; 4])>,
        close_loop: bool,
        flat_shading: bool,
    ) -> Self {
        self.quad_loft(vertices, close_loop, flat_shading);
        self
    }
}

// ==== Helper functions ====

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

fn bevy_col(proto_col: proto::remote::Color) -> Color {
    Color::srgba_u8(
        proto_col.red as u8,
        proto_col.green as u8,
        proto_col.blue as u8,
        proto_col.alpha as u8,
    )
}

fn with_col(
    positions: impl IntoIterator<Item = [f32; 3]>,
    color: Color,
) -> impl Iterator<Item = ([f32; 3], [f32; 4])> {
    positions
        .into_iter()
        .zip(iter::repeat(color.to_linear().to_f32_array()))
}
