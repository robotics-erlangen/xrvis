use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureDimension, TextureFormat, TextureUsages};

pub fn viewport_plugin(app: &mut App) {
    // TODO: Panorbit camera
}

#[derive(Component)]
struct ViewportCamera;

pub fn spawn(commands: &mut Commands, images: &mut Assets<Image>) -> Entity {
    let mut image = Image::new_uninit(
        default(),
        TextureDimension::D2,
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::all(),
    );
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image_handle = images.add(image);

    let viewport_cam = commands
        .spawn((
            ViewportCamera,
            Camera3d::default(),
            Camera {
                order: -1,
                ..default()
            },
            RenderTarget::Image(image_handle.into()),
            bevy::core_pipeline::prepass::DepthPrepass,
            Transform::from_xyz(0.0, 8.0, 9.0)
                .with_rotation(Quat::from_rotation_x(-45_f32.to_radians())),
        ))
        .id();

    commands
        .spawn((
            Node {
                flex_grow: 1.0,
                height: percent(100),
                ..default()
            },
            ViewportNode {
                camera: { Some(viewport_cam) },
            },
        ))
        .id()
}
