use std::f32::consts::PI;

use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::prelude::*;
use sslgame::fields::robots::Robot;

pub fn robot_cam_plugin(app: &mut App) {
    app.add_observer(add_robot_cams);
}

fn add_robot_cams(event: On<Add<Robot>>, mut commands: Commands) {
    commands.entity(event.entity).with_child((
        RobotCam,
        Transform::from_translation(Vec3::new(0.0, 0.12, -0.08))
            .with_rotation(Quat::from_rotation_x(-PI / 6.0)),
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            near: 0.001,
            fov: PI / 2.0,
            ..Default::default()
        }),
        Camera {
            order: -1,
            is_active: false, // Will be activated by ViewportShowsCamera
            ..default()
        },
        DepthPrepass,
    ));
}

#[derive(Component)]
pub struct RobotCam;
