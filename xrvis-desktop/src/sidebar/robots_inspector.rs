use crate::sidebar::inspector_base_scene;
use crate::ui::{TextOfComponent, regular_text, tokens};
use bevy::ecs::template::{EntityTemplate, TemplateContext};
use bevy::feathers;
use bevy::feathers::theme::ThemeBackgroundColor;
use bevy::prelude::*;
use sslgame::field::Team;
use sslgame::field::robots::Robot;

pub fn robots_inspector_plugin(app: &mut App) {
    app.add_observer(on_robot_add);
    app.add_systems(PostUpdate, update_header_robot_count);
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
#[relationship(relationship_target = RobotOfCard)]
struct CardForRobot(Entity);
#[derive(Component, Clone, Copy)]
#[relationship_target(relationship = CardForRobot, linked_spawn)]
struct RobotOfCard(Entity);

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
                        @regular_text(text) TextFont { font_size: px(14) }
                    ]
                    --
                    @regular_text(robot_count.to_string()) TextFont { font_size: px(14) }
                ]
            }
        }

        fn robots_grid(team: Team, robots: &[(Robot, Team, Entity)]) -> (impl Scene, usize) {
            let mut robots = robots
                .iter()
                .filter(|(_, robot_team, _)| *robot_team == team)
                .collect::<Vec<_>>();
            robots.sort_by_key(|(robot, _, _)| robot.0);
            let robot_cards = robots
                .into_iter()
                .map(|(robot, _, entity)| robot_card(team, robot.0, *entity))
                .collect::<Vec<_>>();
            let robot_count = robot_cards.len();
            (
                bsn! {
                    Node {
                        display: Display::Grid,
                        width: percent(100),
                        flex_grow: 1.0,
                        row_gap: px(6),
                        column_gap: px(6),
                        padding: UiRect::horizontal(px(6)),
                        grid_template_rows: {vec![RepeatedGridTrack::px(GridTrackRepetition::AutoFill, 80.0)]},
                        grid_template_columns: {vec![RepeatedGridTrack::flex(3, 1.0)]},
                    }
                    Children [{robot_cards}]
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

fn robot_card(team: Team, robot_id: u8, robot_entity: Entity) -> impl Scene {
    bsn! {
        #RobotCardRoot
        CardForRobot(robot_entity)
        Node {
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border_radius: px(4),
        }
        ThemeBackgroundColor(feathers::tokens::BUTTON_BG)
        Children [
            TextOfComponent(#RobotCardRoot) @regular_text(format!("Robot {}", robot_id))
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
        let card_entity = commands
            .spawn_scene(robot_card(*team, robot_id.0, robot_entity))
            .id();
        commands
            .entity(grid_entity)
            .queue(insert_child_sorted(card_entity));
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
            .get::<CardForRobot>(child)
            .and_then(|robot_ref| world.get::<Robot>(robot_ref.0).map(|robot| robot.0))
            .unwrap_or(0);

        let index = parent
            .get::<Children>()
            .into_iter()
            .flatten()
            .enumerate()
            .find_map(|(i, e)| {
                let this_robot_id = world
                    .get::<CardForRobot>(*e)
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
