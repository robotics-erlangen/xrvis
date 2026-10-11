use crate::proto::remote::Visualization;
use bevy::mesh::Mesh;

pub mod simple;

pub fn visualization_mesh(vis_list: &[&Visualization]) -> Mesh {
    simple::simple_vis_mesh(vis_list)
}
