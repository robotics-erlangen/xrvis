use crate::icons::icon;
use crate::sidebar::inspector_base_scene;
use crate::ui::{ComponentText, TextOfComponent, regular_text, tokens};
use bevy::ecs::template::{EntityTemplate, TemplateContext};
use bevy::feathers;
use bevy::feathers::controls::{
    FeathersMenu, FeathersMenuButton, FeathersMenuItem, FeathersMenuPopup,
};
use bevy::feathers::theme::ThemeBackgroundColor;
use bevy::prelude::*;
use bevy::ui_widgets::{Activate, MenuAction, MenuEvent};
use sslgame::fields::Team;
use sslgame::fields::robots::Robot;
use sslgame::hosts::robot_control::{RobotControlledByGamepad, RobotControlledByKeyboard};

pub fn robots_inspector_plugin(app: &mut App) {
    app.add_observer(on_robot_add);
    app.add_systems(
        PostUpdate,
        (
            update_header_robot_count,
            (update_control_gamepad_options, update_control_selection).chain(),
        ),
    );
}

pub fn scene(field_entity: Entity) -> impl Scene {
    bsn! {
        RobotsInspector { field_entity }
    }
}

#[derive(Component, Clone, Copy)]
#[relationship(relationship_target = FieldRobotsInspectedBy)]
struct RobotsInspector(Entity);
#[derive(Component, Clone, Copy)]
#[relationship_target(relationship = RobotsInspector, linked_spawn)]
struct FieldRobotsInspectedBy(Entity);

#[derive(Component, FromTemplate, Clone, Copy)]
#[relationship(relationship_target = RobotOfInspectorEntry)]
struct InspectorEntryForRobot(Entity);
#[derive(Component, Clone, Copy)]
#[relationship_target(relationship = InspectorEntryForRobot, linked_spawn)]
struct RobotOfInspectorEntry(Entity);

#[derive(Default)]
struct RobotsInspectorTemplate {
    field_entity: EntityTemplate,
}
impl FromTemplate for RobotsInspector {
    type Template = RobotsInspectorTemplate;
}
impl Template for RobotsInspectorTemplate {
    type Output = RobotsInspector;

    fn build_template(&self, context: &mut TemplateContext) -> Result<Self::Output> {
        let field_entity = self.field_entity.build_template(context)?;

        let robots = context.entity.world_scope(|world| {
            let mut q_robots = world.query::<(&Robot, &Team, Entity)>();
            let field_children = world
                .entity(field_entity)
                .get::<Children>()
                .into_iter()
                .flatten();
            q_robots
                .iter_many(world, field_children)
                .matched()
                .map(|(robot, team, entity)| (*robot, *team, entity))
                .collect::<Vec<_>>()
        });

        fn header(team: Team, robot_count: usize) -> impl Scene {
            let text = match team {
                Team::Yellow => "Yellow Robots",
                Team::Blue => "Blue Robots",
            };
            let (higher_col, lower_col) = match team {
                Team::Yellow => (tokens::YELLOW, tokens::YELLOW_1),
                Team::Blue => (tokens::BLUE, tokens::BLUE_1),
            };
            bsn! {
                Node {
                    width: percent(100),
                    height: px(24),
                    flex_direction: FlexDirection::Row,
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    padding: UiRect::right(px(6)),
                    column_gap: px(6),
                    border_radius: px(4),
                }
                ThemeBackgroundColor(lower_col)
                Children [
                    Node {
                        width: percent(100),
                        height: percent(100),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border_radius: px(4),
                    }
                    ThemeBackgroundColor(higher_col)
                    Children [
                        @regular_text(text, px(14))
                    ]
                    --
                    @regular_text(robot_count.to_string(), px(14))
                ]
            }
        }

        fn robots_grid(team: Team, robots: &[(Robot, Team, Entity)]) -> (impl Scene, usize) {
            let mut robots = robots
                .iter()
                .filter(|(_, robot_team, _)| *robot_team == team)
                .collect::<Vec<_>>();
            robots.sort_by_key(|(robot, _, _)| robot.0);
            let robot_entries = robots
                .into_iter()
                .map(|(robot, _, entity)| robot_entry_scene(robot.0, *entity))
                .collect::<Vec<_>>();
            let robot_count = robot_entries.len();
            (
                bsn! {
                    Node {
                        flex_direction: FlexDirection::Column,
                        width: percent(100),
                        flex_grow: 1.0,
                        row_gap: px(6),
                        padding: UiRect::horizontal(px(6)),
                    }
                    Children [{robot_entries}]
                },
                robot_count,
            )
        }

        let (yellow_robot_grid, yellow_robot_count) = robots_grid(Team::Yellow, &robots);
        let (blue_robot_grid, blue_robot_count) = robots_grid(Team::Blue, &robots);

        let scene = bsn! {
            @inspector_base_scene()
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                padding: px(6),
            }
            Children [
                @header(Team::Yellow, yellow_robot_count)
                --
                @yellow_robot_grid
                --
                @header(Team::Blue, blue_robot_count)
                --
                @blue_robot_grid
            ]
        };

