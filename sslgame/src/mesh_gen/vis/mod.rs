pub mod simple;

use crate::AvailableVisualizations;
use crate::proto::remote::Visualization;
use bevy::mesh::Mesh;

pub fn visualization_mesh(
    vis: &Visualization, // Only supports a single vis for now because each one can have its own theme
    debug_names: Option<&AvailableVisualizations>,
) -> Mesh {
    match vis.shape_theme.as_deref() {
        _ => simple::simple_vis_mesh(&[vis], debug_names),
    }
}
