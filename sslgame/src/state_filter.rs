use crate::proto::status_streaming;
use crate::proto::status_streaming::vis_part::Geom;
use crate::{FieldGeometry, GameState};
use bevy::prelude::*;
use std::collections::{HashSet, VecDeque};
use std::f32::consts::PI;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{AtomicI64, AtomicU32};
use std::time::{Duration, Instant};

// TODO: Fully transform into internal representation
// TODO: Continuous offset adjustment based on actual control algorithms

// TODO: Make this variable based on connection instability
const TARGET_BUFFER_TIME: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub struct StateFilter {
    /// Sliding window of the past received packets with their timestamp relative to time_reference.
    packet_history: VecDeque<(u64, status_streaming::Status)>,

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

#[derive(Debug)]
struct BufferHealthTracker {
    min_buffer_health: AtomicI64,
    stutter_count: AtomicU32,
    scheduled_time: Instant,
}

impl Default for StateFilter {
    fn default() -> Self {
        Self {
            packet_history: VecDeque::new(),
            time_reference: Instant::now(),
            time_offset: None,
            health_tracking_period: Duration::from_secs(10),
            buffer_health_tracker: None,
        }
    }
}

impl StateFilter {
    pub fn current_game_state(&self) -> GameState {
        let latest = self.packet_history.front().map(|s| &s.1);

        latest.map_or(GameState::default(), |s| GameState {
            yellow_team: s.yellow_team.clone().map_or("".to_string(), |t| t.name),
            blue_team: s.blue_team.clone().map_or("".to_string(), |t| t.name),
        })
    }

    pub fn current_field_geometry(&self) -> FieldGeometry {
        let latest = self.packet_history.iter().find_map(|p| p.1.field_geometry);
        latest
            .and_then(|g| {
                let geom = FieldGeometry {
                    play_area_size: Vec2::new(g.field_size_x, g.field_size_y),
                    boundary_width: g.boundary_width.unwrap_or(0.0),
                    defense_size: Vec2::new(g.defense_size_x, g.defense_size_y),
                    goal_width: g.goal_width,
                };
                if geom.play_area_size == Vec2::new(0.0, 0.0)
                    || geom.defense_size == Vec2::new(0.0, 0.0)
                    || geom.goal_width == 0.0
                {
                    None
                } else {
                    Some(geom)
                }
            })
            .unwrap_or_default()
    }

    pub fn current_world_state(&self, filter: bool) -> status_streaming::WorldState {
        if !filter {
            return self
                .packet_history
                .iter()
                .find_map(|p| p.1.world_state.as_ref())
                .cloned()
                .unwrap_or_default();
        }

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
            // Normal case: Two packets to interpolate between are available
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
            // Buffer too small: Already past newest available packet
            (Some((prev_time, prev)), None) => {
                // Record stutter
                if let Some(buffer_health_tracker) = &self.buffer_health_tracker {
                    buffer_health_tracker.stutter_count.fetch_add(1, SeqCst);
                    buffer_health_tracker
                        .min_buffer_health
                        .fetch_min(*prev_time as i64 - curr_timestamp as i64, SeqCst);
                }

                // Get the packet before prev (= the second-newest packet)
                let prev_prev = self
                    .packet_history
                    .iter()
                    .filter(|(_, p)| p.world_state.is_some())
                    .nth(1);

                if let Some((prev_prev_time, prev_prev)) = prev_prev {
                    // Two past packets available -> extrapolate
                    // TODO: Fix extrapolation
                    interpolate_world_state(
                        curr_timestamp,
                        *prev_prev_time,
                        prev_prev.world_state.as_ref().unwrap(),
                        *prev_time,
                        prev.world_state.as_ref().unwrap(),
                    )
                } else {
                    // Only one packet available
                    prev.world_state.clone().unwrap()
                }
            }
            (None, Some(_next)) => unreachable!("Next can only be set with from a prev value"),
            (None, None) => status_streaming::WorldState::default(),
        }
    }

    pub fn visualization_updates(
        &self,
    ) -> (u32, HashSet<u32>, Vec<status_streaming::Visualization>) {
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

    pub fn push_packet(&mut self, mut status: status_streaming::Status) {
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
                // Min one measurement
                && min_time != i64::MAX
                // Too much delay or stutters
                && (min_time > TARGET_BUFFER_TIME.as_micros() as i64 * 2
                    || stutter_count > (self.health_tracking_period.as_secs() / 5) as u32)
            {
                self.time_offset = self.time_offset.map(|old_offset| {
                    old_offset + (TARGET_BUFFER_TIME.as_micros() as i64 - min_time)
                }); // Set offset so that there would have been 5ms extra buffer
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
    prev: &status_streaming::WorldState,
    next_time: u64,
    next: &status_streaming::WorldState,
) -> status_streaming::WorldState {
    fn interpolate_robots(
        prev: &[status_streaming::Robot],
        next: &[status_streaming::Robot],
        ratio: f32,
    ) -> Vec<status_streaming::Robot> {
        prev.iter()
            .filter_map(|pr| {
                next.iter()
                    .find(|nr| pr.id == nr.id)
                    .map(|nr| status_streaming::Robot {
                        id: pr.id,
                        p_x: pr.p_x + ratio * (nr.p_x - pr.p_x),
                        p_y: pr.p_y + ratio * (nr.p_y - pr.p_y),
                        phi: pr.phi + ratio * ((nr.phi - pr.phi + PI).rem_euclid(2.0 * PI) - PI),
                    })
            })
            .collect()
    }

    let ratio = (curr_time as f32 - prev_time as f32) / (next_time as f32 - prev_time as f32);

    // TODO: Multi-Ball interpolation and tracking across frames
    status_streaming::WorldState {
        ball: if prev.ball.len() == 1 && next.ball.len() == 1 {
            let pb = prev.ball[0];
            let nb = next.ball[0];
            vec![status_streaming::Ball {
                p_x: pb.p_x + ratio * (nb.p_x - pb.p_x),
                p_y: pb.p_y + ratio * (nb.p_y - pb.p_y),
                p_z: pb
                    .p_z
                    .and_then(|pz| nb.p_z.map(|nz| pz + ratio * (nz - pz))),
            }]
        } else {
            next.ball.clone()
        },
        yellow_robot: interpolate_robots(&prev.yellow_robot, &next.yellow_robot, ratio),
        blue_robot: interpolate_robots(&prev.blue_robot, &next.blue_robot, ratio),
    }
}

/// Converts from the vision coordinate system (right-handed, z up, x towards blue goal, +x forward)
/// to bevy's coordinate system (right-handed, y up, x towards blue goal, -z forward) with y and z swapped
fn remap_coordinates(status: &mut status_streaming::Status) {
    if let Some(world_state) = &mut status.world_state {
        for ball in &mut world_state.ball {
            ball.p_y = -ball.p_y;
        }
        for robot in &mut world_state.yellow_robot {
            robot.p_y = -robot.p_y;
            robot.phi -= PI / 2.0;
        }
        for robot in &mut world_state.blue_robot {
            robot.p_y = -robot.p_y;
            robot.phi -= PI / 2.0;
        }
    }

    for vis in &mut status.visualization {
        for part in &mut vis.part {
            match &mut part.geom {
                Some(Geom::Circle(c)) => {
                    c.p_y = -c.p_y;
                }
                Some(Geom::Polygon(p)) => {
                    for point in &mut p.point {
                        point.y = -point.y;
                    }
                }
                Some(Geom::Path(p)) => {
                    for point in &mut p.point {
                        point.y = -point.y;
                    }
                }
                None => {}
            }
        }
    }
}
