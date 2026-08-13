use crate::mesh_gen::{CustomMeshBuilder, bevy_col, circle_vertices, with_col};
use crate::proto;
use crate::proto::remote::vis_shape::Geom;
use crate::proto::remote::{VisShape, Visualization};
use bevy::mesh::Mesh;
use tracing::warn;

const Z_HEIGHT: f32 = 0.01;
const DEFAULT_LINE_WIDTH: f32 = 0.01;

/// Builds a single mesh containing all geometry from the visualization list.
pub fn simple_vis_mesh(vis_list: &[&Visualization]) -> Mesh {
    let mut mesh = CustomMeshBuilder::new();

    for (vis_id, part) in vis_list
        .iter()
        .flat_map(|v| v.shape.iter().map(|p| (&v.id, p)))
    {
        match part.geom.as_ref() {
            Some(Geom::Circle(_)) => circle_vis(&mut mesh, part),
            Some(Geom::Polygon(poly)) if !poly.point.is_empty() => polygon_vis(&mut mesh, part),
            Some(Geom::Path(path)) if !path.point.is_empty() => path_vis(&mut mesh, part),
            other => {
                // TODO: Print the string name of the broken visualization
                warn!(
                    "Invalid visualization part in {vis_id}: {}",
                    other.map(|_| "Empty geometry").unwrap_or("No geometry")
                );
                continue;
            }
        }
    }

    mesh.build(false)
}

fn vis_point(p_2d: &proto::remote::Point) -> [f32; 3] {
    [p_2d.x, Z_HEIGHT, p_2d.y]
}
fn border_width(shape: &VisShape) -> f32 {
    shape
        .border_style
        .and_then(|style| style.width)
        .unwrap_or(DEFAULT_LINE_WIDTH)
}

fn circle_vis(builder: &mut CustomMeshBuilder, part: &VisShape) {
    let Some(Geom::Circle(c)) = &part.geom else {
        return;
    };

    let center = [c.center.x, Z_HEIGHT, c.center.y];
    let radius = c.radius;

    // Dynamic vertex count based on radius
    let resolution = (radius as u32 * 64).max(32);

    if let Some(fill) = part.fill_color {
        let fill_radius = if part.border_style.is_some() {
            radius - border_width(part) / 2.0
        } else {
            radius
        };
        builder.insert_filled_circle(center, fill_radius, resolution, bevy_col(fill));
    }

    if let Some(border) = part.border_style {
        let border_col = bevy_col(border.color.unwrap_or_default());

        builder.insert_vertices(with_col(
            circle_vertices(center, radius - (border_width(part) / 2.), resolution),
            border_col,
        ));
        builder.quad_loft(
            with_col(
                circle_vertices(center, radius + (border_width(part) / 2.), resolution),
                border_col,
            ),
            true,
            false,
        );
    }
}

fn polygon_vis(builder: &mut CustomMeshBuilder, part: &VisShape) {
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
            builder.insert_polygon(with_col(poly.point.iter().map(vis_point), fill_col));
        } else {
            builder.insert_polygon(with_col(poly.point.iter().map(vis_point).rev(), fill_col));
        }
    }
    if let Some(border) = part.border_style {
        let border_col = bevy_col(border.color.unwrap_or_default());

        for point in &poly.point {
            builder.insert_filled_circle(
                vis_point(point),
                border_width(part) / 2.0,
                12,
                border_col,
            );
        }
        for edge in poly.point.windows(2) {
            let a = vis_point(&edge[0]);
            let b = vis_point(&edge[1]);
            builder.insert_path_quad(a, b, border_width(part), border_col);
        }
        // Add final closing edge
        let a = poly.point.last().map(vis_point).unwrap();
        let b = poly.point.first().map(vis_point).unwrap();
        builder.insert_path_quad(a, b, border_width(part), border_col);
    }
}

fn path_vis(builder: &mut CustomMeshBuilder, part: &VisShape) {
    let Some(Geom::Path(path)) = &part.geom else {
        return;
    };

    let color = bevy_col(
        part.fill_color
            .unwrap_or_else(|| part.border_style.and_then(|b| b.color).unwrap_or_default()),
    );

    for point in &path.point {
        builder.insert_filled_circle(
            [point.x, Z_HEIGHT, point.y],
            border_width(part) / 2.0,
            16,
            color,
        );
    }
    for edge in path.point.windows(2) {
        builder.insert_path_quad(
            vis_point(&edge[0]),
            vis_point(&edge[1]),
            border_width(part),
            color,
        );
    }
}
