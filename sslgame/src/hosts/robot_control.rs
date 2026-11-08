use crate::fields::robots::Robot;
use crate::fields::{Field, Team};
use crate::hosts::{BlueRobotHost, HostConnection, YellowRobotHost};
use crate::proto;
use crate::proto::robot_command::{Rotation, Translation};
use bevy::ecs::system::SystemParam;
use bevy::prelude::ops::atan2;
use bevy::prelude::*;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::f32::consts::PI;
use std::time::{Duration, Instant};

const MOVE_SPEED: f32 = 1.5;
const ROT_SPEED: f32 = 3.0;

/// Adds the [RobotCommandProcessor] to hosts to safely send robot commands and handles
/// the [RobotControlledByKeyboard] and [RobotControlledByGamepad] components.
pub fn robot_control_plugin(app: &mut App) {
    app.add_observer(on_gamepad_removed);
    app.add_systems(
        Update,
        (
            process_gamepad_robot_control,
            process_keyboard_robot_control,
        ),
    );
    app.register_required_components::<HostConnection, RobotCommandProcessor>();
    app.add_systems(PostUpdate, send_robot_command);
}

/// Collects robot commands each frame to be sent at regular intervals. Commands should be set every frame.
#[derive(Component, Default)]
pub struct RobotCommandProcessor {
    /// Commands submitted in the current frame
    current_commands: HashMap<proto::RobotRef, proto::RobotCommand>,
    /// List of robots that were sent commands with the last update
    last_commands: Vec<proto::RobotRef>,
}

impl RobotCommandProcessor {
    /// Sets the command for a single robot for this frame. Will be merged with any previous commands from this frame.
    pub fn set(&mut self, new: proto::RobotCommand) {
        /// Returns the only value or the result of the function if both are set
        fn merge_option<T>(a: Option<T>, b: Option<T>, f: impl FnOnce(T, T) -> T) -> Option<T> {
            match (a, b) {
                (Some(a), Some(b)) => Some(f(a, b)),
                (a, b) => a.or(b),
            }
        }

        match self.current_commands.entry(new.robot) {
            Entry::Occupied(mut entry) => {
                let base = entry.get_mut();

                base.hold_ball = merge_option(base.hold_ball, new.hold_ball, |a, b| a || b);
                base.translation =
                    merge_option(base.translation, new.translation, |a, b| match (a, b) {
                        (Translation::WorldPos(a), Translation::WorldPos(b)) => {
                            // Pos average isn't commutative for 2+ inputs, but it's still better than fully replacing the last target
                            Translation::WorldPos(proto::robot_command::TranslationVector {
                                x: merge_option(a.x, b.x, |a, b| (a + b) / 2.0),
                                y: merge_option(a.y, b.y, |a, b| (a + b) / 2.0),
                            })
                        }
                        (Translation::WorldVel(a), Translation::WorldVel(b)) => {
                            Translation::WorldVel(proto::robot_command::TranslationVector {
                                x: merge_option(a.x, b.x, |a, b| a + b),
                                y: merge_option(a.y, b.y, |a, b| a + b),
                            })
                        }
                        (Translation::LocalVel(a), Translation::LocalVel(b)) => {
                            Translation::LocalVel(proto::robot_command::TranslationVector {
                                x: merge_option(a.x, b.x, |a, b| a + b),
                                y: merge_option(a.y, b.y, |a, b| a + b),
                            })
                        }
                        _ => b,
                    });
                base.rotation = merge_option(base.rotation, new.rotation, |a, b| match (a, b) {
                    (Rotation::AnglePos(a), Rotation::AnglePos(b)) => {
                        // Pos average isn't commutative for 2+ inputs, but it's still better than fully replacing the last target
                        Rotation::AnglePos(((a + b) / 2.0).rem_euclid(2.0 * PI))
                    }
                    (Rotation::AngleVel(a), Rotation::AngleVel(b)) => Rotation::AngleVel(a + b),
                    _ => b,
                });
                base.kick = new.kick.or(base.kick);
            }
            Entry::Vacant(entry) => {
                entry.insert(new);
            }
        }
    }
}

