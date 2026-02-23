use crate::interaction::input::{LeftHandPointer, PointerActions, RightHandPointer};
use crate::interaction::panels::XrPanel;
use bevy::app::App;
use bevy::asset::uuid::Uuid;
use bevy::camera::{NormalizedRenderTarget, RenderTarget};
use bevy::math::Ray3d;
use bevy::picking::PickingSystems;
use bevy::picking::pointer::{
    Location, PointerAction, PointerId, PointerInput, PointerLocation, PointerPress,
};
use bevy::prelude::*;
use schminput::BoolActionValue;
use std::ops::Range;

const LEFT_HAND_POINTER_ID: PointerId = PointerId::Custom(Uuid::from_u128(10101010));
const RIGHT_HAND_POINTER_ID: PointerId = PointerId::Custom(Uuid::from_u128(20202020));

pub fn xr_picking_plugin(app: &mut App) {
    // Running this in First with the other picking sources causes a one-frame delay because openxr space transforms are updated in PreUpdate
    app.add_systems(
        First,
        (update_hand_pointer_rays, drive_xr_pointers)
            .chain()
            .in_set(PickingSystems::Input),
    );

    app.register_required_components_with::<LeftHandPointer, _>(|| LEFT_HAND_POINTER_ID);
    app.register_required_components_with::<LeftHandPointer, _>(|| XrPointer {
        ray: Ray3d::new(Vec3::ZERO, Dir3::NEG_Z),
        range: 0.0..10.0,
        trigger_pressed: false,
    });
    app.register_required_components_with::<RightHandPointer, _>(|| RIGHT_HAND_POINTER_ID);
    app.register_required_components_with::<RightHandPointer, _>(|| XrPointer {
        ray: Ray3d::new(Vec3::ZERO, Dir3::NEG_Z),
        range: 0.0..10.0,
        trigger_pressed: false,
    });
}

#[derive(Component)]
pub struct XrPointer {
    ray: Ray3d,
    range: Range<f32>,
    trigger_pressed: bool,
}

pub fn update_hand_pointer_rays(
    mut gizmos: Gizmos,
    pointer_actions: Res<PointerActions>,
    action_values: Query<&BoolActionValue>,
    pointers: Query<(&mut XrPointer, &PointerId, &GlobalTransform)>,
) {
    for (mut xr_pointer, pointer_id, transform) in pointers {
        let trigger_entity = if *pointer_id == LEFT_HAND_POINTER_ID {
            pointer_actions.left_aim_activate
        } else {
            pointer_actions.right_aim_activate
        };
        let trigger_bool = action_values.get(trigger_entity).unwrap().any;

        xr_pointer.trigger_pressed = trigger_bool;
        xr_pointer.ray = Ray3d {
            origin: transform.translation(),
            direction: transform.forward(),
        };

        let trigger_float = trigger_bool as u8 as f32;
        gizmos.line(
            xr_pointer.ray.origin,
            xr_pointer.ray.get_point(1.),
            Color::srgb(1. - trigger_float, 1. - trigger_float, 1.),
        );
    }
}

