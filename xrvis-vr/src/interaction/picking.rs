use crate::interaction::input::{LeftHandPointer, PointerActions, RightHandPointer};
use bevy::app::{App, PreUpdate, Update};
use bevy::asset::uuid::Uuid;
use bevy::math::Ray3d;
use bevy::picking::pointer::PointerId;
use bevy::prelude::*;
use schminput::F32ActionValue;
use std::ops::Range;

const LEFT_HAND_POINTER: PointerId = PointerId::Custom(Uuid::from_u128(10101010));
const RIGHT_HAND_POINTER: PointerId = PointerId::Custom(Uuid::from_u128(20202020));

pub fn xr_picking_plugin(app: &mut App) {
    app.add_systems(PreUpdate, update_hand_pointer_rays);
    app.add_systems(Update, draw_pointer_ray);

    app.register_required_components_with::<LeftHandPointer, _>(|| XrPointer {
        id: LEFT_HAND_POINTER,
        active_range: 0.0..100.0,
        ray: Ray3d::new(Vec3::ZERO, Dir3::NEG_Z),
    });
    app.register_required_components_with::<RightHandPointer, _>(|| XrPointer {
        id: RIGHT_HAND_POINTER,
        active_range: 0.0..100.0,
        ray: Ray3d::new(Vec3::ZERO, Dir3::NEG_Z),
    });
}

#[derive(Component)]
pub struct XrPointer {
    id: PointerId,
    active_range: Range<f32>,
    ray: Ray3d,
}

#[allow(clippy::type_complexity)]
pub fn update_hand_pointer_rays(
    mut left_hand: Option<
        Single<(&mut XrPointer, &Transform), (With<LeftHandPointer>, Without<RightHandPointer>)>,
    >,
    mut right_hand: Option<
        Single<(&mut XrPointer, &Transform), (With<RightHandPointer>, Without<LeftHandPointer>)>,
    >,
) {
    if let Some((pointer, transform)) = left_hand.as_deref_mut() {
        pointer.ray = Ray3d {
            origin: transform.translation,
            direction: transform.forward(),
        };
    }
    if let Some((pointer, transform)) = right_hand.as_deref_mut() {
        pointer.ray = Ray3d {
            origin: transform.translation,
            direction: transform.forward(),
        };
    }
}

pub fn draw_pointer_ray(
    mut gizmos: Gizmos,
    pointer_actions: Res<PointerActions>,
    action_values: Query<&F32ActionValue>,
    pointers: Query<&XrPointer>,
) {
    for pointer in pointers.iter() {
        let value = if pointer.id == LEFT_HAND_POINTER {
            action_values
                .get(pointer_actions.left_aim_activate)
                .unwrap()
                .any
        } else {
            action_values
                .get(pointer_actions.right_aim_activate)
                .unwrap()
                .any
        };

        gizmos.line(
            pointer.ray.origin,
            pointer.ray.get_point(1.),
            Color::srgb(1. - value, 1. - value, 1.),
        );
    }
}
