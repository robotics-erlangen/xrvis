mod panorbit_cam;

use crate::viewport::panorbit_cam::ViewportCamera;
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::ecs::template::TemplateContext;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureDimension, TextureFormat, TextureUsages};

pub fn viewport_plugin(app: &mut App) {
    app.add_plugins(panorbit_cam::panorbit_camera_plugin);
}

pub fn scene() -> impl Scene {
    bsn! {
        ~ViewportTemplate
    }
}

#[derive(Default)]
struct ViewportTemplate;

impl Template for ViewportTemplate {
    type Output = ViewportNode;

    fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
        let mut image = Image::new_uninit(
            default(),
            TextureDimension::D2,
            TextureFormat::Bgra8UnormSrgb,
            RenderAssetUsages::all(),
        );
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_DST
            | TextureUsages::RENDER_ATTACHMENT;
        let image_handle = context.resource_mut::<Assets<Image>>().add(image);

        let viewport_cam = context.entity.world_scope(move |world| {
            world
                .spawn((
                    ViewportCamera::default(),
                    Transform::from_translation(Vec3::new(0.0, 9.0, 10.0)),
                    Camera3d::default(),
                    Camera {
                        order: -1,
                        ..default()
                    },
                    DepthPrepass,
                    RenderTarget::Image(image_handle.into()),
                ))
                .id()
        });

        context.entity.insert((
            Node {
                flex_grow: 1.0,
                height: percent(100),
                border_radius: px(4).into(),
                ..default()
            },
            Hovered::default(),
        ));

        Ok(ViewportNode {
            camera: { Some(viewport_cam) },
        })
    }

    fn clone_template(&self) -> Self {
        Self
    }
}
