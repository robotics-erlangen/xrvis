use crate::proto::VisShape;
use bevy::mesh::Mesh;

pub mod simple;

pub fn visualization_mesh<'a>(vis_list: impl IntoIterator<Item = &'a VisShape>) -> Mesh {
    simple::simple_vis_mesh(vis_list)
}
