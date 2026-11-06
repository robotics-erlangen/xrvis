use bevy::prelude::*;
use std::cmp::PartialEq;
use std::fmt::{Display, Formatter};

pub mod host_transfer;
pub mod robots;

/// Transfers data that was received by the [host_plugin](crate::hosts::host_plugin) to every fields connected via
/// host relations ([GeometryHost](crate::hosts::GeometryHost), [YellowRobotHost](crate::hosts::YellowRobotHost), ...).
/// Also sets up the resources for robot rendering.
pub fn field_plugin(app: &mut App) {
    app.add_plugins(robots::robot_plugin);
    app.add_plugins(host_transfer::host_field_transfer_plugin);
}

/// SSL team. Used both as a normal value and a component on robots.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Team {
    #[default]
    Yellow,
    Blue,
}

impl Display for Team {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Team::Yellow => write!(f, "Yellow"),
            Team::Blue => write!(f, "Blue"),
        }
    }
}

#[derive(Component, Debug)]
#[require(Visibility, Transform)]
pub struct Field;
