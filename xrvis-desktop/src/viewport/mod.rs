use crate::viewport::panorbit_cam::PanOrbitCamera;
use crate::viewport::robot_cam::RobotCam;
use bevy::asset::RenderAssetUsages;
use bevy::camera::{CameraUpdateSystems, RenderTarget};
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::template::TemplateContext;
use bevy::ecs::world::DeferredWorld;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureDimension, TextureFormat, TextureUsages};
use sslgame::fields::Field;
use sslgame::hosts::robot_control::{RobotControlledByGamepad, WorldSpaceRobotControl};

mod camera_tabs;
mod panorbit_cam;
mod robot_cam;

pub fn viewport_plugin(app: &mut App) {
    app.add_plugins(camera_tabs::camera_tabs_plugin);
    app.add_plugins(panorbit_cam::panorbit_camera_plugin);
    app.add_plugins(robot_cam::robot_cam_plugin);
    app.add_systems(Update, update_global_control_dir);
    app.add_systems(
        PostUpdate,
        ensure_viewport_has_camera.before(CameraUpdateSystems),
    );
}

#[derive(Component)]
#[relationship(relationship_target = DefaultCameraOfViewport)]
struct DefaultViewportCamera(Entity);
#[derive(Component)]
#[relationship_target(relationship = DefaultViewportCamera)]
struct DefaultCameraOfViewport(Entity);

#[derive(Component)]
struct ViewportRenderTarget(Handle<Image>);

pub fn scene() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            height: percent(100),
            flex_grow: 1.0,
            row_gap: px(6),
        }
        Children [
            @camera_tabs::scene(#Viewport)
            --
            #Viewport
            ~ViewportTemplate
        ]
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

        let context_entity = context.entity.id();
        let viewport_cam = context.entity.world_scope(|world| {
            world
                .spawn((
                    PanOrbitCamera::new(context_entity, Vec3::ZERO),
                    Transform::from_translation(Vec3::new(0.0, 9.0, 10.0)),
                    Camera3d::default(),
                    Camera {
                        order: -1,
                        // Will be immediately activated by ViewportShowsCamera
                        is_active: false,
                        ..default()
                    },
                    DepthPrepass,
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
            ViewportRenderTarget(image_handle),
            ViewportShowsCamera(viewport_cam),
            DefaultViewportCamera(viewport_cam),
        ));

        Ok(ViewportNode {
            camera: Some(viewport_cam),
        })
    }

    fn clone_template(&self) -> Self {
        Self
    }
}

#[derive(Component)]
#[relationship(relationship_target = CameraActiveForViewport)]
#[component(on_insert = on_viewport_insert_cam, on_discard = on_viewport_discard_cam)]
struct ViewportShowsCamera(Entity);

fn on_viewport_insert_cam(mut world: DeferredWorld, context: HookContext) {
    let camera_entity = world
        .entity(context.entity)
        .get::<ViewportShowsCamera>()
        .unwrap()
        .0;

    if let Some(mut viewport_node) = world.entity_mut(context.entity).get_mut::<ViewportNode>() {
        viewport_node.camera = Some(camera_entity);
    }
}

fn on_viewport_discard_cam(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut viewport_node) = world.entity_mut(context.entity).get_mut::<ViewportNode>() {
        viewport_node.camera = None;
    }
}

#[derive(Component)]
#[relationship_target(relationship = ViewportShowsCamera)]
#[component(on_insert = on_cam_insert_viewport, on_discard = on_cam_discard_viewport)]
struct CameraActiveForViewport(Entity);

fn on_cam_insert_viewport(mut world: DeferredWorld, context: HookContext) {
    // Set render target to the viewport image
    if let Some(viewport_ref) = world.get::<CameraActiveForViewport>(context.entity)
        && let Some(viewport_target) = world
            .get::<ViewportRenderTarget>(viewport_ref.0)
            .map(|v| v.0.clone())
        && let Some(mut cam_target) = world.get_mut::<RenderTarget>(context.entity)
    {
        *cam_target = RenderTarget::Image(viewport_target.clone().into());
    }

    if let Some(mut camera) = world.get_mut::<Camera>(context.entity) {
        camera.is_active = true;
    }
}
fn on_cam_discard_viewport(mut world: DeferredWorld, context: HookContext) {
    if let Some(mut camera) = world.get_mut::<Camera>(context.entity) {
        camera.is_active = false;
    }
}

/// Fall back to a viewports default camera if its current [`ViewportShowsCamera`] is invalid.
#[allow(clippy::type_complexity)]
fn ensure_viewport_has_camera(
    mut commands: Commands,
    q_viewports: Query<
        (Entity, Option<&DefaultViewportCamera>),
        (With<ViewportRenderTarget>, Without<ViewportShowsCamera>),
    >,
) {
    for (viewport_entity, default_cam) in q_viewports.iter() {
        if let Some(default_cam) = default_cam {
            commands
                .entity(viewport_entity)
                .insert(ViewportShowsCamera(default_cam.0));
        }
    }
}

/// Updates [`WorldSpaceRobotControl`] on robots that are controlled by a gamepad to be relative to the current camera.
fn update_global_control_dir(
    mut commands: Commands,
    q_viewports: Option<Single<&ViewportShowsCamera>>,
    q_cam: Query<(&GlobalTransform, Has<RobotCam>, Option<&ChildOf>)>,
    q_field: Query<&GlobalTransform, With<Field>>,
    q_robots: Query<(
        Option<&mut WorldSpaceRobotControl>,
        Has<RobotControlledByGamepad>,
        &ChildOf,
        Entity,
    )>,
) {
    let Some(viewport_cam_ref) = q_viewports.as_deref() else {
        return;
    };

    let Ok((cam_transform, is_robot_cam, cam_parent)) = q_cam.get(viewport_cam_ref.0) else {
        return;
    };

    let active_robot = is_robot_cam
        .then(|| cam_parent.map(|parent| parent.0))
        .flatten();
    let cam_forward = cam_transform.forward().as_vec3();

    for (global_control, is_gamepad_controlled, field_ref, robot_entity) in q_robots {
        let should_have_global_control =
            is_gamepad_controlled && active_robot != Some(robot_entity);

        if should_have_global_control {
            let field_transform = *q_field.get(field_ref.0).unwrap();
            let field_cam_dir = field_transform
                .affine()
                .inverse()
                .transform_vector3(cam_forward);
            let look_dir = Vec2::new(field_cam_dir.x, -field_cam_dir.z).normalize_or_zero();

            if let Some(mut global_control) = global_control {
                global_control.field_forward = look_dir;
            } else {
                commands
                    .entity(robot_entity)
                    .insert(WorldSpaceRobotControl {
                        field_forward: look_dir,
                    });
            }
        } else if global_control.is_some() {
            commands
                .entity(robot_entity)
                .remove::<WorldSpaceRobotControl>();
        }
    }
}
