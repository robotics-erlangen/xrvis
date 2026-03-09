use bevy::color::palettes::tailwind::*;
use bevy::prelude::*;

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
