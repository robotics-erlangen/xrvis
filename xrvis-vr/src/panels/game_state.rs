use crate::panels::{XrPanelAnchor, XrPanelSpawner};
use bevy::color::palettes::tailwind::*;
use bevy::prelude::*;
use sslgame::{FieldGeometry, GameState, Team};
use std::f32::consts::PI;

pub fn game_state_panel_plugin(app: &mut App) {
    app.add_systems(Update, manage_game_state_panels);
    app.add_systems(Update, update_score_panel);
    app.add_systems(Update, update_team_panel);
}

fn manage_game_state_panels(
    mut commands: Commands,
    mut panel_spawner: XrPanelSpawner,
    asset_server: Res<AssetServer>,
    (q_fields, mut q_panels): (
        Query<(&Transform, Ref<FieldGeometry>, Entity), Without<XrPanelAnchor>>,
        Query<(&mut Transform, &ChildOf), With<XrPanelAnchor>>,
    ),
) {
    for (field_transform, field_geom, field_entity) in q_fields {
        let panel_anchor = q_panels
            .iter_mut()
            .find(|(_, c)| c.parent() == field_entity);

        match panel_anchor {
            Some((mut anchor_transform, _)) if field_geom.is_changed() => {
                // Update position
                let new_anchor_pos = field_transform.translation
                    + field_transform.forward()
                        * (field_geom.play_area_size.y / 2.0 + field_geom.boundary_width + 0.1);
                anchor_transform.translation = new_anchor_pos;
                anchor_transform.look_at(Vec3::ZERO, Vec3::Y);
            }
            None => {
                // Spawn new panel
                let score_panel = panel_spawner.spawn_panel(
                    &mut commands,
                    Transform {
                        translation: Vec3::new(0., 0.5, 0.),
                        rotation: Quat::from_rotation_x(PI / 6.),
                        scale: Vec3::new(0.5, 0.5, 1.),
                    },
                    Color::srgba(0., 0., 0., 0.),
                    move |parent| {
                        parent.spawn(score_panel(field_entity));
                    },
                );
                let team_icon_left = asset_server.load("teams/logos/erforce_light.png");
                let team_icon_right = team_icon_left.clone();
                let card_icon_left = asset_server.load("icons/card.png");
                let card_icon_right = card_icon_left.clone();
                let left_panel = panel_spawner.spawn_panel(
                    &mut commands,
                    Transform {
                        translation: Vec3::new(0.3 + 0.75, 0.5, 0.),
                        rotation: Quat::from_rotation_x(PI / 6.),
                        scale: Vec3::new(1.5, 0.5, 1.),
                    },
                    Color::srgba(0., 0., 0., 0.),
                    move |parent| {
                        parent.spawn(team_panel(
                            field_entity,
                            team_icon_left,
                            card_icon_left,
                            Team::Yellow,
                            true,
                        ));
                    },
                );
                let right_panel = panel_spawner.spawn_panel(
                    &mut commands,
                    Transform {
                        translation: Vec3::new(-0.3 - 0.75, 0.5, 0.),
                        rotation: Quat::from_rotation_x(PI / 6.),
                        scale: Vec3::new(1.5, 0.5, 1.),
                    },
                    Color::srgba(0., 0., 0., 0.),
                    move |parent| {
                        parent.spawn(team_panel(
                            field_entity,
                            team_icon_right,
                            card_icon_right,
                            Team::Blue,
                            false,
                        ));
                    },
                );

                let panel_anchor = commands
                    .spawn((
                        Transform::from_translation(
                            field_transform.translation
                                + field_transform.forward()
                                    * (field_geom.play_area_size.y / 2.0
                                        + field_geom.boundary_width
                                        + 0.1),
                        )
                        .looking_at(Vec3::ZERO, Vec3::Y),
                        XrPanelAnchor,
                    ))
                    .add_children(&[score_panel, left_panel, right_panel])
                    .id();
                commands.entity(field_entity).add_child(panel_anchor);
            }
            _ => {}
        }
    }
}

// ======== Score Panel  ========

#[derive(Component, Debug)]
struct ScorePanel {
    state_source: Entity,
    left: Team,
    right: Team,
}

fn score_panel(state_source: Entity) -> impl Bundle {
    (
        ScorePanel {
            state_source,
            left: Team::Yellow,
            right: Team::Blue,
        },
        Node {
            width: percent(100),
            height: percent(100),
            padding: UiRect::all(px(5.)),
            border_radius: BorderRadius::all(px(5.)),
            flex_direction: FlexDirection::Column,
            align_content: AlignContent::Center,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            justify_items: JustifyItems::Center,
            ..default()
        },
        BackgroundColor(ZINC_700.into()),
        children![
            (Text::new("0:0"), TextFont::from_font_size(20.)),
            (Text::new("GameStage"), TextFont::from_font_size(8.))
        ],
    )
}

