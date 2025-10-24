use bevy::prelude::*;
use bevy_mod_openxr::openxr_session_running;
use bevy_mod_xr::hands::{HandBone, LeftHand, RightHand, XrHandBoneEntities, XrHandBoneRadius};
use sslgame::{Field, RenderSettings, RobotRenderSettings};

pub fn interaction_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            insert_left_hand_interaction_state,
            left_hand_interaction,
            right_hand_interaction,
        )
            .run_if(openxr_session_running),
    );
}

#[derive(Component)]
pub struct LeftHandInteractionState {
    triggered: bool,
    render_settings_cycle: Vec<RenderSettings>,
    next_index: usize,
}

pub fn insert_left_hand_interaction_state(
    mut commands: Commands,
    q_left_hands: Query<Entity, (With<LeftHand>, Without<LeftHandInteractionState>)>,
) {
    for hand in q_left_hands {
        commands.entity(hand).insert(LeftHandInteractionState {
            triggered: false,
            render_settings_cycle: vec![
                RenderSettings {
                    field: true,
                    robots: RobotRenderSettings::Fallback,
                    ball: true,
                    visualizations: true,
                },
                RenderSettings {
                    field: true,
                    robots: RobotRenderSettings::Fallback,
                    ball: true,
                    visualizations: false,
                },
                RenderSettings {
                    field: false,
                    robots: RobotRenderSettings::Cutout,
                    ball: false,
                    visualizations: true,
                },
            ],
            next_index: 0,
        });
    }
}

fn left_hand_interaction(
    mut render_settings: ResMut<RenderSettings>,
    mut left_hand: Option<
        Single<(
            &LeftHand,
            &XrHandBoneEntities,
            &mut LeftHandInteractionState,
        )>,
    >,
    q_bones: Query<(&XrHandBoneRadius, &Transform)>,
) {
    let Some((_, bones, state)) = left_hand.as_deref_mut() else {
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

    if !state.triggered
        && thumb_transform
            .translation
            .distance(index_transform.translation)
            < thumb_radius.0 + index_radius.0
    {
        *render_settings = state.render_settings_cycle[state.next_index].clone();
        state.next_index = (state.next_index + 1) % state.render_settings_cycle.len();
        state.triggered = true;
    } else if state.triggered
        && thumb_transform
            .translation
            .distance(index_transform.translation)
            > (thumb_radius.0 + index_radius.0) * 1.5
    {
        state.triggered = false;
    }
}

#[derive(Component)]
pub struct RightHandInteractionState {
    start_finger_pos: Vec3,
}

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