/// Updates the [RobotCommandProcessor] components and sends their commands at regular intervals.
fn send_robot_command(
    mut last_send: Local<Option<Instant>>,
    mut q_hosts: Query<(&mut RobotCommandProcessor, &HostConnection)>,
) {
    let now = Instant::now();
    let should_send =
        last_send.is_none_or(|last_send| now.duration_since(last_send) > Duration::from_millis(50));

    if !should_send {
        for (mut processor, _) in &mut q_hosts {
            processor.current_commands.clear();
        }
        return;
    }

    for (mut commands, connection) in &mut q_hosts {
        let mut previous_robots = std::mem::take(&mut commands.last_commands);
        let mut sent_robots = Vec::with_capacity(commands.current_commands.len());

        for (_, command) in commands.current_commands.drain() {
            // Remove this robot from the previous set. The remaining ones will get a cancel command.
            if let Some(index) = previous_robots
                .iter()
                .position(|robot| *robot == command.robot)
            {
                previous_robots.swap_remove(index);
            }

            sent_robots.push(command.robot);
            connection
                .sender
                .send_blocking(proto::ws_request::Content::ControlRobot(command))
                .unwrap();
        }

        for robot in previous_robots {
            // Send a cancel command for each robot that had a command last update but doesn't now.
            connection
                .sender
                .send_blocking(proto::ws_request::Content::ControlRobot(
                    proto::RobotCommand { robot, ..default() },
                ))
                .unwrap();
        }

        commands.last_commands = sent_robots;
    }

    *last_send = Some(now);
}

/// Makes input from movement sources like [`RobotControlledByGamepad`] relative to a fixed
/// forward direction in field coordinates instead of that robot's local orientation.
#[derive(Component, Clone, Copy)]
pub struct WorldSpaceRobotControl {
    pub field_forward: Vec2,
}

/// Controls a [Robot] in local coordinates using keyboard inputs.
#[derive(Component, Default)]
pub struct RobotControlledByKeyboard;

/// Controls a [Robot] in local coordinates using the referenced [Gamepad].
#[derive(Component)]
#[relationship(relationship_target = GamepadControlsRobots)]
pub struct RobotControlledByGamepad(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = RobotControlledByGamepad)]
pub struct GamepadControlsRobots(Vec<Entity>);

/// Gamepad disconnects only remove the [Gamepad] component, but leave its entity intact to remember
/// [GamepadSettings] across reconnections (see [gamepad_connection_system](bevy::input::gamepad::gamepad_connection_system)).
/// This removes the [RobotControlledByGamepad] relationship from the gamepad to free the controlled robots.
fn on_gamepad_removed(event: On<Remove<Gamepad>>, mut commands: Commands) {
    commands
        .entity(event.entity)
        .remove::<GamepadControlsRobots>();
}

/// Helper [SystemParam] for sending robot commands from a field.
/// Automatically handles the "field -> team robot host" lookup.
#[derive(SystemParam)]
pub struct RobotCommands<'w, 's> {
    q_hosts: Query<'w, 's, &'static mut RobotCommandProcessor>,
    q_fields: Query<
        'w,
        's,
        (
            Option<&'static YellowRobotHost>,
            Option<&'static BlueRobotHost>,
        ),
        With<Field>,
    >,
}

impl<'w, 's> RobotCommands<'w, 's> {
    /// Forwards a robot command to [RobotCommandProcessor::set] on the correct host for that team on this field.
    pub fn submit(&mut self, field_entity: Entity, command: proto::RobotCommand) {
        if let Ok((yellow_host_ref, blue_host_ref)) = self.q_fields.get(field_entity) {
            let team = if command.robot.is_blue() {
                Team::Blue
            } else {
                Team::Yellow
            };
            let host = match (team, yellow_host_ref, blue_host_ref) {
                (Team::Yellow, Some(host_ref), _) => self.q_hosts.get_mut(host_ref.0).ok(),
                (Team::Blue, _, Some(host_ref)) => self.q_hosts.get_mut(host_ref.0).ok(),
                _ => None,
            };

            if let Some(mut host_commands) = host {
                host_commands.set(command);
            }
        }
    }
}

