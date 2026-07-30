use bevy::prelude::*;
use bevy_mod_openxr::openxr_session_running;
use bevy_mod_xr::hands::{HandBone, RightHand, XrHandBoneEntities, XrHandBoneRadius};
use sslgame::field::Field;

// TODO: Replace this with UI panels and system-level input actions

pub fn old_interaction_plugin(app: &mut App) {
    app.add_systems(
        Update,
        right_hand_interaction.run_if(openxr_session_running),
    );
}

#[derive(Component)]
pub struct RightHandInteractionState {
    start_finger_pos: Vec3,
}

#[allow(clippy::type_complexity)]
fn right_hand_interaction(
    mut gizmos: Gizmos,
    mut commands: Commands,
    mut field: Option<Single<&mut Transform, With<Field>>>,
    mut right_hand: Option<
        Single<(
            &RightHand,
            &XrHandBoneEntities,
            Option<&mut RightHandInteractionState>,
            Entity,
        )>,
    >,
    q_bones: Query<(&XrHandBoneRadius, &Transform), Without<Field>>,
) {
    let Some(field_transform) = field.as_deref_mut() else {
        return;
    };

    let Some((_, bones, state, hand)) = right_hand.as_deref_mut() else {
        return;
    };

    let Ok((index_radius, index_transform)) = q_bones.get(bones.0[HandBone::IndexTip as usize])
    else {
        return;
    };

    let Ok((thumb_radius, thumb_transform)) = q_bones.get(bones.0[HandBone::ThumbTip as usize])
    else {
        return;
    };

    let finger_pos = thumb_transform.translation;

    if let Some(state) = state {
        gizmos.line(state.start_finger_pos, finger_pos, Color::WHITE);
        if thumb_transform
            .translation
            .distance(index_transform.translation)
            > (thumb_radius.0 + index_radius.0) * 1.5
        {
            // Interaction finished -> Check results
            if state.start_finger_pos.distance(finger_pos) > 1. {
                // Only accept interaction with >1m of distance
                field_transform.translation = state.start_finger_pos;
                let mut dir = finger_pos - state.start_finger_pos;
                dir.y = 0.0;
                if dir.length_squared() > 1e-6 {
                    let dir = dir.normalize();
                    // Compute yaw so the field faces along dir while staying parallel to ground.
                    // We build a rotation that aligns the field's local -Z axis to `dir`.
                    // Angle around Y axis between -Z and `dir`:
                    let yaw = f32::atan2(dir.x, dir.z);
                    // Rotate around Y by -yaw so that -Z ends up pointing along `dir`.
                    field_transform.rotation = Quat::from_rotation_y(yaw);
                }
            }
            commands.entity(*hand).remove::<RightHandInteractionState>();
        }
    } else if thumb_transform
        .translation
        .distance(index_transform.translation)
        < thumb_radius.0 + index_radius.0
        && thumb_transform.translation.y < 0.5
    {
        // Start interaction
        commands.entity(*hand).insert(RightHandInteractionState {
            start_finger_pos: finger_pos,
        });
    }
}
