use crate::icons::icon;
use crate::sidebar::SelectedField;
use crate::ui::{regular_text, tokens};
use crate::viewport::ViewportShowsCamera;
use crate::viewport::panorbit_cam::PanOrbitCamera;
use crate::viewport::robot_cam::RobotCam;
use bevy::ecs::template::EntityTemplate;
use bevy::feathers::theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor, ThemeToken};
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::{Pressed, Selected};
use bevy::ui_widgets::{ControlOrientation, SelectedTab, Tab, TabActivation, TabList, ValueChange};
use sslgame::fields::Team;
use sslgame::fields::robots::Robot;

pub fn camera_tabs_plugin(app: &mut App) {
    app.add_observer(on_camera_tab_selected);
    app.add_systems(
        Update,
        (
            handle_field_selection_change,
            update_tab_stacks,
            update_camera_tab_selection,
            scroll_camera_tabs,
            update_tab_ui,
            update_tab_styles,
        )
            .chain(),
    );
}

pub fn scene(viewport_entity: EntityTemplate) -> impl Scene {
    bsn! {
        #CameraTabBar
        Node {
            height: px(26),
            padding: UiRect::horizontal(px(6)),
            column_gap: px(6),
        }
        Children [
            #TabBarIcon
            Node {
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                padding: px(4),
            }
            Children [
                @icon(lucide_icons::Icon::Video, px(20)) ThemeTextColor(tokens::TEXT_1)
            ]
            --
            #TabBarRow
            Node {
                flex_grow: 1.0,
                column_gap: px(6),
            }
            CameraTabBar {
                viewport: viewport_entity,
            }
            TabList {
                orientation: ControlOrientation::Horizontal,
                activation: TabActivation::Manual,
            }
            Children [
                #GlobalTab
                CameraTabStackScope::Global
                Node {
                    display: Display::None,
                }
            ]
        ]
    }
}

/// Marks a tab bar and stores which viewport it controls.
#[derive(Component, FromTemplate)]
struct CameraTabBar {
    viewport: Entity,
}

#[derive(Component, Clone, Copy, Default, PartialEq)]
enum CameraTabStackScope {
    #[default]
    Global,
    FieldRobots {
        field: Entity,
        team: Team,
    },
}

