use crate::FieldGeometry;
use crate::mesh_gen::{CustomMeshBuilder, circle_vertices, with_col};
use bevy::color::Color;
use bevy::mesh::Mesh;

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
