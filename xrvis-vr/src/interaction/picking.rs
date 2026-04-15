use crate::interaction::input::{LeftHandPointer, PointerActions, RightHandPointer};
use crate::panels::{XrPanel, XrUiRoot};
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
use sslgame::proto::remote::{RobotMoveCommand, ws_request};
use sslgame::{Field, FieldGeometry, Robot, Team};
use std::ops::Range;
use std::time::Instant;

const LEFT_HAND_POINTER_ID: PointerId = PointerId::Custom(Uuid::from_u128(10101010));
const RIGHT_HAND_POINTER_ID: PointerId = PointerId::Custom(Uuid::from_u128(20202020));

pub fn xr_picking_plugin(app: &mut App) {
    // Running this in First with the other picking sources causes a one-frame delay because openxr space transforms are updated in PreUpdate
    app.add_systems(
        First,
        (update_hand_pointer_rays, drive_ui_pointers)
            .chain()
            .in_set(PickingSystems::Input),
    );
    app.add_systems(Update, drive_field_dragging);

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

pub struct XrSurfaceHit {
    pos: Vec2,
    depth: f32,
    in_bounds: bool,
    in_range: bool,
}

impl XrPointer {
    pub fn intersect_plane(
        &self,
        origin: Vec3,
        normal: Dir3,
        axis_x: Dir3,
        axis_y: Dir3,
        bounds: Vec2,
    ) -> Option<XrSurfaceHit> {
        // Calculate ray-panel intersection point on an infinite plane
        let intersect_dist = self
            .ray
            .intersect_plane(origin, InfinitePlane3d::new(normal))?;

        // Project the 3d intersection onto the panel plane
        let global_hit = self.ray.get_point(intersect_dist);
        let local_hit = global_hit - origin;
        let surface_x = local_hit.dot(*axis_x);
        let surface_y = local_hit.dot(*axis_y);
        let surface_hit = Vec2::new(surface_x, surface_y);

        let in_bounds = surface_x.abs() < bounds.x / 2. && surface_y.abs() < bounds.y / 2.;
        let in_range = self.range.contains(&intersect_dist);

        Some(XrSurfaceHit {
            pos: surface_hit,
            depth: intersect_dist,
            in_bounds,
            in_range,
        })
    }
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
pub fn drive_ui_pointers(
    mut gizmos: Gizmos,
    // Pointers
    pointers: Query<(&XrPointer, &PointerId, &PointerLocation, &PointerPress)>,
    // Panels
    panels: Query<&GlobalTransform, With<XrPanel>>,
    ui_roots: Query<(&UiTargetCamera, &XrUiRoot)>,
    render_targets: Query<&RenderTarget>,
    image_assets: Res<Assets<Image>>,
    // Events
    mut pointer_inputs: MessageWriter<PointerInput>,
) -> Result {
    struct PanelHit {
        location: Location,
        depth: f32,
    }

    for (xr_pointer, pointer_id, prev_pointer_loc, pointer_press) in pointers {
        let mut hits: Vec<PanelHit> = Vec::new();

        // Collect pointer hits by looking up the 3d panel for each UI root.
        // Not doing it the other way around because their relationship only
        // enforces that every ui root has a panel, not the other direction.
        for (ui_cam, &XrUiRoot(panel)) in ui_roots {
            // Query the render target of the ui camera
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

            // Get the transform of the display mesh (the physical panel)
            let panel_transform = panels.get(panel).unwrap();

            // Invert x because the panel is viewed from -z (-z "forward" normal),
            // which would make it x-left with bevy's right-handed coordinate system.
            // Invert y because the ui is y-down.
            let Some(surface_hit) = xr_pointer.intersect_plane(
                panel_transform.translation(),
                panel_transform.forward(),
                -panel_transform.right(),
                -panel_transform.up(),
                panel_transform.scale().xy(),
            ) else {
                continue;
            };
            let normalized_surface_hit = surface_hit.pos / panel_transform.scale().xy();
            // Get pixel position on the render target texture (top-left origin, y-down)
            let pointer_pos = (normalized_surface_hit + 0.5) * texture_size.as_vec2();

            // Don't switch panel focus while dragging, even if the pointer leaves the panel bounds
            let prev_render_target = prev_pointer_loc.location.as_ref().map(|l| &l.target);
            if prev_render_target == Some(&render_target) && pointer_press.is_primary_pressed() {
                hits = vec![PanelHit {
                    location: Location {
                        target: render_target,
                        position: pointer_pos,
                    },
                    depth: surface_hit.depth,
                }];
                break;
            }

            // Discard invalid hits
            if !(surface_hit.in_bounds && surface_hit.in_range) {
                continue;
            }

            // Hit accepted -> collect for processing
            hits.push(PanelHit {
                location: Location {
                    target: render_target,
                    position: pointer_pos,
                },
                depth: surface_hit.depth,
            });
        }

        // ==== Handle collected hits ====

        // Sort hits by distance
        hits.sort_unstable_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap());

        // Get the closest hit
        let Some(closest_hit) = hits.into_iter().next() else {
            // Cancel the pointer interaction if there are no hits
            // The location is still set as prev_loc, otherwise the event would be discarded as out-of-bounds before the cancel is processed.
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

// ========= Robot dragging / Field picking ========

/// Pointer, robot id, robot team, last move command send
#[derive(Component, Debug)]
pub struct FieldDragAction(PointerId, u8, Team, Instant);

fn field_intersection(
    pointer: &XrPointer,
    field_transform: &GlobalTransform,
    bounds: Vec2,
) -> Option<XrSurfaceHit> {
    let hit = pointer.intersect_plane(
        field_transform.translation(),
        field_transform.up(),
        field_transform.right(),
        field_transform.forward(),
        bounds,
    )?;

    if hit.in_bounds && hit.in_range {
        Some(hit)
    } else {
        None
    }
}

fn find_hit_robot(
    robots: &Query<(&Robot, &Team, &Transform, &ChildOf)>,
    field_entity: Entity,
    hit_pos: Vec2,
) -> Option<(u8, Team)> {
    robots
        .iter()
        .find(|(_, _, robot_transform, ChildOf(robot_parent))| {
            if *robot_parent != field_entity {
                return false;
            }
            (robot_transform.translation.xz() * Vec2::new(1., -1.)).distance_squared(hit_pos)
                < 0.1 * 0.1
        })
        .map(|(robot, team, _, _)| (robot.0, *team))
}

pub fn drive_field_dragging(
    mut gizmos: Gizmos,
    mut commands: Commands,
    xr_pointers: Query<(&XrPointer, &PointerId)>,
    mut fields: Query<(
        &Field,
        &FieldGeometry,
        &GlobalTransform,
        Option<&mut FieldDragAction>,
        Entity,
    )>,
    robots: Query<(&Robot, &Team, &Transform, &ChildOf)>,
) {
    for (field, field_geometry, field_transform, mut drag_action, field_entity) in fields.iter_mut()
    {
        let drag_bounds = field_geometry.play_area_size + field_geometry.boundary_width * 2.0;

        let (pointer_hit, dragging_robot_id, &mut dragging_robot_team) =
            if let Some(FieldDragAction(pointer_id, robot_id, robot_team, _last_send)) =
                drag_action.as_deref_mut()
            {
                // Continue active drag with the same pointer.
                let hit = xr_pointers
                    .iter()
                    .filter(|(p, _)| p.trigger_pressed)
                    .find(|(_, id)| **id == *pointer_id)
                    .and_then(|(pointer, _)| {
                        field_intersection(pointer, field_transform, drag_bounds).inspect(|hit| {
                            gizmos.sphere(pointer.ray.get_point(hit.depth), 0.01, Color::WHITE);
                        })
                    });

                let Some(hit) = hit else {
                    _ = field
                        .connection
                        .sender
                        .send_blocking(ws_request::Content::MoveRobot(RobotMoveCommand {
                            robot_id: *robot_id as u32,
                            is_blue: *robot_team == Team::Blue,
                            p_x: None,
                            p_y: None,
                        }));
                    commands.entity(field_entity).remove::<FieldDragAction>();
                    continue;
                };

                (hit, *robot_id, robot_team)
            } else {
                // Start a drag if any pointer hits a robot on this field.
                let Some((hit, pointer_id)) = xr_pointers
                    .iter()
                    .filter(|(p, _)| p.trigger_pressed)
                    .find_map(|(pointer, pointer_id)| {
                        field_intersection(pointer, field_transform, drag_bounds)
                            .map(|hit| (hit, *pointer_id))
                            .inspect(|hit| {
                                gizmos.sphere(
                                    pointer.ray.get_point(hit.0.depth),
                                    0.01,
                                    Color::WHITE,
                                );
                            })
                    })
                else {
                    continue;
                };

                let Some((robot_id, robot_team)) = find_hit_robot(&robots, field_entity, hit.pos)
                else {
                    continue;
                };

                commands.entity(field_entity).insert(FieldDragAction(
                    pointer_id,
                    robot_id,
                    robot_team,
                    Instant::now(),
                ));
                continue;
            };

        if let Some(FieldDragAction(_, _, _, last_send)) = drag_action.as_deref_mut()
            && last_send.elapsed() > std::time::Duration::from_millis(30)
        {
            _ = field
                .connection
                .sender
                .send_blocking(ws_request::Content::MoveRobot(RobotMoveCommand {
                    robot_id: dragging_robot_id as u32,
                    is_blue: dragging_robot_team == Team::Blue,
                    p_x: Some(pointer_hit.pos.x),
                    p_y: Some(pointer_hit.pos.y),
                }));
            *last_send = Instant::now();
        }
    }
}
