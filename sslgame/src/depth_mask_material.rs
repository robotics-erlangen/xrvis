use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

// TODO: statically include shader as a string
const SHADER_ASSET_PATH: &str = "shaders/discard_fragment.wgsl";

/// Material that makes objects only show up in the depth prepass, but discards them during actual rendering.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct DepthMaskMaterial {}

impl Material for DepthMaskMaterial {
    fn fragment_shader() -> ShaderRef {
        SHADER_ASSET_PATH.into()
    }
}
