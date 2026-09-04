use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::ecs::template::TemplateContext;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::render::render_resource::{TextureDimension, TextureFormat, TextureUsages};
use std::f32::consts::FRAC_PI_2;
use std::ops::Range;

// Partially based on https://github.com/doceazedo/sprinkles/blob/v0.3.0/crates/bevy_sprinkles_editor/src/viewport.rs under MIT/Apache

const INITIAL_ORBIT_TARGET: Vec3 = Vec3::ZERO;
const INITIAL_CAM_POS: Vec3 = Vec3::new(0.0, 9.0, 10.0);
const ZOOM_SPEED: f32 = 0.1;
const ZOOM_RANGE: Range<f32> = 1.0..40.0;
const PITCH_SPEED: f32 = 0.003;
const PITCH_RANGE: Range<f32> = -(FRAC_PI_2 - 0.01)..(FRAC_PI_2 - 0.01);
const YAW_SPEED: f32 = 0.004;

pub fn viewport_plugin(app: &mut App) {
    app.add_systems(Update, (orbit_camera, zoom_camera, pan_camera));
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
                    Transform::from_translation(INITIAL_CAM_POS)
                        .looking_at(INITIAL_ORBIT_TARGET, Vec3::Y),
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

#[derive(Component)]
struct ViewportCamera {
    pub orbit_target: Vec3,
    pub orbit_distance: f32,
    pub orbiting: bool,
    pub panning: bool,
}

impl Default for ViewportCamera {
    fn default() -> Self {
        Self {
            orbit_target: INITIAL_ORBIT_TARGET,
            orbit_distance: INITIAL_ORBIT_TARGET.distance(INITIAL_CAM_POS),
            orbiting: false,
            panning: false,
        }
    }
}

fn orbit_camera(
    camera: Single<(&mut Transform, &mut ViewportCamera)>,
    viewport: Single<&Hovered, With<ViewportNode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
) {
    let (mut cam_transform, mut cam_state) = camera.into_inner();
    let viewport_hovered = viewport.into_inner();

    if !mouse_buttons.pressed(MouseButton::Left) {
        cam_state.orbiting = false;
        return;
    }
    if mouse_buttons.just_pressed(MouseButton::Left) && viewport_hovered.get() {
        cam_state.orbiting = true;
    }
    if !cam_state.orbiting {
        return;
    }

    let delta_px = -mouse_motion.delta;
    let delta_pitch = delta_px.y * PITCH_SPEED;
    let delta_yaw = delta_px.x * YAW_SPEED;

    let (yaw, pitch, roll) = cam_transform.rotation.to_euler(EulerRot::YXZ);

    let pitch = (pitch + delta_pitch).clamp(PITCH_RANGE.start, PITCH_RANGE.end);
    let yaw = yaw + delta_yaw;
    cam_transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll);

    cam_transform.translation =
        cam_state.orbit_target - cam_transform.forward() * cam_state.orbit_distance;
}

fn pan_camera(
    camera: Single<(&mut Transform, &mut ViewportCamera, &Projection)>,
    viewport: Single<(&Hovered, &ComputedNode), With<ViewportNode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
) {
    let (mut cam_transform, mut cam_state, projection) = camera.into_inner();
    let (viewport_hovered, viewport_node) = viewport.into_inner();

    if !mouse_buttons.pressed(MouseButton::Right) {
        cam_state.panning = false;
        return;
    }
    if mouse_buttons.just_pressed(MouseButton::Right) && viewport_hovered.get() {
        cam_state.panning = true;
    }
    if !cam_state.panning {
        return;
    }

    let delta_px = mouse_motion.delta;
    if delta_px == Vec2::ZERO {
        return;
    }

    let viewport_height_px = viewport_node.size().y * viewport_node.inverse_scale_factor();
    let world_height = match projection {
        Projection::Perspective(perspective) => {
            2.0 * cam_state.orbit_distance * (perspective.fov * 0.5).tan()
        }
        Projection::Orthographic(orthographic) => orthographic.scale,
        _ => unimplemented!(),
    };

    let world_units_per_pixel = world_height / viewport_height_px;

    let pan_offset = cam_transform.right() * delta_px.x * world_units_per_pixel
        + cam_transform.down() * delta_px.y * world_units_per_pixel;

    cam_state.orbit_target -= pan_offset;
    cam_transform.translation =
        cam_state.orbit_target - cam_transform.forward() * cam_state.orbit_distance;
}

fn zoom_camera(
    camera: Single<(&mut Transform, &mut ViewportCamera)>,
    viewport: Single<&Hovered, With<ViewportNode>>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
) {
    let (mut cam_transform, mut cam_state) = camera.into_inner();
    if !viewport.get() {
        return;
    }

    let delta_px_y = mouse_scroll.delta.y;
    if delta_px_y == 0.0 {
        return;
    }

    let zoom_delta = -delta_px_y * ZOOM_SPEED * cam_state.orbit_distance;
    cam_state.orbit_distance =
        (cam_state.orbit_distance + zoom_delta).clamp(ZOOM_RANGE.start, ZOOM_RANGE.end);

    cam_transform.translation =
        cam_state.orbit_target - cam_transform.forward() * cam_state.orbit_distance;
}
