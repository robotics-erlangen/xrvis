use bevy::math::Affine3A;
use bevy::prelude::{Component, Query, Transform};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tracing::warn;

// TODO: Kalman filter
// TODO: Dynamic sample time offset to keep a specified buffer from sample to the latest measurement
// TODO: Transmission delay compensation (don't naively ingest measurements at rx_time)

pub(crate) fn apply_filtered_transform(
    mut q_transforms: Query<(&TransformFilter, &mut Transform)>,
) {
    let now = Instant::now();
    for (filter, mut transform) in q_transforms.iter_mut() {
        if let Some(sample) = filter.sample(now) {
            let (s, r, t) = sample.to_scale_rotation_translation();
            *transform = transform.with_scale(s).with_rotation(r).with_translation(t);
        }
    }
}

#[allow(private_interfaces)]
#[derive(Component, Debug)]
pub enum TransformFilter {
    Nearest(PacketHistory),
    Linear(PacketHistory),
}

impl TransformFilter {
    pub fn new_history(history_length: Duration, interpolate: bool) -> Self {
        let history = PacketHistory {
            history: VecDeque::new(),
            history_length,
        };
        if interpolate {
            Self::Linear(history)
        } else {
            Self::Nearest(history)
        }
    }

    pub fn sample(&self, time: Instant) -> Option<Affine3A> {
        match self {
            TransformFilter::Nearest(history) => history.find_surrounding_samples(time).map(
                |(older, newer, ratio)| {
                    if ratio < 0.5 { older } else { newer }
                },
            ),
            TransformFilter::Linear(history) => {
                history
                    .find_surrounding_samples(time)
                    .map(|(older, newer, ratio)| {
                        let (s0, r0, t0) = older.to_scale_rotation_translation();
                        let (s1, r1, t1) = newer.to_scale_rotation_translation();

                        let scale = s0.lerp(s1, ratio);
                        let rot = r0.slerp(r1, ratio);
                        let trans = t0.lerp(t1, ratio);

                        Affine3A::from_scale_rotation_translation(scale, rot, trans)
                    })
            }
        }
    }

    pub fn push_sample(&mut self, transform: Affine3A, time: Instant) {
        match self {
            Self::Nearest(hist) | Self::Linear(hist) => {
                // Non-monotonic input time -> reset history to keep it sorted
                if hist.history.front().is_some_and(|(t, _)| *t > time) {
                    warn!("Received out-of-order transform sample, clearing history");
                    hist.history.clear();
                }

                hist.history.push_front((time, transform));

                let cutoff_time = Instant::now() - hist.history_length;
                while hist.history.back().is_some_and(|(t, _)| *t < cutoff_time) {
                    hist.history.pop_back();
                }
            }
        }
    }
}

#[derive(Debug)]
struct PacketHistory {
    /// Sliding window of the last received packets (new samples inserted at the front)
    history: VecDeque<(Instant, Affine3A)>,
    history_length: Duration,
}

impl PacketHistory {
    fn find_surrounding_samples(&self, sample_time: Instant) -> Option<(Affine3A, Affine3A, f32)> {
        if self.history.is_empty() {
            return None;
        }

        if self.history.len() == 1 {
            return Some((self.history[0].1, self.history[0].1, 0.0));
        }

        // Get surrounding indices
        let mut older_idx = 0;
        let mut newer_idx = 0;
        for (i, &(t, _)) in self.history.iter().enumerate() {
            if t <= sample_time {
                older_idx = i.max(1);
                newer_idx = older_idx - 1;
                break;
            }
        }
        // Get surrounding values
        let (older_time, older_mat) = self.history[older_idx];
        let (newer_time, newer_mat) = self.history[newer_idx];
        let dt = newer_time.duration_since(older_time).as_secs_f32();
        // Don't clamp to allow for extrapolation
        let ratio = sample_time.duration_since(older_time).as_secs_f32() / dt;
        Some((older_mat, newer_mat, ratio))
    }
}
