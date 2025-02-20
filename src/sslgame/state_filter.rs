use crate::sslgame::proto::amun_compact;
use crate::sslgame::proto::amun_compact::vis_part::Geom;
use crate::sslgame::{FieldGeometry, GameInfo};
use bevy::prelude::*;
use std::collections::{HashSet, VecDeque};
use std::f32::consts::PI;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{AtomicI64, AtomicU32};
use std::time::{Duration, Instant};

// TODO: Reset offset after timeout

#[derive(Component)]
pub struct StateFilter {
    /// Sliding window of the past received packets with their timestamp relative to time_reference.
    packet_history: VecDeque<(u64, amun_compact::Status)>,

    /// Constant reference time to derive the timestamps from
    time_reference: Instant,
    /// Estimated offset of the timestamps in the incoming packets to the local timestamps derived from time_reference.
    /// Is adjusted dynamically to get the optimal buffer delay for the current connection quality.
    time_offset: Option<i64>,

    health_tracking_period: Duration,
    /// Saves the minimum observed remaining buffer time (in µs) to the latest available packet
    /// and the number of stutters over a period of time.
    buffer_health_tracker: Option<BufferHealthTracker>,
}

struct BufferHealthTracker {
    min_buffer_health: AtomicI64,
    stutter_count: AtomicU32,
    scheduled_time: Instant,
}

impl Default for StateFilter {
    fn default() -> Self {
        Self {
            packet_history: Default::default(),
            time_reference: Instant::now(),
            time_offset: None,
            health_tracking_period: Duration::from_secs(10),
            buffer_health_tracker: None,
        }
    }
}

impl StateFilter {
    pub fn current_game_state(&self) -> GameInfo {
        let latest = self.packet_history.front().map(|s| &s.1);

        latest.map_or(GameInfo::default(), |s| GameInfo {
            yellow_team: s.yellow_team.clone().map_or("".to_string(), |t| t.name),
            blue_team: s.blue_team.clone().map_or("".to_string(), |t| t.name),
        })
    }

    pub fn field_geometry_update(&self) -> Option<FieldGeometry> {
        let latest = self.packet_history.iter().find_map(|p| p.1.field_geometry);

        latest.map(|g| FieldGeometry {
            play_area_size: Vec2::new(g.field_width, g.field_height),
            boundary_width: g.boundary_width.unwrap_or(0.0),
            defense_size: Vec2::new(g.defense_width, g.defense_height),
            goal_width: g.goal_width,
        })
    }

    pub fn current_world_state(&self) -> amun_compact::WorldState {
        let curr_timestamp = self.time_reference.elapsed().as_micros() as u64;

        // Find relevant packets
        let (mut prev, mut next) = (None, None);
        for packet in self
            .packet_history
            .iter()
            .filter(|(_, p)| p.world_state.is_some())
        {
            next = prev;
            prev = Some(packet);
            if packet.0 < curr_timestamp {
                break;
            }
        }

        match (prev, next) {
            (Some((prev_time, prev)), Some((next_time, next))) => {
                if let Some(buffer_health_tracker) = &self.buffer_health_tracker {
                    let (latest_world_timestamp, _) = self
                        .packet_history
                        .iter()
                        .find(|(_, p)| p.world_state.is_some())
                        .unwrap();
                    buffer_health_tracker.min_buffer_health.fetch_min(
                        *latest_world_timestamp as i64 - curr_timestamp as i64,
                        SeqCst,
                    );
                }

                interpolate_world_state(
                    curr_timestamp,
                    *prev_time,
                    prev.world_state.as_ref().unwrap(),
                    *next_time,
                    next.world_state.as_ref().unwrap(),
                )
            }
            (Some((prev_time, prev)), None) => {
                if let Some(buffer_health_tracker) = &self.buffer_health_tracker {
                    buffer_health_tracker.stutter_count.fetch_add(1, SeqCst);
                    buffer_health_tracker
                        .min_buffer_health
                        .fetch_min(*prev_time as i64 - curr_timestamp as i64, SeqCst);
                }
                prev.world_state.clone().unwrap()
            }
            (None, Some(_next)) => unreachable!("Next can only be set with from a prev value"),
            (None, None) => amun_compact::WorldState::default(),
        }
    }

    pub fn visualization_updates(&self) -> (u32, HashSet<u32>, Vec<amun_compact::Visualization>) {
        let mut group_count = None;
        let mut groups = HashSet::new();
        let mut visualizations = Vec::new();

        // TODO: Short-circuit vis collection when every group is populated
        self.packet_history
            .iter()
            .filter(|(_, s)| s.visualization_group.is_some())
            .map(|(_, s)| (s.visualization_group.unwrap(), &s.visualization))
            .take_while(|(group, _)| {
                if let Some(prev_count) = group_count {
                    group.group_count == prev_count
                } else {
                    group_count = Some(group.group_count);
                    true
                }
            })
            .for_each(|(group, vis_list)| {
                if !groups.contains(&group.group) {
                    groups.insert(group.group);
                    vis_list.iter().for_each(|v| visualizations.push(v.clone()));
                }
            });

        (group_count.unwrap_or(1), groups, visualizations)
    }