fn update_score_panel(
    state_sources: Query<Ref<GameState>>,
    panels: Query<(&ScorePanel, &Children)>,
    mut texts: Query<&mut Text>,
) {
    for (score_panel, children) in panels.iter() {
        let game_state = state_sources.get(score_panel.state_source).unwrap();
        if !game_state.is_changed() {
            continue;
        }
        let left_team = match score_panel.left {
            Team::Yellow => game_state.yellow_team.as_ref(),
            Team::Blue => game_state.blue_team.as_ref(),
        };
        let right_team = match score_panel.right {
            Team::Yellow => game_state.yellow_team.as_ref(),
            Team::Blue => game_state.blue_team.as_ref(),
        };

        let [mut score_text, mut game_stage_text] = texts
            .get_many_mut([children[0].entity(), children[1].entity()])
            .unwrap();
        score_text.0 = format!(
            "{}:{}",
            left_team.and_then(|l| l.score).unwrap_or(0),
            right_team.and_then(|r| r.score).unwrap_or(0)
        );
        game_stage_text.0 = format!("{:?}", game_state.game_stage);
    }
}

// ======== Team Panel  ========

#[derive(Component, Debug)]
struct TeamPanel {
    state_source: Entity,
    team: Team,
}

fn team_panel(
    state_source: Entity,
    team_logo: Handle<Image>,
    card_icon: Handle<Image>,
    team: Team,
    right_aligned: bool,
) -> impl Bundle {
    let flex_direction = if right_aligned {
        FlexDirection::RowReverse
    } else {
        FlexDirection::Row
    };

    fn card_pill(color: Color, icon: Handle<Image>, text: &str) -> impl Bundle {
        (
            Node {
                height: percent(100.),
                border_radius: BorderRadius::all(percent(100.)),
                padding: UiRect::horizontal(px(3.5)),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(3.),
                ..default()
            },
            BackgroundColor(color),
            children![
                (
                    ImageNode::new(icon),
                    Node {
                        width: px(6.),
                        height: px(6.),
                        ..default()
                    }
                ),
                (Text::new(text), TextFont::from_font_size(6.))
            ],
        )
    }

    (
        TeamPanel { state_source, team },
        Node {
            width: percent(100),
            height: percent(100),
            padding: UiRect::all(px(5.)),
            border_radius: BorderRadius::all(px(5.)),
            flex_direction,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            ..default()
        },
        BackgroundColor(ZINC_700.into()),
        children![
            (
                ImageNode::new(team_logo),
                Node {
                    height: percent(100.),
                    aspect_ratio: Some(1.),
                    ..default()
                }
            ),
            (
                Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Stretch,
                    row_gap: px(2.),
                    ..default()
                },
                children![
                    (Text::new("ER-Force"), TextFont::from_font_size(14.)),
                    (
                        Node {
                            height: px(10.),
                            flex_direction,
                            justify_content: JustifyContent::FlexStart,
                            align_items: AlignItems::Center,
                            column_gap: px(5.),
                            ..default()
                        },
                        children![
                            (
                                Node {
                                    height: percent(100.),
                                    aspect_ratio: Some(1.),
                                    border_radius: BorderRadius::all(percent(100.)),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                    ..default()
                                },
                                BackgroundColor(ZINC_500.into()),
                                children![(Text::new("8"), TextFont::from_font_size(6.))]
                            ),
                            card_pill(YELLOW_400.into(), card_icon.clone(), "3"),
                            card_pill(RED_400.into(), card_icon, "1"),
                        ]
                    )
                ]
            )
        ],
    )
}

fn update_team_panel(
    state_sources: Query<Ref<GameState>>,
    panels: Query<(&TeamPanel, &Children)>,
    nodes: Query<&Children, With<Node>>,
    mut icons: Query<&mut ImageNode>,
    mut texts: Query<&mut Text>,
) {
    for (team_panel, children) in panels.iter() {
        let game_state = state_sources.get(team_panel.state_source).unwrap();
        if !game_state.is_changed() {
            continue;
        }
        let team_state = match team_panel.team {
            Team::Yellow => game_state.yellow_team.as_ref(),
            Team::Blue => game_state.blue_team.as_ref(),
        };

        // - Team Panel
        //   - <Image> Team Logo
        //   - <Node> Content Parent
        //     - <Text> Team Name
        //     - <Node> Pill Parent
        //       - <Node> Foul Pill
        //         - <Text> Fouls
        //       - <Node> Yellow Pill
        //         - <Image> Card Icon
        //         - <Text> Yellow Cards
        //       - <Node> Red Pill
        //         - <Image> Card Icon
        //         - <Text> Red Cards

        let mut _icon = icons.get_mut(children[0].entity()).unwrap();
        // TODO: Update icon

        let content_parent = nodes.get(children[1].entity()).unwrap();

        let mut team_name = texts.get_mut(content_parent[0].entity()).unwrap();
        team_name.0 = team_state
            .and_then(|t| t.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        // Get pill nodes
        let pill_parent = nodes.get(content_parent[1].entity()).unwrap();
        let foul_pill = nodes.get(pill_parent[0].entity()).unwrap();
        let [yellow_pill, red_pill] = nodes
            .get_many([pill_parent[1].entity(), pill_parent[2].entity()])
            .unwrap();

        // Update pill texts
        let [mut fouls, mut yellow_cards, mut red_cards] = texts
            .get_many_mut([
                foul_pill[0].entity(),
                yellow_pill[1].entity(),
                red_pill[1].entity(),
            ])
            .unwrap();
        fouls.0 = team_state.and_then(|b| b.fouls).unwrap_or(0).to_string();
        yellow_cards.0 = team_state
            .and_then(|b| b.yellow_cards)
            .unwrap_or(0)
            .to_string();
        red_cards.0 = team_state
            .and_then(|b| b.red_cards)
            .unwrap_or(0)
            .to_string();
    }
}