        context.entity.apply_scene(scene)?;
        Ok(RobotsInspector(field_entity))
    }

    fn clone_template(&self) -> Self {
        Self {
            field_entity: self.field_entity,
        }
    }
}

fn robot_entry_scene(robot_id: u8, robot_entity: Entity) -> impl Scene {
    bsn! {
        #RobotEntryRoot
        InspectorEntryForRobot(robot_entity)
        Node {
            width: percent(100),
            padding: {UiRect::all(px(3)).with_left(px(6))},
            align_items: AlignItems::Center,
            border_radius: px(4),
            column_gap: px(6),
        }
        ThemeBackgroundColor(feathers::tokens::BUTTON_BG)
        Children [
            TextOfComponent(#RobotEntryRoot) @regular_text(robot_id.to_string(), px(16))
            --
            Node {flex_grow: 1.0}
            --
            @control_selector_scene(robot_entity)
        ]
    }
}

fn on_robot_add(
    event: On<Add<Robot>>,
    mut commands: Commands,
    q_robot: Query<(&Robot, &Team)>,
    (q_parent, q_children): (Query<&ChildOf>, Query<&Children>),
    q_inspector_ref: Query<&FieldRobotsInspectedBy>,
) {
    let robot_entity = event.entity;
    if let Ok((robot_id, team)) = q_robot.get(robot_entity)
        && let Ok(field_entity) = q_parent.get(robot_entity)
        && let Ok(inspector_entity) = q_inspector_ref.get(field_entity.0)
        && let Ok(inspector_children) = q_children.get(inspector_entity.0)
        && let grid_entity = match team {
            Team::Yellow => inspector_children[1],
            Team::Blue => inspector_children[3],
        }
    {
        let entry_entity = commands
            .spawn_scene(robot_entry_scene(robot_id.0, robot_entity))
            .id();
        commands
            .entity(grid_entity)
            .queue(insert_child_sorted(entry_entity));
    }
}

/// Updates the counter in the header whenever the grid's children change
fn update_header_robot_count(
    q_inspectors: Query<&Children, With<RobotsInspector>>,
    q_children: Query<Ref<Children>>,
    mut q_text: Query<&mut Text>,
) {
    for inspector_children in &q_inspectors {
        if let &[yellow_header, yellow_grid, blue_header, blue_grid] =
            inspector_children.collection().as_slice()
        {
            for (header_entity, grid_entity) in
                [(yellow_header, yellow_grid), (blue_header, blue_grid)]
            {
                let grid_children = q_children.get(grid_entity);
                let count = if let Ok(grid_children) = grid_children {
                    if grid_children.is_changed() {
                        grid_children.len()
                    } else {
                        continue;
                    }
                } else {
                    0
                };

                if let Ok(header_children) = q_children.get(header_entity)
                    && let Some(&text_entity) = header_children.get(1)
                    && let Ok(mut text) = q_text.get_mut(text_entity)
                {
                    let count_str = count.to_string();
                    if text.0 != count_str {
                        text.0 = count_str;
                    }
                }
            }
        }
    }
}

/// Inserts a child entity, sorted by the id of its referenced robot.
fn insert_child_sorted(child: Entity) -> impl EntityCommand {
    move |mut parent: EntityWorldMut| -> Result<(), BevyError> {
        let world = parent.world();

        let new_robot_id = world
            .get::<InspectorEntryForRobot>(child)
            .and_then(|robot_ref| world.get::<Robot>(robot_ref.0).map(|robot| robot.0))
            .unwrap_or(0);

        let index = parent
            .get::<Children>()
            .into_iter()
            .flatten()
            .enumerate()
            .find_map(|(i, e)| {
                let this_robot_id = world
                    .get::<InspectorEntryForRobot>(*e)
                    .and_then(|robot_ref| world.get::<Robot>(robot_ref.0).map(|robot| robot.0))
                    .unwrap_or(0);
                (new_robot_id < this_robot_id).then_some(i)
            });

        if let Some(i) = index {
            parent.insert_child(i, child);
        } else {
            parent.add_child(child);
        }

        Ok(())
    }
}

// ======== Control selector dropdown ========

#[derive(Component, FromTemplate, Clone, Copy)]
struct RobotControlButtonIcon(Entity);

#[derive(Component, FromTemplate, Clone, Copy)]
struct RobotControlPopup(Entity);

#[derive(Component, FromTemplate, Clone, Copy)]
struct RobotControlPopupOption {
    robot: Entity,
    source: RobotControlSource,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
enum RobotControlSource {
    #[default]
    Ai,
    Keyboard,
    Gamepad(Entity),
}

impl RobotControlSource {
    fn icon(self) -> lucide_icons::Icon {
        match self {
            Self::Ai => lucide_icons::Icon::BrainCircuit,
            Self::Keyboard => lucide_icons::Icon::Keyboard,
            Self::Gamepad(_) => lucide_icons::Icon::Gamepad2,
        }
    }
}

/// A button and dropdown that switches the control source of a robot.
fn control_selector_scene(robot_entity: Entity) -> impl Scene {
    bsn! {
        @FeathersMenu
            Children [
                @FeathersMenuButton {
                    @caption: bsn! {
                        @icon(lucide_icons::Icon::BrainCircuit, px(16))
                        RobotControlButtonIcon(robot_entity)
                    },
                }
                Node {
                    height: percent(100),
                    padding: {UiRect::all(px(3))},
                }
                --
                @FeathersMenuPopup
                RobotControlPopup(robot_entity)
                Children [
                    @control_option_scene(robot_entity, RobotControlSource::Ai, "AI")
                    --
                    @control_option_scene(robot_entity, RobotControlSource::Keyboard, "Keyboard")
                ]
            ]
    }
}

/// One entry in the control selector dropdown.
fn control_option_scene(
    robot: Entity,
    source: RobotControlSource,
    label: impl Into<String>,
) -> impl Scene {
    bsn! {
        #OptionRoot
        @FeathersMenuItem {
            @caption: bsn_list! {
                @icon(source.icon(), px(16))
                --
                @regular_text(label, px(14)) TextOfComponent(#OptionRoot)
                --
                @icon(lucide_icons::Icon::Check, px(14))
                Visibility::Hidden
            },
        }
        Node { column_gap: px(6) }
        RobotControlPopupOption { robot, source }
        on(on_control_option_activate)
    }
}

/// Changes the control source of a robot when its option is clicked.
fn on_control_option_activate(
    event: On<Activate>,
    q_options: Query<&RobotControlPopupOption>,
    mut commands: Commands,
) {
    let Ok(&RobotControlPopupOption { robot, source }) = q_options.get(event.entity) else {
        return;
    };

    commands
        .entity(robot)
        .remove::<(RobotControlledByKeyboard, RobotControlledByGamepad)>();
    match source {
        RobotControlSource::Ai => {}
        RobotControlSource::Keyboard => {
            commands.entity(robot).insert(RobotControlledByKeyboard);
        }
        RobotControlSource::Gamepad(gamepad) => {
            commands
                .entity(robot)
                .insert(RobotControlledByGamepad(gamepad));
        }
    }

    commands.trigger(MenuEvent {
        source: event.entity,
        action: MenuAction::CloseAll,
    });
    commands.trigger(MenuEvent {
        source: event.entity,
        action: MenuAction::FocusRoot,
    });
}

/// Updates the options in the popup to react to gamepad changes.
fn update_control_gamepad_options(
    q_gamepads: Query<(Entity, Option<&Name>), With<Gamepad>>,
    q_popups: Query<(Entity, &RobotControlPopup, &Children)>,
    q_options: Query<(Entity, &RobotControlPopupOption, &ComponentText)>,
    mut q_labels: Query<&mut Text>,
    mut commands: Commands,
) {
    fn gamepad_label(entity: Entity, name: Option<&Name>) -> String {
        format!("{} ({entity})", name.map_or("Gamepad", Name::as_str))
    }

    let mut current_gamepads = q_gamepads.iter().collect::<Vec<_>>();
    current_gamepads.sort_by_key(|(entity, _)| entity.to_bits());

    for (popup_entity, robot_ref, options) in &q_popups {
        let mut current_gamepad_options = Vec::new();
        for (option_entity, option, option_text_ref) in q_options.iter_many(options).matched() {
            if let RobotControlSource::Gamepad(gamepad) = option.source {
                // Remove the option if its gamepad was disconnected
                let Ok((_, name)) = q_gamepads.get(gamepad) else {
                    commands.entity(option_entity).try_despawn();
                    continue;
                };
                current_gamepad_options.push(gamepad);
                // Update label in the name changed
                if let Ok(mut text) = q_labels.get_mut(*option_text_ref.collection()) {
                    let label = gamepad_label(gamepad, name);
                    text.set_if_neq(Text(label));
                }
            }
        }
        // Spawn options for new gamepads
        for &(gamepad, name) in &current_gamepads {
            if !current_gamepad_options.contains(&gamepad) {
                commands
                    .spawn_scene(control_option_scene(
                        robot_ref.0,
                        RobotControlSource::Gamepad(gamepad),
                        gamepad_label(gamepad, name),
                    ))
                    .insert(ChildOf(popup_entity));
            }
        }
    }
}

/// Updates the button and the dropdown options of the control selector whenever the control source of a robot changes.
fn update_control_selection(
    q_robots: Query<
        (
            Has<RobotControlledByKeyboard>,
            Option<&RobotControlledByGamepad>,
        ),
        With<Robot>,
    >,
    mut q_icons: Query<(&RobotControlButtonIcon, &mut Text)>,
    q_options: Query<(&RobotControlPopupOption, &Children)>,
    mut q_visibility: Query<&mut Visibility>,
) {
    // Gets the current control source of a robot
    let control_of_robot = |robot| match q_robots.get(robot) {
        Ok((_, Some(gamepad))) => RobotControlSource::Gamepad(gamepad.0),
        Ok((true, None)) => RobotControlSource::Keyboard,
        _ => RobotControlSource::Ai,
    };

    // Update the button icon
    for (robot, mut text) in &mut q_icons {
        let icon_glyph = control_of_robot(robot.0).icon().unicode().to_string();
        text.set_if_neq(Text(icon_glyph));
    }

    // Update the checkmark in the dropdown
    for (option, children) in &q_options {
        let visibility = if control_of_robot(option.robot) == option.source {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(check_entity) = children.last()
            && let Ok(mut check_visibility) = q_visibility.get_mut(*check_entity)
        {
            check_visibility.set_if_neq(visibility);
        }
    }
}
