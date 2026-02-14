use crate::proto::remote::vis_part::Geom;
use crate::proto::remote::{VisPart, Visualization};
use crate::{AvailableVisualizations, FieldGeometry, proto};
use bevy::asset::RenderAssetUsages;
use bevy::color::Color;
use bevy::math::Vec3;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use earcut::Earcut;
use std::f32::consts::PI;
use std::iter;

// Visualization parameters
const Z_HEIGHT: f32 = 0.01;
const LINE_WIDTH: f32 = 0.01;

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
        let segment_length = new_positions.len();
        let old_segment_start = self.positions.len() - segment_length;

        let left_old = if flat_shading && self.free_vertices <= segment_length {
            self.positions
                .extend_from_within(old_segment_start..old_segment_start + segment_length);
            self.colors
                .extend_from_within(old_segment_start..old_segment_start + segment_length);
            self.positions.len() - segment_length
        } else {
            old_segment_start
        };
        let left_new = self.positions.len();
        self.positions.extend(&new_positions);
        self.colors.extend(&new_colors);
        let right_old = if flat_shading {
            self.positions
                .extend_from_within(left_old..left_old + segment_length);
            self.colors
                .extend_from_within(left_old..left_old + segment_length);
            self.positions.len() - segment_length
        } else {
            left_old
        };
        let right_new = if flat_shading {
            self.positions
                .extend_from_within(left_new..left_new + segment_length);
            self.colors
                .extend_from_within(left_new..left_new + segment_length);
            self.positions.len() - segment_length
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
        vertices: impl IntoIterator<Item = ([f32; 3], [f32; 4])>,
        close_loop: bool,
        flat_shading: bool,
    ) -> Self {
        self.quad_loft(vertices, close_loop, flat_shading);
        self
    }

    fn circle_vis(&mut self, part: &VisPart) {
        let Some(Geom::Circle(c)) = &part.geom else {
            return;
        };

        let center = [c.p_x, Z_HEIGHT, c.p_y];
        let radius = c.radius;

        // Dynamic vertex count based on radius
        let resolution = (radius as u32 * 64).max(32);

        match (part.border_style, part.fill_color) {
            (Some(border), Some(fill)) => {
                self.insert_filled_circle(
                    center,
                    radius - (LINE_WIDTH / 2.),
                    resolution,
                    bevy_col(fill),
                );
                self.quad_loft(
                    with_col(
                        circle_vertices(center, radius + (LINE_WIDTH / 2.), resolution),
                        bevy_col(border.color.unwrap_or_default()),
                    ),
                    true,
                    true,
                );
            }
            (Some(border), None) => {
                let border_col = bevy_col(border.color.unwrap_or_default());

                self.insert_vertices(with_col(
                    circle_vertices(center, radius - (LINE_WIDTH / 2.), resolution),
                    border_col,
                ));
                self.quad_loft(
                    with_col(
                        circle_vertices(center, radius + (LINE_WIDTH / 2.), resolution),
                        border_col,
                    ),
                    true,
                    false,
                );
            }
            (None, Some(fill)) => {
                self.insert_filled_circle(center, radius, 32, bevy_col(fill));
            }
            _ => {}
        }
    }

    fn polygon_vis(&mut self, part: &VisPart) {
        let Some(Geom::Polygon(poly)) = &part.geom else {
            return;
        };

        if poly.point.len() < 3 {
            warn!(
                "Tried to build polygon visualization with less than 3 points.\
                Degenerate geometry should have already been filtered by the host."
            );
            return;
        }

        let is_ccw = poly
            .point
            .iter()
            .zip(poly.point.iter().cycle().skip(1))
            .map(|(a, b)| (b.x - a.x) * (b.y + a.y))
            .sum::<f32>()
            > 0.0;

        if let Some(fill) = part.fill_color {
            let fill_col = bevy_col(fill);

            if is_ccw {
                self.insert_polygon(with_col(poly.point.iter().map(vis_point), fill_col));
            } else {
                self.insert_polygon(with_col(poly.point.iter().map(vis_point).rev(), fill_col));
            }
        }
        if let Some(border) = part.border_style {
            let border_col = bevy_col(border.color.unwrap_or_default());

            for point in &poly.point {
                self.insert_filled_circle(vis_point(point), LINE_WIDTH / 2.0, 12, border_col);
            }
            for edge in poly.point.windows(2) {
                let a = vis_point(&edge[0]);
                let b = vis_point(&edge[1]);
                self.insert_path_quad(a, b, LINE_WIDTH, border_col);
            }
            // Add final closing edge
            let a = poly.point.last().map(vis_point).unwrap();
            let b = poly.point.first().map(vis_point).unwrap();
            self.insert_path_quad(a, b, LINE_WIDTH, border_col);
        }
    }

    fn path_vis(&mut self, part: &VisPart) {
        let Some(Geom::Path(path)) = &part.geom else {
            return;
        };

        let color = bevy_col(
            part.fill_color
                .unwrap_or_else(|| part.border_style.and_then(|b| b.color).unwrap_or_default()),
        );

        for point in &path.point {
            self.insert_filled_circle([point.x, Z_HEIGHT, point.y], LINE_WIDTH / 2.0, 16, color);
        }
        for edge in path.point.windows(2) {
            self.insert_path_quad(vis_point(&edge[0]), vis_point(&edge[1]), LINE_WIDTH, color);
        }
    }
}

