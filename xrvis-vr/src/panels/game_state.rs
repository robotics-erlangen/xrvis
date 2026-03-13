use crate::panels::{XrPanelAnchor, XrPanelSpawner};
use bevy::color::palettes::tailwind::*;
use bevy::prelude::*;
use sslgame::FieldGeometry;
use std::f32::consts::PI;

pub fn manage_game_state_panels(
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
                        parent.spawn(score_panel());
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
                        parent.spawn(team_panel(team_icon_left, card_icon_left, true));
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
                        parent.spawn(team_panel(team_icon_right, card_icon_right, false));
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

pub fn score_panel() -> impl Bundle {
    (
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

pub fn team_panel(team_logo: Handle<Image>, card_icon: Handle<Image>, mirror: bool) -> impl Bundle {
    let flex_direction = if mirror {
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