    pub fn push_packet(&mut self, mut status: amun_compact::Status) {
        let now = Instant::now();
        let current_timestamp = (now - self.time_reference).as_micros() as u64;

        // Set initial offset
        if self.time_offset.is_none() {
            self.time_offset = Some(current_timestamp as i64 - status.timestamp.unwrap() as i64);
            self.buffer_health_tracker = Some(BufferHealthTracker {
                min_buffer_health: AtomicI64::new(i64::MAX),
                stutter_count: AtomicU32::new(0),
                scheduled_time: now + Duration::from_secs(1),
            });
        }

        // Adjust offset
        if let Some(buffer_health_tracker) = &self.buffer_health_tracker {
            let min_time = buffer_health_tracker.min_buffer_health.load(SeqCst);
            let stutter_count = buffer_health_tracker.stutter_count.load(SeqCst);
            if buffer_health_tracker.scheduled_time < now
                && min_time < i64::MAX
                && !(min_time < 0
                    && stutter_count < (self.health_tracking_period.as_secs() / 5) as u32)
            // One stutter every five seconds is okay
            {
                self.time_offset = self
                    .time_offset
                    .map(|old_offset| old_offset + (1000 - min_time));
                self.buffer_health_tracker = Some(BufferHealthTracker {
                    min_buffer_health: AtomicI64::new(i64::MAX),
                    stutter_count: AtomicU32::new(0),
                    scheduled_time: now + self.health_tracking_period,
                });
            }
        }

        remap_coordinates(&mut status);

        // Insert the new packet into buffer, ordered by its converted local timestamp
        let new_timestamp = (status.timestamp.unwrap() as i64 + self.time_offset.unwrap()) as u64;
        let insert_index = self
            .packet_history
            .iter()
            .take_while(|(timestamp, _)| *timestamp > new_timestamp)
            .count();
        self.packet_history
            .insert(insert_index, (new_timestamp, status));

        // Remove old packets from the buffer
        self.packet_history.truncate(
            self.packet_history
                .iter()
                .take_while(|(timestamp, _)| current_timestamp < timestamp + 1000000)
                .count(),
        );
    }
}

fn interpolate_world_state(
    curr_time: u64,
    prev_time: u64,
    prev: &amun_compact::WorldState,
    next_time: u64,
    next: &amun_compact::WorldState,
) -> amun_compact::WorldState {
    fn interpolate_robots(
        prev: &[amun_compact::Robot],
        next: &[amun_compact::Robot],
        ratio: f32,
    ) -> Vec<amun_compact::Robot> {
        prev.iter()
            .filter_map(|pr| {
                next.iter()
                    .find(|nr| pr.id == nr.id)
                    .map(|nr| amun_compact::Robot {
                        id: pr.id,
                        p_x: pr.p_x + ratio * (nr.p_x - pr.p_x),
                        p_y: pr.p_y + ratio * (nr.p_y - pr.p_y),
                        phi: pr.phi + ratio * (nr.phi - pr.phi),
                    })
            })
            .collect()
    }

    let ratio = (curr_time as f32 - prev_time as f32) / (next_time as f32 - prev_time as f32);

    amun_compact::WorldState {
        ball: prev.ball.and_then(|pb| {
            next.ball.map(|nb| amun_compact::Ball {
                p_x: pb.p_x + ratio * (nb.p_x - pb.p_x),
                p_y: pb.p_y + ratio * (nb.p_y - pb.p_y),
                p_z: pb
                    .p_z
                    .and_then(|pz| nb.p_z.map(|nz| pz + ratio * (nz - pz))),
            })
        }),
        yellow_robot: interpolate_robots(&prev.yellow_robot, &next.yellow_robot, ratio),
        blue_robot: interpolate_robots(&prev.blue_robot, &next.blue_robot, ratio),
    }
}

fn remap_coordinates(status: &mut amun_compact::Status) {
    if let Some(world_state) = &mut status.world_state {
        if let Some(ball) = &mut world_state.ball {
            ball.p_x *= -1.0;
        }
        for robot in &mut world_state.yellow_robot {
            robot.p_x *= -1.0;
            robot.phi += PI / 2.0;
        }
        for robot in &mut world_state.blue_robot {
            robot.p_x *= -1.0;
            robot.phi += PI / 2.0;
        }
    }

    for vis in &mut status.visualization {
        for part in &mut vis.part {
            match &mut part.geom {
                Some(Geom::Circle(c)) => {
                    c.p_x *= -1.0;
                }
                Some(Geom::Polygon(p)) => {
                    for point in &mut p.point {
                        point.x *= -1.0;
                    }
                }
                Some(Geom::Path(p)) => {
                    for point in &mut p.point {
                        point.x *= -1.0;
                    }
                }
                None => {}
            }
        }
    }
}