/// Builds a single mesh containing all geometry from the visualization list.
pub fn visualization_mesh(
    vis_list: &[Visualization],
    debug_names: Option<&AvailableVisualizations>,
) -> Mesh {
    let mut mesh = CustomMeshBuilder::new();

    for (vis_id, part) in vis_list
        .iter()
        .flat_map(|v| v.part.iter().map(move |p| (&v.id, p)))
    {
        match part.geom.as_ref() {
            Some(Geom::Circle(_)) => mesh.circle_vis(part),
            Some(Geom::Polygon(poly)) if !poly.point.is_empty() => mesh.polygon_vis(part),
            Some(Geom::Path(path)) if !path.point.is_empty() => mesh.path_vis(part),
            other => {
                warn!(
                    "Invalid visualization part in {}: {}",
                    debug_names
                        .and_then(|names| names.visualizations.get(vis_id))
                        .unwrap_or(&vis_id.to_string()),
                    other.map(|_| "Empty geometry").unwrap_or("No geometry")
                );
                continue;
            }
        }
    }

    mesh.build(false)
}

pub fn field_mesh(geom: &FieldGeometry) -> Mesh {
    let field_col = Color::srgba_u8(0, 135, 0, 255);
    let wall_col = Color::srgba_u8(0, 0, 0, 255);
    let goal_y_col = Color::srgba_u8(255, 255, 0, 255);
    let goal_b_col = Color::srgba_u8(0, 0, 255, 255);
    let line_col = Color::srgba_u8(255, 255, 255, 255);

    static WALL_WIDTH: f32 = 0.04;
    static WALL_HEIGHT: f32 = 0.16;
    static GOAL_WALL: f32 = 0.03;
    static GOAL_WALL_HALF: f32 = GOAL_WALL / 2f32;
    static CENTER_CIRCLE_RADIUS: f32 = 0.5;
    static LINE_WIDTH: f32 = 0.01;
    static LINE_HALF: f32 = LINE_WIDTH / 2f32;

    let mut mesh = CustomMeshBuilder::new();

    // ==== Field ====

    let border_x = geom.play_area_size.x / 2.0;
    let border_y = geom.play_area_size.y / 2.0;
    let field_x = border_x + geom.boundary_width;
    let field_y = border_y + geom.boundary_width;

    mesh.insert_convex_polygon(with_col(
        [
            [-field_x, 0.0, -field_y],
            [-field_x, 0.0, field_y],
            [field_x, 0.0, field_y],
            [field_x, 0.0, -field_y],
        ],
        field_col,
    ));

    // ==== Wall ====

    mesh.insert_vertices(with_col(
        [
            [-field_x, 0.0, -field_y],
            [-field_x, 0.0, field_y],
            [field_x, 0.0, field_y],
            [field_x, 0.0, -field_y],
        ],
        wall_col,
    ));
    mesh.quad_loft(
        with_col(
            [
                [-field_x, WALL_HEIGHT, -field_y],
                [-field_x, WALL_HEIGHT, field_y],
                [field_x, WALL_HEIGHT, field_y],
                [field_x, WALL_HEIGHT, -field_y],
            ],
            wall_col,
        ),
        true,
        true,
    );
    mesh.quad_loft(
        with_col(
            [
                [-field_x - WALL_WIDTH, WALL_HEIGHT, -field_y - WALL_WIDTH],
                [-field_x - WALL_WIDTH, WALL_HEIGHT, field_y + WALL_WIDTH],
                [field_x + WALL_WIDTH, WALL_HEIGHT, field_y + WALL_WIDTH],
                [field_x + WALL_WIDTH, WALL_HEIGHT, -field_y - WALL_WIDTH],
            ],
            wall_col,
        ),
        true,
        true,
    );
    mesh.quad_loft(
        with_col(
            [
                [-field_x - WALL_WIDTH, 0.0, -field_y - WALL_WIDTH],
                [-field_x - WALL_WIDTH, 0.0, field_y + WALL_WIDTH],
                [field_x + WALL_WIDTH, 0.0, field_y + WALL_WIDTH],
                [field_x + WALL_WIDTH, 0.0, -field_y - WALL_WIDTH],
            ],
            wall_col,
        ),
        true,
        true,
    );

    // ==== Goal ====

    let goal_y = geom.goal_width / 2.0;

    // Yellow goal
    mesh.insert_vertices(with_col(
        [
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
        ],
        goal_y_col,
    ));
    mesh.quad_loft(
        with_col(
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
            goal_y_col,
        ),
        true,
        true,
    );
    mesh.close_hole(true);

    // Blue goal
    mesh.insert_vertices(with_col(
        [
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
        ],
        goal_b_col,
    ));
    mesh.quad_loft(
        with_col(
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
            goal_b_col,
        ),
        true,
        true,
    );
    mesh.close_hole(true);

    // ==== Lines ====

    // Center circle
    mesh.insert_vertices(with_col(
        circle_vertices([0.0, 0.0001, 0.0], CENTER_CIRCLE_RADIUS - LINE_HALF, 128),
        line_col,
    ));
    mesh.quad_loft(
        with_col(
            circle_vertices([0.0, 0.0001, 0.0], CENTER_CIRCLE_RADIUS + LINE_HALF, 128),
            line_col,
        ),
        true,
        false,
    );

    // Center line
    mesh.insert_path_quad(
        [0.0, 0.0001, -border_y],
        [0.0, 0.0001, border_y],
        LINE_WIDTH,
        line_col,
    );

    // Border
    mesh.insert_vertices(with_col(
        [
            [-border_x + LINE_HALF, 0.0001, -border_y + LINE_HALF],
            [-border_x + LINE_HALF, 0.0001, border_y - LINE_HALF],
            [border_x - LINE_HALF, 0.0001, border_y - LINE_HALF],
            [border_x - LINE_HALF, 0.0001, -border_y + LINE_HALF],
        ],
        line_col,
    ));
    mesh.quad_loft(
        with_col(
            [
                [-border_x - LINE_HALF, 0.0001, -border_y - LINE_HALF],
                [-border_x - LINE_HALF, 0.0001, border_y + LINE_HALF],
                [border_x + LINE_HALF, 0.0001, border_y + LINE_HALF],
                [border_x + LINE_HALF, 0.0001, -border_y - LINE_HALF],
            ],
            line_col,
        ),
        true,
        false,
    );

    let defense_x = border_x - geom.defense_size.x;
    let defense_y = geom.defense_size.y / 2.0;

    // Defense area yellow
    mesh.insert_vertices(with_col(
        [
            [-border_x, 0.0001, defense_y - LINE_HALF],
            [-defense_x - LINE_HALF, 0.0001, defense_y - LINE_HALF],
            [-defense_x - LINE_HALF, 0.0001, -defense_y + LINE_HALF],
            [-border_x, 0.0001, -defense_y + LINE_HALF],
        ],
        line_col,
    ));
    mesh.quad_loft(
        with_col(
            [
                [-border_x, 0.0001, defense_y + LINE_HALF],
                [-defense_x + LINE_HALF, 0.0001, defense_y + LINE_HALF],
                [-defense_x + LINE_HALF, 0.0001, -defense_y - LINE_HALF],
                [-border_x, 0.0001, -defense_y - LINE_HALF],
            ],
            line_col,
        ),
        true,
        false,
    );

    // Defense area blue
    mesh.insert_vertices(with_col(
        [
            [border_x, 0.0001, -defense_y + LINE_HALF],
            [defense_x + LINE_HALF, 0.0001, -defense_y + LINE_HALF],
            [defense_x + LINE_HALF, 0.0001, defense_y - LINE_HALF],
            [border_x, 0.0001, defense_y - LINE_HALF],
        ],
        line_col,
    ));
    mesh.quad_loft(
        with_col(
            [
                [border_x, 0.0001, -defense_y - LINE_HALF],
                [defense_x - LINE_HALF, 0.0001, -defense_y - LINE_HALF],
                [defense_x - LINE_HALF, 0.0001, defense_y + LINE_HALF],
                [border_x, 0.0001, defense_y + LINE_HALF],
            ],
            line_col,
        ),
        true,
        false,
    );

    mesh.build(false)
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

fn vis_point(p_2d: &proto::remote::Point) -> [f32; 3] {
    [p_2d.x, Z_HEIGHT, p_2d.y]
}