/// Forwards pointer events from openxr to virtual pointers on the UI panels
#[allow(clippy::too_many_arguments)]
pub fn drive_xr_pointers(
    mut gizmos: Gizmos,
    // Pointers
    pointers: Query<(&XrPointer, &PointerId, &PointerLocation, &PointerPress)>,
    // Panels
    panels: Query<(&GlobalTransform, &UiTargetCamera), With<XrPanel>>,
    render_targets: Query<&RenderTarget>,
    image_assets: Res<Assets<Image>>,
    // Events
    mut pointer_inputs: MessageWriter<PointerInput>,
) -> Result {
    struct XrPointerHit {
        location: Location,
        depth: f32,
    }

    for (xr_pointer, pointer_id, prev_pointer_loc, pointer_press) in pointers {
        let mut hits: Vec<XrPointerHit> = Vec::new();

        // Collect pointer hits
        for (transform, ui_cam) in panels {
            // Get render target info
            let (render_target, texture_size) =
                if let Ok(RenderTarget::Image(img_target)) = render_targets.get(ui_cam.entity()) {
                    let image = image_assets.get(&img_target.handle).unwrap();
                    (
                        NormalizedRenderTarget::Image(img_target.clone()),
                        image.size(),
                    )
                } else {
                    warn!(
                        "Failed to get image render target for xr panel camera {}",
                        ui_cam.entity()
                    );
                    continue;
                };

            // Calculate intersection with infinite plane
            let Some(intersect_dist) = xr_pointer.ray.intersect_plane(
                transform.translation(),
                InfinitePlane3d::new(transform.forward()),
            ) else {
                continue;
            };

            // Discard hits outside the active pointer range
            if !xr_pointer.range.contains(&intersect_dist) {
                continue;
            }

            // Calculate 2d hit position on the surface
            let global_hit = xr_pointer.ray.get_point(intersect_dist);
            let local_hit = global_hit - transform.translation();
            let surface_x = local_hit.dot(*transform.right());
            let surface_y = local_hit.dot(*transform.up());
            let surface_hit = Vec2::new(surface_x, surface_y);
            let normalized_surface_hit = surface_hit / transform.scale().xy();
            // Get pixel position on the render target texture (top-left origin, y-down)
            let pointer_pos = (normalized_surface_hit + 0.5) * texture_size.as_vec2();

            // Don't switch panel focus while dragging, even if the pointer leaves the panel bounds
            let prev_render_target = prev_pointer_loc.location.as_ref().map(|l| &l.target);
            if prev_render_target == Some(&render_target) && pointer_press.is_primary_pressed() {
                hits = vec![XrPointerHit {
                    location: Location {
                        target: render_target,
                        position: pointer_pos,
                    },
                    depth: intersect_dist,
                }];
                break;
            }

            // Discard hits outside the panel bounds
            if surface_hit.abs().cmpgt(transform.scale().xy() / 2.).any() {
                continue;
            }

            // Push hit to the list to process later
            hits.push(XrPointerHit {
                location: Location {
                    target: render_target,
                    position: pointer_pos,
                },
                depth: intersect_dist,
            });
        }

        // ==== Handle collected hits ====

        // Sort hits by distance
        hits.sort_unstable_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap());

        // Get the closest hit
        let Some(closest_hit) = hits.into_iter().next() else {
            // Cancel the pointer interaction if there are no hits
            // prev_loc is kept, otherwise the event would be discarded as out-of-bounds before the cancel is processed
            if let Some(prev_loc) = &prev_pointer_loc.location {
                pointer_inputs.write(PointerInput::new(
                    *pointer_id,
                    prev_loc.clone(),
                    PointerAction::Cancel,
                ));
            }
            continue;
        };

        // Sending a cancel event when moving to a different panel is not necessary because
        // dragging locks the cursor to one panel and hover state works across panels

        // Draw hit marker
        gizmos.sphere(
            xr_pointer.ray.get_point(closest_hit.depth),
            0.01,
            Color::WHITE,
        );

        // Get pointer locations
        let pointer_loc = closest_hit.location;
        let prev_pointer_loc = prev_pointer_loc.location.as_ref();

        // Send click events
        if xr_pointer.trigger_pressed && !pointer_press.is_primary_pressed() {
            pointer_inputs.write(PointerInput::new(
                *pointer_id,
                pointer_loc.clone(),
                PointerAction::Press(PointerButton::Primary),
            ));
        } else if !xr_pointer.trigger_pressed && pointer_press.is_primary_pressed() {
            pointer_inputs.write(PointerInput::new(
                *pointer_id,
                pointer_loc.clone(),
                PointerAction::Release(PointerButton::Primary),
            ));
        }

        // Send movement event if the position changed
        if prev_pointer_loc != Some(&pointer_loc) {
            pointer_inputs.write(PointerInput {
                pointer_id: *pointer_id,
                action: PointerAction::Move {
                    delta: if let Some(prev_loc) = prev_pointer_loc
                        && prev_loc.target == pointer_loc.target
                    {
                        (pointer_loc.position) - prev_loc.position
                    } else {
                        Vec2::ZERO
                    },
                },
                location: pointer_loc,
            });
        }
    }

    Ok(())
}
