use crate::interaction::input::{InputActions, LeftHandPointer, RightHandPointer};
use bevy::app::App;
use bevy::asset::uuid::Uuid;
use bevy::camera::{NormalizedRenderTarget, RenderTarget};
use bevy::ecs::system::SystemParam;
use bevy::picking::PickingSystems;
use bevy::picking::pointer::{
    Location, PointerAction, PointerId, PointerInput, PointerLocation, PointerPressState,
};
use bevy::prelude::*;
use schminput::BoolActionValue;
use sslgame::fields::robots::Robot;
use sslgame::fields::{Field, Team};
use sslgame::hosts::GeometryHost;
use sslgame::hosts::data::FieldGeometry;
use sslgame::hosts::robot_control::RobotCommands;
use sslgame::panels::{SpatialPanel, SpatialUiRoot};
use sslgame::proto::{RobotCommand, RobotMoveTarget, RobotRef, robot_command};
use std::ops::Range;

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
    pos_2d: Vec2,
    pos_3d: Vec3,
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
            pos_2d: surface_hit,
            pos_3d: global_hit,
            depth: intersect_dist,
            in_bounds,
            in_range,
        })
    }
}

pub fn update_hand_pointer_rays(
    mut gizmos: Gizmos,
    pointer_actions: Res<InputActions>,
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
    pointers: Query<(&XrPointer, &PointerId, &PointerLocation, &PointerPressState)>,
    // Panels
    panels: Query<(&GlobalTransform, Option<&InheritedVisibility>), With<SpatialPanel>>,
    ui_roots: Query<(&UiTargetCamera, &SpatialUiRoot)>,
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
        for (ui_cam, &SpatialUiRoot(panel)) in ui_roots {
            // Query the render target of the ui camera
            let (render_target, texture_size, scale_factor) =
                if let Ok(RenderTarget::Image(img_target)) = render_targets.get(ui_cam.entity()) {
                    let image = image_assets.get(&img_target.handle).unwrap();
                    (
                        NormalizedRenderTarget::Image(img_target.clone()),
                        image.size(),
                        img_target.scale_factor,
                    )
                } else {
                    warn!(
                        "Failed to get image render target for xr panel camera {}",
                        ui_cam.entity()
                    );
                    continue;
                };

            // Get the transform of the display mesh (the physical panel)
            let (panel_transform, panel_visibility) = panels.get(panel).unwrap();

            // Skip hidden panels
            if !panel_visibility.is_none_or(|v| v.get()) {
                continue;
            }

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
            let normalized_surface_hit = surface_hit.pos_2d / panel_transform.scale().xy();
            // Get logical pixel position on the render target texture (top-left origin, y-down)
            // The manual scaling is necessary because only the global UiScale is handled automatically, but the panels use the individual render target scale instead.
            let pointer_pos =
                (normalized_surface_hit + 0.5) * texture_size.as_vec2() / scale_factor;

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

        // Sending a cancel event when moving directly to a different panel is not necessary
        // because dragging locks the cursor to one panel and hover state works across panels

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

#[derive(Component, Debug)]
#[relationship_target(relationship = DraggingRobot)]
pub struct RobotDraggedBy(Entity);

#[derive(Component, Debug)]
#[relationship(relationship_target = RobotDraggedBy)]
pub struct DraggingRobot(Entity);

#[derive(SystemParam)]
struct FieldRaycaster<'w, 's> {
    q_fields: Query<'w, 's, (&'static GeometryHost, &'static GlobalTransform, Entity), With<Field>>,
    q_geometry_hosts: Query<'w, 's, &'static FieldGeometry>,
}

impl<'w, 's> FieldRaycaster<'w, 's> {
    fn intersect(&self, field_entity: Entity, pointer: &XrPointer) -> Option<XrSurfaceHit> {
        let (geom_host_ref, field_transform, _) = self.q_fields.get(field_entity).ok()?;
        let geometry = self.q_geometry_hosts.get(geom_host_ref.0).ok()?;

        pointer.intersect_plane(
            field_transform.translation(),
            field_transform.up(),
            field_transform.right(),
            field_transform.forward(),
            geometry.play_area_size + geometry.boundary_width * 2.0,
        )
    }

    fn intersect_first(&self, pointer: &XrPointer) -> Option<(Entity, XrSurfaceHit)> {
        self.q_fields
            .iter()
            .filter_map(|(_, _, field_entity)| {
                self.intersect(field_entity, pointer)
                    .map(|hit| (field_entity, hit))
            })
            .filter(|(_, hit)| hit.in_bounds && hit.in_range)
            .min_by(|(_, hit1), (_, hit2)| hit1.depth.partial_cmp(&hit2.depth).unwrap())
    }
}

#[allow(clippy::type_complexity)]
fn drive_field_dragging(
    mut gizmos: Gizmos,
    mut commands: Commands,
    q_xr_pointers: Query<(&XrPointer, Option<&DraggingRobot>, Entity)>,
    field_raycaster: FieldRaycaster,
    mut robot_commands: RobotCommands,
    q_robots: Query<(
        &Robot,
        &Team,
        &Transform,
        Has<RobotDraggedBy>,
        &ChildOf,
        Entity,
    )>,
) {
    for (pointer, dragged_robot_ref, pointer_entity) in q_xr_pointers {
        let (field_entity, robot_ref, field_hit) = if let Some(dragged_robot_ref) =
            dragged_robot_ref
        {
            let (robot_id, robot_team, _, _, field_ref, _) =
                q_robots.get(dragged_robot_ref.0).unwrap();

            if let Some(field_hit) = field_raycaster.intersect(field_ref.0, pointer)
                && pointer.trigger_pressed
                && field_hit.in_range
            {
                gizmos.sphere(field_hit.pos_3d, 0.01, Color::WHITE);

                (
                    field_ref.0,
                    RobotRef {
                        robot_id: robot_id.0 as u32,
                        is_blue: Some(*robot_team == Team::Blue),
                    },
                    field_hit,
                )
            } else {
                commands.entity(pointer_entity).remove::<DraggingRobot>();
                continue;
            }
        } else if let Some((field_entity, field_hit)) = field_raycaster.intersect_first(pointer) {
            gizmos.sphere(field_hit.pos_3d, 0.01, Color::WHITE);

            if !pointer.trigger_pressed {
                continue;
            }

            // Pointing at a free robot?
            let Some((robot_id, robot_team, robot_entity)) = q_robots
                .iter()
                .find(
                    |(_, _, robot_transform, has_dragged, ChildOf(robot_parent), _)| {
                        if *robot_parent != field_entity || *has_dragged {
                            return false;
                        }
                        (robot_transform.translation.xz() * Vec2::new(1., -1.))
                            .distance_squared(field_hit.pos_2d)
                            < 0.1 * 0.1
                    },
                )
                .map(|(robot, team, _, _, _, entity)| (*robot, *team, entity))
            else {
                continue;
            };

            commands
                .entity(pointer_entity)
                .insert(DraggingRobot(robot_entity));

            (
                field_entity,
                RobotRef {
                    robot_id: robot_id.0 as u32,
                    is_blue: Some(robot_team == Team::Blue),
                },
                field_hit,
            )
        } else {
            continue;
        };

        robot_commands.submit(
            field_entity,
            RobotCommand {
                robot: robot_ref,
                hold_ball: None,
                move_target: Some(robot_command::MoveTarget::WorldPos(RobotMoveTarget {
                    x: Some(field_hit.pos_2d.x),
                    y: Some(field_hit.pos_2d.y),
                    phi: None,
                })),
                kick_command: None,
            },
        );
    }
}
