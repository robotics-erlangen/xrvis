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

#[derive(Component, Clone, Debug)]
pub struct PanOrbitCamera {
    pub input_node: Entity,
    orbit_target: Vec3,
    orbiting: bool,
    panning: bool,
}

impl PanOrbitCamera {
    pub fn new(input_node: Entity, orbit_target: Vec3) -> Self {
        Self {
            input_node,
            orbit_target,
            orbiting: false,
            panning: false,
        }
    }
}

fn initial_camera_look(
    new_cam: On<Add<PanOrbitCamera>>,
    mut q_cam: Query<(&mut Transform, &PanOrbitCamera)>,
) {
    if let Ok((mut cam_transform, cam_state)) = q_cam.get_mut(new_cam.entity) {
        cam_transform.look_at(cam_state.orbit_target, Vec3::Y);
    }
}

fn orbit_camera(
    q_cameras: Query<(&mut Transform, &mut PanOrbitCamera)>,
    q_hovered: Query<&Hovered>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
) {
    for (mut cam_transform, mut cam_state) in q_cameras {
        let Ok(viewport_hovered) = q_hovered.get(cam_state.input_node) else {
            return;
        };

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

        let orbit_distance = cam_state.orbit_target.distance(cam_transform.translation);
        cam_transform.translation =
            cam_state.orbit_target - cam_transform.forward() * orbit_distance;
    }
}

fn pan_camera(
    q_cameras: Query<(&mut Transform, &mut PanOrbitCamera, &Projection)>,
    q_input_nodes: Query<(&Hovered, &ComputedNode)>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
) {
    for (mut cam_transform, mut cam_state, projection) in q_cameras {
        let Ok((viewport_hovered, viewport_node)) = q_input_nodes.get(cam_state.input_node) else {
            return;
        };

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

        let orbit_distance = cam_state.orbit_target.distance(cam_transform.translation);

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

        cam_state.orbit_target -= pan_offset;
        cam_transform.translation =
            cam_state.orbit_target - cam_transform.forward() * orbit_distance;
    }
}

fn zoom_camera(
    q_cameras: Query<(&mut Transform, &PanOrbitCamera)>,
    q_hovered: Query<&Hovered>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
) {
    for (mut cam_transform, cam_state) in q_cameras {
        if !q_hovered.get(cam_state.input_node).is_ok_and(|h| h.get()) {
            return;
        }

        let delta_px_y = mouse_scroll.delta.y;
        if delta_px_y == 0.0 {
            return;
        }

        let old_orbit_distance = cam_state.orbit_target.distance(cam_transform.translation);
        let zoom_delta = -delta_px_y * ZOOM_SPEED * old_orbit_distance;
        let new_orbit_distance =
            (old_orbit_distance + zoom_delta).clamp(ZOOM_RANGE.start, ZOOM_RANGE.end);

        cam_transform.translation =
            cam_state.orbit_target - cam_transform.forward() * new_orbit_distance;
    }
}

// TODO: Trackpad, touch and keyboard input