#[derive(Component, Clone, Debug, PartialEq)]
struct CameraTabStack {
    color: Option<ThemeToken>,
    cams: Vec<(CamType, Entity)>,
    front_idx: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CamType {
    PanOrbit,
    Robot(Team, u8),
}

fn handle_field_selection_change(
    mut commands: Commands,
    selected_field: Res<SelectedField>,
    q_bars: Query<(&mut SelectedTab, Option<&Children>, Entity), With<CameraTabBar>>,
    q_tabs: Query<&CameraTabStackScope>,
) {
    if !selected_field.is_changed() {
        return;
    }

    let selected_field_entity = selected_field.get();

    for (mut bar_selection, bar_children, bar_entity) in q_bars {
        // Remove tabs for different fields
        if let Some(tabs) = bar_children {
            let mut first_remaining = None;
            let mut removed_selection = false;

            for &tab_entity in tabs {
                let Ok(tab_scope) = q_tabs.get(tab_entity) else {
                    continue;
                };

                let field_entity = match tab_scope {
                    CameraTabStackScope::Global => None,
                    CameraTabStackScope::FieldRobots { field, .. } => Some(*field),
                };

                if field_entity.is_some() && field_entity != selected_field_entity {
                    removed_selection |= bar_selection.0 == Some(tab_entity);
                    commands.entity(tab_entity).despawn();
                } else if first_remaining.is_none() {
                    first_remaining = Some(tab_entity);
                }
            }

            if removed_selection {
                bar_selection.0 = first_remaining;
            }
        }

        // Add new tabs for robots on the field
        if let Some(selected_field) = selected_field_entity {
            commands.spawn((
                Name::new("YellowRobotTab"),
                ChildOf(bar_entity),
                CameraTabStackScope::FieldRobots {
                    field: selected_field,
                    team: Team::Yellow,
                },
                Node {
                    display: Display::None,
                    ..default()
                },
            ));
            commands.spawn((
                Name::new("BlueRobotTab"),
                ChildOf(bar_entity),
                CameraTabStackScope::FieldRobots {
                    field: selected_field,
                    team: Team::Blue,
                },
                Node {
                    display: Display::None,
                    ..default()
                },
            ));
        }
    }
}

/// [`CameraTabStackScope`] -> [`CameraTabStack`]
fn update_tab_stacks(
    mut commands: Commands,
    q_bars: Query<&CameraTabBar>,
    q_tabs: Query<(
        &CameraTabStackScope,
        Option<&mut CameraTabStack>,
        &ChildOf,
        Entity,
    )>,
    q_panorbit_cams: Query<(Entity, &PanOrbitCamera)>,
    q_robot_cams: Query<(Entity, &ChildOf), With<RobotCam>>,
    q_robots: Query<(&Robot, &Team, &ChildOf)>,
) {
    let tabs_old = q_tabs.into_iter().collect::<Vec<_>>();
    let mut tabs_new = vec![
        CameraTabStack {
            color: Default::default(),
            cams: Vec::new(),
            front_idx: 0, // Will be transferred from old later
        };
        tabs_old.len()
    ];

    // Collect panorbit cams
    for (cam_entity, panorbit) in q_panorbit_cams {
        tabs_old
            .iter()
            .enumerate()
            .filter(|(_, (tab_scope, _, bar_ref, _))| {
                **tab_scope == CameraTabStackScope::Global
                    && q_bars
                        .get(bar_ref.0)
                        .is_ok_and(|bar| bar.viewport == panorbit.input_node)
            })
            .for_each(|(i, _)| tabs_new[i].cams.push((CamType::PanOrbit, cam_entity)));
    }

    // Collect robot cams
    for (cam_entity, robot_ref) in q_robot_cams {
        let (robot_id, robot_team, field_ref) = q_robots.get(robot_ref.0).unwrap();
        tabs_old
            .iter()
            .enumerate()
            .filter(|(_, (tab_scope, ..))| {
                **tab_scope
                    == CameraTabStackScope::FieldRobots {
                        field: field_ref.0,
                        team: *robot_team,
                    }
            })
            .for_each(|(i, _)| {
                tabs_new[i].color = match robot_team {
                    Team::Yellow => Some(tokens::YELLOW),
                    Team::Blue => Some(tokens::BLUE),
                };
                tabs_new[i]
                    .cams
                    .push((CamType::Robot(*robot_team, robot_id.0), cam_entity))
            });
    }

    // Sort the new stacks
    for stack in tabs_new.iter_mut() {
        stack.cams.sort_by_key(|(cam_type, _)| *cam_type);
    }

    // Update the component value
    for ((_, old, _, entity), mut new) in tabs_old.into_iter().zip(tabs_new) {
        if let Some(mut old) = old {
            if !new.cams.is_empty() {
                // Keep front index from old stack
                new.front_idx = old.front_idx.clamp(0, new.cams.len() - 1);
                old.set_if_neq(new);
            } else {
                commands.entity(entity).remove::<CameraTabStack>();
            }
        } else if !new.cams.is_empty() {
            commands.entity(entity).insert(new);
        }
    }
}

/// [`CameraTabStack`] -> UI
fn update_tab_ui(
    mut commands: Commands,
    q_tabs: Query<(Entity, &CameraTabStack), Changed<CameraTabStack>>,
    mut removed_tabs: RemovedComponents<CameraTabStack>,
) {
    // Hide tabs without a CameraTabStack (without cameras)
    for entity in removed_tabs.read() {
        if let Ok(mut entity_commands) = commands.get_entity(entity) {
            entity_commands.despawn_children().insert((Node {
                display: Display::None,
                ..default()
            },));
        }
    }

    // Update changed tabs with the new scene for the stack
    for (entity, tab_stack) in q_tabs.iter() {
        commands
            .entity(entity)
            .despawn_children()
            .apply_scene(tab_stack_scene(tab_stack));
    }
}

/// Synchronizes the [`SelectedTab`] on [`CameraTabBar`] with the active camera in [`ViewportShowsCamera`].
fn update_camera_tab_selection(
    mut q_bars: Query<(&CameraTabBar, &mut SelectedTab, Option<&Children>)>,
    mut q_tabs: Query<&mut CameraTabStack>,
    q_viewport_cam: Query<&ViewportShowsCamera>,
) {
    for (bar, mut selected_tab, tabs_ref) in q_bars.iter_mut() {
        let active_cam = q_viewport_cam.get(bar.viewport).ok().map(|cam| cam.0);
        let selected = active_cam.and_then(|active_cam_entity| {
            tabs_ref.into_iter().flatten().copied().find(|tab_entity| {
                if let Ok(mut tab) = q_tabs.get_mut(*tab_entity)
                    && let Some(pos) = tab.cams.iter().position(|(_, e)| *e == active_cam_entity)
                {
                    if tab.front_idx != pos {
                        tab.front_idx = pos;
                    }
                    return true;
                }
                false
            })
        });
        if selected_tab.0 != selected {
            selected_tab.0 = selected;
        }
    }
}

/// Scrolling over a tab steps the viewport through that group's cameras.
fn scroll_camera_tabs(
    mut commands: Commands,
    mut scroll_acc: Local<f32>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    mut q_tabs: Query<(&Hovered, Has<Selected>, &mut CameraTabStack, &ChildOf)>,
    q_bars: Query<&CameraTabBar>,
) {
    let hovered_tab = q_tabs.iter_mut().find(|(hovered, ..)| hovered.get());
    let Some((_, selected, mut stack, bar_ref)) = hovered_tab else {
        // Reset if not hovering
        *scroll_acc = 0.0;
        return;
    };

    // Decay when stopping
    if mouse_scroll.delta.y == 0.0 {
        // Framerate-dependent, but the exact speed doesn't matter that much
        *scroll_acc *= 0.8;
        if scroll_acc.abs() < 0.01 {
            *scroll_acc = 0.0;
        }
        return;
    }

    const SCROLL_PIXELS_PER_LINE: f32 = 30.0;
    let frac_step = match mouse_scroll.unit {
        MouseScrollUnit::Line => mouse_scroll.delta.y,
        MouseScrollUnit::Pixel => mouse_scroll.delta.y / SCROLL_PIXELS_PER_LINE,
    };

    // Reset if reversing
    if (*scroll_acc > 0.0 && frac_step < 0.0) || (*scroll_acc < 0.0 && frac_step > 0.0) {
        *scroll_acc = 0.0;
    }

    *scroll_acc += frac_step;

    let steps = *scroll_acc as isize;
    if steps == 0 {
        return;
    }

    *scroll_acc -= steps as f32;

    if stack.cams.is_empty() {
        return;
    }

    let Ok(bar) = q_bars.get(bar_ref.0) else {
        return;
    };

    stack.front_idx =
        (stack.front_idx as isize + steps).rem_euclid(stack.cams.len() as isize) as usize;

    // Switch to the new camera
    if selected {
        let target = stack.cams[stack.front_idx].1;
        commands
            .entity(bar.viewport)
            .insert(ViewportShowsCamera(target));
    }
}

fn on_camera_tab_selected(
    change: On<ValueChange<Option<Entity>>>,
    mut commands: Commands,
    q_bars: Query<&CameraTabBar>,
    q_tabs: Query<&CameraTabStack>,
    q_viewport_cam: Query<&ViewportShowsCamera>,
) {
    if let Ok(bar) = q_bars.get(change.source)
        && let Some(tab_entity) = change.value
        && let Ok(group) = q_tabs.get(tab_entity)
        && let Some((_, front_cam_entity)) = group.cams.get(group.front_idx)
    {
        let active = q_viewport_cam.get(bar.viewport).ok().map(|cam| cam.0);
        if Some(*front_cam_entity) != active {
            commands
                .entity(bar.viewport)
                .insert(ViewportShowsCamera(*front_cam_entity));
        }
    };
}

// ======== Scenes ========

fn tab_stack_scene(tab_stack: &CameraTabStack) -> impl Scene {
    let (front_cam_type, _) = &tab_stack.cams[tab_stack.front_idx];
    let front_icon = match front_cam_type {
        CamType::PanOrbit => lucide_icons::Icon::Rotate3D,
        CamType::Robot(_, _) => lucide_icons::Icon::Bot,
    };
    let label = match front_cam_type {
        CamType::PanOrbit => format!(
            "PanOrbit {}",
            tab_stack
                .cams
                .iter()
                .take(tab_stack.front_idx)
                .filter(|(t, _)| *t == CamType::PanOrbit)
                .count()
        ),
        CamType::Robot(team, robot_id) => format!("{team} Robot {robot_id}"),
    };

    let color: Box<dyn Scene> = match tab_stack.color.clone() {
        Some(token) => Box::new(bsn! {
            Node {
                height: percent(100),
                width: px(3),
                border_radius: px(3),
            }
            ThemeBackgroundColor(token)
        }),
        None => Box::new(bsn! {}),
    };

    let scroll_icon: Box<dyn Scene> = if tab_stack.cams.len() > 1 {
        Box::new(bsn! {
            @icon(lucide_icons::Icon::ChevronsUpDown, px(12)) ThemeTextColor(tokens::TEXT_1)
        })
    } else {
        Box::new(bsn! {})
    };

    bsn! {
        Tab
        Hovered
        Node {
            padding: px(4),
            column_gap: px(4),
            align_items: AlignItems::Center,
            border_radius: px(6),
            border: px(1),
        }
        ThemeBackgroundColor(tokens::WINDOW_BG)
        ThemeBorderColor(tokens::TABS_UNSELECTED_BORDER)
        Children [
            @color
            --
            @icon(front_icon, px(18)) ThemeTextColor(tokens::TEXT_0)
            --
            @regular_text(label, px(12))
            --
            @scroll_icon
        ]
    }
}

#[allow(clippy::type_complexity)]
fn update_tab_styles(
    mut commands: Commands,
    q_buttons: Query<
        (
            &Hovered,
            Has<Pressed>,
            Has<Selected>,
            &ThemeBorderColor,
            &ThemeBackgroundColor,
            Entity,
        ),
        With<CameraTabStack>,
    >,
) {
    for (hovered, pressed, selected, old_border, old_bg, button_entity) in q_buttons.iter() {
        let bg_token = match (hovered.0, pressed, selected) {
            (false, false, false) => tokens::WINDOW_BG,
            _ => tokens::TABS_SELECTED_BG,
        };

        let border_token = match selected {
            true => tokens::TABS_SELECTED_BORDER,
            _ => tokens::TABS_UNSELECTED_BORDER,
        };

        if old_bg.0 != bg_token {
            commands
                .entity(button_entity)
                .insert(ThemeBackgroundColor(bg_token));
        }
        if old_border.0 != border_token {
            commands
                .entity(button_entity)
                .insert(ThemeBorderColor(border_token));
        }
    }
}