/// Implements the [RobotControlledByKeyboard] behavior
fn process_keyboard_robot_control(
    mut robot_commands: RobotCommands,
    q_robots: Query<
        (&Robot, &Team, Option<&WorldSpaceRobotControl>, &ChildOf),
        With<RobotControlledByKeyboard>,
    >,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    for (robot, team, global_control, field_ref) in q_robots.iter() {
        // Normal global coordinates are right-handed, +x forward. So the local ones should be too.
        let move_input = Vec2::new(
            keyboard.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) as i32 as f32
                - keyboard.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) as i32 as f32,
            keyboard.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) as i32 as f32
                - keyboard.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) as i32 as f32,
        );
        let rotate_input = keyboard.pressed(KeyCode::KeyQ) as i32 as f32
            - keyboard.pressed(KeyCode::KeyE) as i32 as f32;
        let hold_ball = keyboard.any_pressed([KeyCode::Space, KeyCode::KeyR]);

        let kick = if keyboard.pressed(KeyCode::KeyF) {
            Some(proto::robot_command::Kick::LinearKickSpeed(3.0))
        } else if keyboard.pressed(KeyCode::KeyG) {
            Some(proto::robot_command::Kick::ChipKickDistance(2.0))
        } else {
            None
        };

        let command = set_command_movement(
            proto::RobotCommand {
                robot: proto::RobotRef {
                    robot_id: robot.0 as u32,
                    is_blue: Some(*team == Team::Blue),
                },
                hold_ball: Some(hold_ball),
                translation: None,
                rotation: None,
                kick,
            },
            move_input,
            Vec2::Y.rotate_angle(rotate_input),
            global_control.copied(),
        );

        robot_commands.submit(field_ref.0, command);
    }
}

/// Implements the [RobotControlledByGamepad] behavior
fn process_gamepad_robot_control(
    mut robot_commands: RobotCommands,
    q_robots: Query<(
        &Robot,
        &Team,
        &RobotControlledByGamepad,
        Option<&WorldSpaceRobotControl>,
        &ChildOf,
    )>,
    q_gamepads: Query<&Gamepad>,
) {
    for (robot, team, gamepad_ref, global_control, field_ref) in q_robots.iter() {
        let gamepad = q_gamepads.get(gamepad_ref.0).unwrap();

        let left_stick = gamepad.left_stick();
        let right_stick = gamepad.right_stick();

        let hold_ball = gamepad.pressed(GamepadButton::LeftTrigger)
            || gamepad.pressed(GamepadButton::LeftTrigger2);

        let kick = if gamepad.pressed(GamepadButton::RightTrigger) {
            Some(proto::robot_command::Kick::ChipKickDistance(2.0))
        } else if gamepad.pressed(GamepadButton::RightTrigger2) {
            Some(proto::robot_command::Kick::LinearKickSpeed(3.0))
        } else {
            None
        };

        let command = set_command_movement(
            proto::RobotCommand {
                robot: proto::RobotRef {
                    robot_id: robot.0 as u32,
                    is_blue: Some(*team == Team::Blue),
                },
                hold_ball: Some(hold_ball),
                translation: None,
                rotation: None,
                kick,
            },
            left_stick,
            right_stick,
            global_control.copied(),
        );

        robot_commands.submit(field_ref.0, command);
    }
}

/// Helper for setting [`RobotCommand::translation`](proto::RobotCommand::translation) and
/// [`RobotCommand::rotation`](proto::RobotCommand::rotation) based on the inputs and that
/// robot's [`WorldSpaceRobotControl`].
fn set_command_movement(
    base: proto::RobotCommand,
    move_input: Vec2,
    rot_input: Vec2,
    global_control: Option<WorldSpaceRobotControl>,
) -> proto::RobotCommand {
    match global_control {
        Some(global) => {
            let look_angle = atan2(global.field_forward.y, global.field_forward.x);
            let world_input = move_input.rotate_angle(look_angle) * MOVE_SPEED;

            proto::RobotCommand {
                translation: Some(Translation::WorldVel(
                    proto::robot_command::TranslationVector {
                        x: Some(world_input.y),
                        y: Some(-world_input.x),
                    },
                )),
                rotation: if rot_input.length_squared() > 0.1 {
                    Some(Rotation::AnglePos(
                        (atan2(-rot_input.x, rot_input.y) + look_angle)
                            .rem_euclid(2.0 * std::f32::consts::PI),
                    ))
                } else {
                    None
                },
                ..base
            }
        }
        None => proto::RobotCommand {
            translation: Some(Translation::LocalVel(
                proto::robot_command::TranslationVector {
                    x: Some(move_input.y),
                    y: Some(-move_input.x),
                },
            )),
            rotation: if rot_input.x.abs() > 0.1 {
                Some(Rotation::AngleVel(-rot_input.x * ROT_SPEED))
            } else {
                None
            },
            ..base
        },
    }
}
