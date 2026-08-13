use crate::panels::floating_anchor::FloatingPanelAnchor;
use bevy::color::palettes::tailwind::ZINC_700;
use bevy::feathers::FeathersPlugins;
use bevy::feathers::controls::FeathersToggleSwitch;
use bevy::feathers::dark_theme::create_dark_theme;
use bevy::feathers::display::{label, label_small};
use bevy::feathers::theme::UiTheme;
use bevy::prelude::*;
use bevy::ui::Checked;
use bevy::ui_widgets::ValueChange;
use sslgame::RobotRenderSettings;
use sslgame::panels::{SpatialPanelScaling, SpatialPanelSpawner};
use std::f32::consts::PI;

pub fn global_settings_plugin(app: &mut App) {
    app.add_plugins(FeathersPlugins)
        .insert_resource(UiTheme(create_dark_theme()))
        .add_systems(
            Update,
            (manage_global_settings_panel, update_global_settings_panel),
        );
}

fn manage_global_settings_panel(
    mut commands: Commands,
    mut panel_spawner: SpatialPanelSpawner,
    (floating_anchor, q_panels): (
        Single<Entity, With<FloatingPanelAnchor>>,
        Query<&GlobalSettingsPanel>,
    ),
) {
    if !q_panels.is_empty() {
        return;
    }

    // Spawn new panel
    let graphics_panel = panel_spawner.spawn_panel(
        &mut commands,
        Transform {
            translation: Vec3::ZERO,
            rotation: Quat::from_rotation_y(PI),
            scale: Vec3::new(0.5, 0.5, 1.),
        },
        SpatialPanelScaling {
            physical_px_per_meter: 1000.0,
            logical_px_per_meter: 300.0,
        },
        Color::srgba(0., 0., 0., 0.),
        global_settings_panel(),
    );

    commands.entity(*floating_anchor).add_child(graphics_panel);
}

#[derive(Component, Clone, Debug, FromTemplate)]
pub struct GlobalSettingsPanel {
    field_toggle: Entity,
    robots_toggle: Entity,
    ball_toggle: Entity,
    vis_toggle: Entity,
}

impl Default for GlobalSettingsPanel {
    fn default() -> Self {
        Self {
            field_toggle: Entity::PLACEHOLDER,
            ball_toggle: Entity::PLACEHOLDER,
            robots_toggle: Entity::PLACEHOLDER,
            vis_toggle: Entity::PLACEHOLDER,
        }
    }
}

fn global_settings_panel() -> impl Scene {
    bsn! {
        #GlobalSettingsPanel
        GlobalSettingsPanel {
            field_toggle: #FieldToggle,
            robots_toggle: #RobotsToggle,
            ball_toggle: #BallToggle,
            vis_toggle: #VisToggle,
        }
        Node {
            width: percent(100),
            height: percent(100),
            padding: px(10.),
            border_radius: px(5.),
            flex_direction: FlexDirection::Column,
            align_content: AlignContent::Center,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            justify_items: JustifyItems::Center,
            row_gap: px(10.),
        }
        BackgroundColor(ZINC_700)
        Children [
            label("Rendering Settings"),
            (
                #FieldToggleRow
                Node {
                    width: percent(100),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                }
                Children [
                    label_small("Field"),
                    (
                        #FieldToggle
                        @FeathersToggleSwitch
                        on(|change: On<ValueChange<bool>>, mut commands: Commands, mut render_settings: ResMut<sslgame::RenderSettings>| {
                            render_settings.field = change.value;
                            if change.value {
                                commands.entity(change.source).insert(Checked);
                            } else {
                                commands.entity(change.source).remove::<Checked>();
                            }
                        })
                    )
                ]
            ),
            (
                #RobotsToggleRow
                Node {
                    width: percent(100),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                }
                Children [
                    label_small("Robots"),
                    (
                        #RobotsToggle
                        @FeathersToggleSwitch
                        on(|change: On<ValueChange<bool>>, mut commands: Commands, mut render_settings: ResMut<sslgame::RenderSettings>| {
                            render_settings.robots = if change.value { RobotRenderSettings::Fallback } else { RobotRenderSettings::Cutout };
                            if change.value {
                                commands.entity(change.source).insert(Checked);
                            } else {
                                commands.entity(change.source).remove::<Checked>();
                            }
                        })
                    )
                ]
            ),
            (
                #BallToggleRow
                Node {
                    width: percent(100),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                }
                Children [
                    label_small("Ball"),
                    (
                        #BallToggle
                        @FeathersToggleSwitch
                        on(|change: On<ValueChange<bool>>, mut commands: Commands, mut render_settings: ResMut<sslgame::RenderSettings>| {
                            render_settings.ball = change.value;
                            if change.value {
                                commands.entity(change.source).insert(Checked);
                            } else {
                                commands.entity(change.source).remove::<Checked>();
                            }
                        })
                    )
                ]
            ),
            (
                #VisToggleRow
                Node {
                    width: percent(100),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                }
                Children [
                    label_small("Visualizations"),
                    (
                        #VisToggle
                        @FeathersToggleSwitch
                        on(|change: On<ValueChange<bool>>, mut commands: Commands, mut render_settings: ResMut<sslgame::RenderSettings>| {
                            render_settings.visualizations = change.value;
                            if change.value {
                                commands.entity(change.source).insert(Checked);
                            } else {
                                commands.entity(change.source).remove::<Checked>();
                            }
                        })
                    )
                ]
            ),
        ]
    }
}

/// Updates the switch state whenever the underlying resource changes
fn update_global_settings_panel(
    mut commands: Commands,
    settings: Res<sslgame::RenderSettings>,
    q_panels: Query<Ref<GlobalSettingsPanel>>,
    q_checked: Query<&Checked>,
) {
    for panel in q_panels {
        if !(panel.is_added() || settings.is_changed()) {
            continue;
        }

        let field_checked = q_checked.contains(panel.field_toggle);
        let robots_checked = q_checked.contains(panel.robots_toggle);
        let ball_checked = q_checked.contains(panel.ball_toggle);
        let vis_checked = q_checked.contains(panel.vis_toggle);

        if !field_checked && settings.field {
            commands.entity(panel.field_toggle).insert(Checked);
        } else if field_checked && !settings.field {
            commands.entity(panel.field_toggle).remove::<Checked>();
        }
        if !robots_checked && settings.robots == RobotRenderSettings::Fallback {
            commands.entity(panel.robots_toggle).insert(Checked);
        } else if robots_checked && settings.robots != RobotRenderSettings::Fallback {
            commands.entity(panel.robots_toggle).remove::<Checked>();
        }
        if !ball_checked && settings.ball {
            commands.entity(panel.ball_toggle).insert(Checked);
        } else if ball_checked && !settings.ball {
            commands.entity(panel.ball_toggle).remove::<Checked>();
        }
        if !vis_checked && settings.visualizations {
            commands.entity(panel.vis_toggle).insert(Checked);
        } else if vis_checked && !settings.visualizations {
            commands.entity(panel.vis_toggle).remove::<Checked>();
        }
    }
}
