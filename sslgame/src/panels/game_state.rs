use crate::field::{FieldGeometry, GameState, Team};
use crate::panels::{SpatialPanelScaling, SpatialPanelSpawner};
use bevy::color::palettes::tailwind::*;
use bevy::prelude::*;
use std::f32::consts::PI;

pub fn game_state_panel_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            manage_game_state_panels,
            update_score_panel,
            update_team_panel,
        ),
    );
}

#[derive(Component, Clone, Debug, Default)]
struct FieldSidePanelAnchor;

/// Spawns score and team panels next to every field
#[allow(clippy::type_complexity)]
fn manage_game_state_panels(
    mut commands: Commands,
    mut panel_spawner: SpatialPanelSpawner,
    (q_fields, mut q_panels): (
        Query<(&Transform, Ref<FieldGeometry>, Entity), Without<FieldSidePanelAnchor>>,
        Query<(&mut Transform, &ChildOf), With<FieldSidePanelAnchor>>,
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
                let scaling = SpatialPanelScaling {
                    physical_px_per_meter: 500.0,
                    logical_px_per_meter: 100.0,
                };

                // Spawn new panels
                let score_panel = panel_spawner.spawn_panel(
                    &mut commands,
                    Transform {
                        translation: Vec3::new(0., 0.5, 0.),
                        rotation: Quat::from_rotation_x(PI / 6.),
                        scale: Vec3::new(0.5, 0.5, 1.),
                    },
                    scaling,
                    Color::srgba(0., 0., 0., 0.),
                    score_panel(field_entity),
                );
                let left_panel = panel_spawner.spawn_panel(
                    &mut commands,
                    Transform {
                        translation: Vec3::new(0.3 + 0.75, 0.5, 0.),
                        rotation: Quat::from_rotation_x(PI / 6.),
                        scale: Vec3::new(1.5, 0.5, 1.),
                    },
                    scaling,
                    Color::srgba(0., 0., 0., 0.),
                    team_panel(field_entity, Team::Yellow, true),
                );
                let right_panel = panel_spawner.spawn_panel(
                    &mut commands,
                    Transform {
                        translation: Vec3::new(-0.3 - 0.75, 0.5, 0.),
                        rotation: Quat::from_rotation_x(PI / 6.),
                        scale: Vec3::new(1.5, 0.5, 1.),
                    },
                    scaling,
                    Color::srgba(0., 0., 0., 0.),
                    team_panel(field_entity, Team::Blue, false),
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
                        FieldSidePanelAnchor,
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

#[derive(Component, FromTemplate, Clone, Debug)]
struct ScorePanel {
    state_source: Entity,
    left: Team,
    right: Team,

    score_text: Entity,
    stage_text: Entity,
}

impl Default for ScorePanel {
    fn default() -> Self {
        Self {
            state_source: Entity::PLACEHOLDER,
            left: Team::Yellow,
            right: Team::Blue,

            score_text: Entity::PLACEHOLDER,
            stage_text: Entity::PLACEHOLDER,
        }
    }
}

fn score_panel(state_source: Entity) -> impl Scene {
    bsn! {
        #ScorePanel
        ScorePanel {
            state_source: state_source,

            score_text: #Score,
            stage_text: #Stage,
        }
        Node {
            width: percent(100),
            height: percent(100),
            padding: px(5.),
            border_radius: px(5.),
            flex_direction: FlexDirection::Column,
            align_content: AlignContent::Center,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            justify_items: JustifyItems::Center,
        }
        BackgroundColor(ZINC_700)
        Children [
            (#Score Text("0:0") TextFont { font_size: px(20.) }),
            (#Stage Text("GameStage") TextFont { font_size: px(8.) }),
        ]
    }
}

fn update_score_panel(
    state_sources: Query<Ref<GameState>>,
    panels: Query<Ref<ScorePanel>>,
    mut texts: Query<&mut Text>,
) {
    for score_panel in panels {
        let Some(game_state) = state_sources
            .get(score_panel.state_source)
            .ok()
            .filter(|s| s.is_changed() || score_panel.is_added())
        else {
            continue;
        };
        let left_team = match score_panel.left {
            Team::Yellow => game_state.yellow_team.as_ref(),
            Team::Blue => game_state.blue_team.as_ref(),
        };
        let right_team = match score_panel.right {
            Team::Yellow => game_state.yellow_team.as_ref(),
            Team::Blue => game_state.blue_team.as_ref(),
        };

        if let Ok(mut t) = texts.get_mut(score_panel.score_text) {
            t.0 = format!(
                "{}:{}",
                left_team.and_then(|l| l.score).unwrap_or(0),
                right_team.and_then(|r| r.score).unwrap_or(0)
            )
        }
        if let Ok(mut t) = texts.get_mut(score_panel.stage_text) {
            t.0 = format!("{:?}", game_state.game_stage);
        }
    }
}

// ======== Team Panel  ========

#[derive(Component, FromTemplate, Clone, Debug)]
struct TeamPanel {
    state_source: Entity,
    team: Team,

    logo: Entity,
    name_text: Entity,
    fouls_text: Entity,
    yellow_text: Entity,
    red_text: Entity,
}

impl Default for TeamPanel {
    fn default() -> Self {
        Self {
            state_source: Entity::PLACEHOLDER,
            team: Team::default(),

            logo: Entity::PLACEHOLDER,
            name_text: Entity::PLACEHOLDER,
            fouls_text: Entity::PLACEHOLDER,
            yellow_text: Entity::PLACEHOLDER,
            red_text: Entity::PLACEHOLDER,
        }
    }
}

fn team_panel(state_source: Entity, team: Team, right_aligned: bool) -> impl Scene {
    let flex_direction = if right_aligned {
        FlexDirection::RowReverse
    } else {
        FlexDirection::Row
    };

    fn card_pill(color: Color) -> impl Scene {
        bsn! {
            Node {
                height: percent(100.),
                border_radius: percent(100.),
                padding: UiRect::horizontal(px(3.5)),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: px(3.),
            }
            BackgroundColor(color)
        }
    }

    bsn! {
        #TeamPanel
        TeamPanel {
            state_source: state_source,
            team: team,

            logo: #TeamLogo,
            name_text: #TeamName,
            fouls_text: #FoulText,
            yellow_text: #YellowCardText,
            red_text: #RedCardText,
        }
        Node {
            width: percent(100),
            height: percent(100),
            padding: px(5.),
            border_radius: px(5.),
            flex_direction,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
        }
        BackgroundColor(ZINC_700)
        Children [
            (
                #TeamLogo
                ImageNode { image: "teams/logos/unknown.png" }
                Node {
                    height: percent(100.),
                    aspect_ratio: {Some(1.)},
                }
            ),
            (
                #Content
                Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Stretch,
                    row_gap: px(2.),
                }
                Children [
                    (#TeamName Text("Unknown") TextFont{font_size: px(14.)}),
                    (
                        #CardRow
                        Node {
                            height: px(10.),
                            flex_direction,
                            justify_content: JustifyContent::FlexStart,
                            align_items: AlignItems::Center,
                            column_gap: px(5.),
                        }
                        Children [
                            (
                                #FoulPill
                                Node {
                                    height: percent(100.),
                                    aspect_ratio: {Some(1.)},
                                    border_radius: percent(100.),
                                    justify_content: JustifyContent::Center,
                                    align_items: AlignItems::Center,
                                }
                                BackgroundColor(ZINC_500)
                                Children [(#FoulText Text("0") TextFont{font_size: px(6.)})]
                            )
                            // Deduplicating these is impossible right now because we need to get the text references in the top-level component
                            (
                                #YellowCardPill
                                card_pill(YELLOW_400.into())
                                Children [
                                    (
                                        #YellowCardIcon
                                        ImageNode { image: "icons/card.png" }
                                        Node { width: px(6.), height: px(6.) }
                                    ),
                                    (#YellowCardText Text("0") TextFont{font_size: px(6.)})
                                ]
                            ),
                            (
                                #RedCardPill
                                card_pill(RED_400.into())
                                Children [
                                    (
                                        #RedCardIcon
                                        ImageNode { image: "icons/card.png" }
                                        Node { width: px(6.), height: px(6.) }
                                    ),
                                    (#RedCardText Text("0") TextFont{font_size: px(6.)})
                                ]
                            )
                        ]
                    )
                ]
            )
        ]
    }
}

fn update_team_panel(
    asset_server: Res<AssetServer>,
    state_sources: Query<Ref<GameState>>,
    panels: Query<Ref<TeamPanel>>,
    mut texts: Query<&mut Text>,
    mut images: Query<&mut ImageNode>,
) {
    for team_panel in panels.iter() {
        let Some(game_state) = state_sources
            .get(team_panel.state_source)
            .ok()
            .filter(|s| s.is_changed() || team_panel.is_added())
        else {
            continue;
        };
        let Some(team_state) = (match team_panel.team {
            Team::Yellow => game_state.yellow_team.as_ref(),
            Team::Blue => game_state.blue_team.as_ref(),
        }) else {
            continue;
        };

        let team_name = team_state
            .name
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        let team_logo = format!("teams/logos/{}.png", team_name.to_ascii_lowercase());
        let fouls = team_state.fouls.unwrap_or(0).to_string();
        let yellow_cards = team_state.yellow_cards.unwrap_or(0).to_string();
        let red_cards = team_state.red_cards.unwrap_or(0).to_string();

        if let Ok(mut i) = images.get_mut(team_panel.logo) {
            // Has to be manually loaded because automatic loading from strings only works in bsn
            i.image = asset_server.load(team_logo);
        }
        if let Ok(mut t) = texts.get_mut(team_panel.name_text) {
            t.0 = team_name;
        }
        if let Ok(mut t) = texts.get_mut(team_panel.fouls_text) {
            t.0 = fouls;
        }
        if let Ok(mut t) = texts.get_mut(team_panel.yellow_text) {
            t.0 = yellow_cards;
        }
        if let Ok(mut t) = texts.get_mut(team_panel.red_text) {
            t.0 = red_cards;
        }
    }
}
