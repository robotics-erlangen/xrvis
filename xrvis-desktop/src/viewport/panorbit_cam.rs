use bevy::camera::Projection;
use bevy::input::ButtonInput;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::math::{EulerRot, Quat, Vec2, Vec3};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;
use std::ops::Range;

// Partially based on https://github.com/doceazedo/sprinkles/blob/v0.3.0/crates/bevy_sprinkles_editor/src/viewport.rs under MIT/Apache

pub fn panorbit_camera_plugin(app: &mut App) {
    app.add_systems(Update, (orbit_camera, zoom_camera, pan_camera))
        .add_observer(initial_camera_look);
}

const ZOOM_SPEED: f32 = 0.1;
const ZOOM_RANGE: Range<f32> = 1.0..40.0;
const PITCH_SPEED: f32 = 0.003;
const PITCH_RANGE: Range<f32> = -(FRAC_PI_2 - 0.01)..(FRAC_PI_2 - 0.01);
const YAW_SPEED: f32 = 0.004;

#[derive(Component, Debug, Default)]
#[require(ViewportCameraOrbitTarget)]
pub struct ViewportCamera {
    pub orbiting: bool,
    pub panning: bool,
}

#[derive(Component, Debug, Default)]
pub struct ViewportCameraOrbitTarget(pub Vec3);

fn initial_camera_look(
    new_cam: On<Add, ViewportCamera>,
    mut q_cam: Query<(&mut Transform, &ViewportCameraOrbitTarget)>,
) {
    if let Ok((mut cam_transform, cam_target)) = q_cam.get_mut(new_cam.entity) {
        cam_transform.look_at(cam_target.0, Vec3::Y);
    }
}

fn orbit_camera(
    camera: Single<(
        &mut Transform,
        &ViewportCameraOrbitTarget,
        &mut ViewportCamera,
    )>,
    viewport: Single<&Hovered, With<ViewportNode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
) {
    let (mut cam_transform, cam_target, mut cam_state) = camera.into_inner();
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

    let orbit_distance = cam_target.0.distance(cam_transform.translation);
    cam_transform.translation = cam_target.0 - cam_transform.forward() * orbit_distance;
}

fn pan_camera(
    camera: Single<(
        &mut Transform,
        &mut ViewportCameraOrbitTarget,
        &mut ViewportCamera,
        &Projection,
    )>,
    viewport: Single<(&Hovered, &ComputedNode), With<ViewportNode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
) {
    let (mut cam_transform, mut cam_target, mut cam_state, projection) = camera.into_inner();
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

    let orbit_distance = cam_target.0.distance(cam_transform.translation);

    let viewport_height_px = viewport_node.size().y * viewport_node.inverse_scale_factor();
    let world_height = match projection {
        Projection::Perspective(perspective) => {
            2.0 * orbit_distance * (perspective.fov * 0.5).tan()
        }
        Projection::Orthographic(orthographic) => orthographic.scale,
        _ => unimplemented!(),
    };

    let world_units_per_pixel = world_height / viewport_height_px;

    let pan_offset = cam_transform.right() * delta_px.x * world_units_per_pixel
        + cam_transform.down() * delta_px.y * world_units_per_pixel;

    cam_target.0 -= pan_offset;
    cam_transform.translation = cam_target.0 - cam_transform.forward() * orbit_distance;
}

fn zoom_camera(
    camera: Single<(&mut Transform, &ViewportCameraOrbitTarget), With<ViewportCamera>>,
    viewport: Single<&Hovered, With<ViewportNode>>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
) {
    let (mut cam_transform, cam_target) = camera.into_inner();
    if !viewport.get() {
        return;
    }

    let delta_px_y = mouse_scroll.delta.y;
    if delta_px_y == 0.0 {
        return;
    }

    let old_orbit_distance = cam_target.0.distance(cam_transform.translation);
    let zoom_delta = -delta_px_y * ZOOM_SPEED * old_orbit_distance;
    let new_orbit_distance =
        (old_orbit_distance + zoom_delta).clamp(ZOOM_RANGE.start, ZOOM_RANGE.end);

    cam_transform.translation = cam_target.0 - cam_transform.forward() * new_orbit_distance;
}

// TODO: Trackpad, touch and keyboard input
