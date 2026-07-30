use crate::interaction::input::InputActions;
use bevy::math::FloatPow;
use bevy::math::bounding::Aabb3d;
use bevy::prelude::*;
use bevy_mod_openxr::helper_traits::ToTransform;
use bevy_mod_openxr::resources::OxrViews;
use schminput::BoolActionValue;
use std::f32::consts::PI;

/// Spawns and manages a floating panel anchor that follows the head position
pub fn floating_anchor_plugin(app: &mut App) {
    app.insert_resource(FloatingAnchorSettings::default());
    app.add_systems(Startup, spawn_floating_anchor)
        .add_systems(Update, (hide_floating_anchor, move_floating_anchor));
}

#[derive(Resource, Clone, Debug)]
pub struct FloatingAnchorSettings {
    height: f32,
    distance: f32,
    pos_deadzone: f32,
    rot_deadzone: f32,
    spring_damping: f32,
    spring_stiffness: f32,
    spring_mass: f32,
}

impl FloatingAnchorSettings {
    /// Configures the spring physics using intuitive parameters from https://www.kvin.me/posts/effortless-ui-spring-animations
    fn with_spring(mut self, perceptual_duration_secs: f32, bounce: f32) -> Self {
        self.spring_mass = 1.0;
        self.spring_stiffness = (2.0 * PI / perceptual_duration_secs).squared();
        if bounce >= 0.0 {
            self.spring_damping = ((1.0 - bounce) * 4.0 * PI) / perceptual_duration_secs;
        } else {
            self.spring_damping = (4.0 * PI) / (perceptual_duration_secs * (1.0 + bounce));
        }
        self
    }
}

impl Default for FloatingAnchorSettings {
    fn default() -> Self {
        Self {
            height: 1.2,
            distance: 1.0,
            pos_deadzone: 0.5,
            rot_deadzone: PI / 6.0,
            spring_damping: 0.0,
            spring_stiffness: 0.0,
            spring_mass: 0.0,
        }
        .with_spring(1.0, 0.15)
    }
}

/// Marker component for the floating panel anchor
#[derive(Component, Clone, Debug, Default)]
#[require(Transform)]
pub struct FloatingPanelAnchor;

#[derive(Component, Clone, Debug, Default)]
#[require(Transform)]
struct FloatingAnchorBase {
    pos_target: Vec2,
    pos_vel: Vec2,
    rot_target: f32,
    rot_vel: f32,
}

fn spawn_floating_anchor(
    mut commands: Commands,
    settings: Res<FloatingAnchorSettings>,
    mut gizmos: ResMut<Assets<GizmoAsset>>,
) {
    let mut marker = GizmoAsset::new();
    marker.aabb_3d(
        Aabb3d::new(Vec3::ZERO, Vec3::new(0.1, 0.02, 0.02)),
        Transform::default(),
        Color::WHITE,
    );

    // The offset anchor is moved indirectly using a base entity at the floor below the player.
    commands.spawn(FloatingAnchorBase::default()).with_child((
        FloatingPanelAnchor,
        Visibility::Hidden,
        Transform::from_xyz(0.0, settings.height, -settings.distance),
        Gizmo {
            handle: gizmos.add(marker),
            ..default()
        },
    ));
}

fn hide_floating_anchor(
    actions: Res<InputActions>,
    action_values: Query<&BoolActionValue>,
    mut prev_pressed: Local<bool>,
    mut query: Query<&mut Visibility, With<FloatingPanelAnchor>>,
) {
    let pressed = action_values.get(actions.menu_press).unwrap().any;

    if pressed && !*prev_pressed {
        *prev_pressed = true;
        for mut visibility in query.iter_mut() {
            *visibility = match *visibility {
                Visibility::Inherited | Visibility::Visible => Visibility::Hidden,
                Visibility::Hidden => Visibility::Inherited,
            };
        }
    } else if !pressed && *prev_pressed {
        *prev_pressed = false;
    }
}

fn move_floating_anchor(
    settings: Res<FloatingAnchorSettings>,
    anchor: Single<(&mut FloatingAnchorBase, &mut Transform)>,
    views: Res<OxrViews>,
    time: Res<Time>,
) {
    if views.is_empty() {
        return;
    }

    let head_transform = views[0].pose.to_transform();
    let (mut base, mut base_transform) = anchor.into_inner();

    let head_pos = head_transform.translation.xz();
    let base_pos = base_transform.translation.xz();

    // Compute yaw
    let head_dir = head_transform.forward();
    let head_yaw = f32::atan2(-head_dir.x, -head_dir.z);
    let base_dir = base_transform.forward();
    let base_yaw = f32::atan2(-base_dir.x, -base_dir.z);

    let target_dir_2d = Vec2::from_angle(base.rot_target);
    let head_dir_2d = Vec2::from_angle(head_yaw);
    let base_dir_2d = Vec2::from_angle(base_yaw);

    let target_rot_diff = target_dir_2d.angle_to(head_dir_2d);
    let base_rot_diff = base_dir_2d.angle_to(target_dir_2d);

    // Check deadzones, but also allow target changes while already moving
    let move_exceeded = base.pos_vel.length_squared() > 0.2
        || base.pos_target.distance(head_pos) > settings.pos_deadzone;
    let rotate_exceeded = base.rot_vel.abs() > 0.5 || target_rot_diff.abs() > settings.rot_deadzone;

    if move_exceeded || rotate_exceeded {
        // Will only take effect in the next frame
        base.pos_target = head_pos;
        base.rot_target = head_yaw;
    }

    // Spring physics calculation
    let pos_diff = base.pos_target - base_pos;

    let damping = settings.spring_damping;
    let stiffness = settings.spring_stiffness;
    let mass = settings.spring_mass;
    let spring_accel = (pos_diff * stiffness - base.pos_vel * damping) / mass;
    let rot_accel = (base_rot_diff * stiffness - base.rot_vel * damping) / mass;

    let dt = time.delta_secs();
    base.pos_vel += spring_accel * dt;
    base.rot_vel += rot_accel * dt;
    base_transform.translation += Vec3::new(base.pos_vel.x, 0.0, base.pos_vel.y) * dt;
    base_transform.rotate_y(base.rot_vel * dt);
}
