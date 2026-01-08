use crate::proto::remote::{Ball, Robot, WorldState};
use bevy::prelude::*;
use std::collections::VecDeque;
use std::f32::consts::PI;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::atomic::{AtomicI64, AtomicU32};
use std::time::{Duration, Instant};

// TODO: Replace all of this with a kalman filter

// TODO: Make this variable based on connection instability
const TARGET_BUFFER_TIME: Duration = Duration::from_millis(10);

#[derive(Component, Debug)]
pub struct WorldStateFilter {
    /// Sliding window of the past received packets with their timestamp relative to time_reference.
    history: VecDeque<(u64, WorldState)>,

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

impl Default for WorldStateFilter {
    fn default() -> Self {
        Self {
            history: VecDeque::new(),
            time_reference: Instant::now(),
            time_offset: None,
            health_tracking_period: Duration::from_secs(10),
            buffer_health_tracker: None,
        }
    }
}

impl WorldStateFilter {
    pub fn current_world_state(&self, filter: bool) -> WorldState {
        if !filter {
            return self
                .history
                .front()
                .map(|(_, state)| state.clone())
                .unwrap_or_default();
        }

        let curr_timestamp = self.time_reference.elapsed().as_micros() as u64;

        // Find relevant packets
        let prev_idx = self
            .history
            .iter()
            .enumerate()
            .find(|(_, (time, _))| time < &curr_timestamp)
            .map(|(idx, _)| idx)
            .unwrap_or(usize::MAX); // Impossible value to also invalidate next_idx
        let next_idx = prev_idx - 1;
        let (prev, next) = (self.history.get(prev_idx), self.history.get(next_idx));

        match (prev, next) {
            // Normal case: Two packets to interpolate between are available
            (Some((prev_time, prev)), Some((next_time, next))) => {
                if let Some(buffer_health_tracker) = &self.buffer_health_tracker {
                    let (latest_world_timestamp, _) = self.history.front().unwrap();
                    buffer_health_tracker.min_buffer_health.fetch_min(
                        *latest_world_timestamp as i64 - curr_timestamp as i64,
                        SeqCst,
                    );
                }

                interpolate_world_state(curr_timestamp, *prev_time, prev, *next_time, next)
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
                let prev_prev = self.history.get(1);

                if let Some((prev_prev_time, prev_prev)) = prev_prev {
                    // Two past packets available -> extrapolate
                    // TODO: Fix extrapolation
                    interpolate_world_state(
                        curr_timestamp,
                        *prev_prev_time,
                        prev_prev,
                        *prev_time,
                        prev,
                    )
                } else {
                    // Only one packet available
                    prev.clone()
                }
            }
            (None, Some(_next)) => {
                unreachable!("Next can only be derived from an existing prev value")
            }
            (None, None) => WorldState::default(),
        }
    }

    pub fn push_packet(&mut self, mut packet: WorldState) {
        let now = Instant::now();
        let current_timestamp = (now - self.time_reference).as_micros() as u64;

        // Set initial offset
        if self.time_offset.is_none() {
            self.time_offset = Some(current_timestamp as i64 - packet.timestamp.unwrap() as i64);
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

        remap_world_state(&mut packet);

        // Insert the new packet into buffer, ordered by its converted local timestamp
        let new_timestamp = (packet.timestamp.unwrap() as i64 + self.time_offset.unwrap()) as u64;
        let insert_index = self
            .history
            .iter()
            .take_while(|(timestamp, _)| *timestamp > new_timestamp)
            .count();
        self.history.insert(insert_index, (new_timestamp, packet));

        // Remove old packets from the buffer
        self.history.truncate(
            self.history
                .iter()
                .take_while(|(timestamp, _)| current_timestamp < timestamp + 1000000)
                .count(),
        );
    }
}

fn interpolate_world_state(
    curr_time: u64,
    prev_time: u64,
    prev: &WorldState,
    next_time: u64,
    next: &WorldState,
) -> WorldState {
    fn interpolate_robots(prev: &[Robot], next: &[Robot], ratio: f32) -> Vec<Robot> {
        prev.iter()
            .filter_map(|pr| {
                next.iter().find(|nr| pr.id == nr.id).map(|nr| Robot {
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
    WorldState {
        timestamp: Some(
            prev.timestamp.unwrap()
                + (ratio * (next.timestamp.unwrap() - prev.timestamp.unwrap()) as f32) as u64,
        ),
        ball: if prev.ball.len() == 1 && next.ball.len() == 1 {
            let pb = prev.ball[0];
            let nb = next.ball[0];
            vec![Ball {
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
fn remap_world_state(world_state: &mut WorldState) {
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
