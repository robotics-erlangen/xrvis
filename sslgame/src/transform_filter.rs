use bevy::math::Affine3A;
use bevy::prelude::{Component, Query, Transform};
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tracing::warn;

// TODO: Kalman filter

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

#[derive(Component, Debug)]
pub struct TransformFilter {
    algorithm: TransformFilterAlgorithm,
    /// Mapping between local [`Instant`]s and sample timestamps.
    time_anchor: Option<TimeAnchor>,
}

#[derive(Debug)]
enum TransformFilterAlgorithm {
    Nearest(PacketHistory),
    Linear(PacketHistory),
}

#[derive(Debug, PartialEq)]
struct TimeAnchor {
    local_time: Instant,
    sample_time: Duration,
    is_fallback: bool,
}

impl TransformFilter {
    /// Creates a [`TransformFilter`] with nearest neighbor filtering.
    pub fn new_nearest(history_length: Duration) -> Self {
        Self {
            algorithm: TransformFilterAlgorithm::Nearest(PacketHistory {
                samples: VecDeque::new(),
                history_length,
            }),
            time_anchor: None,
        }
    }

    /// Creates a [`TransformFilter`] with linear filtering.
    pub fn new_linear(history_length: Duration) -> Self {
        Self {
            algorithm: TransformFilterAlgorithm::Linear(PacketHistory {
                samples: VecDeque::new(),
                history_length,
            }),
            time_anchor: None,
        }
    }

    /// Returns the estimated sample timestamp for a given local time, using the minimum observed latency.
    ///
    /// Simpler: If the fastest packet yet arrived at local_time, what would be its timestamp?
    fn estimate_sample_time(&self, local_time: Instant) -> Option<Duration> {
        let anchor = self.time_anchor.as_ref()?;
        if local_time >= anchor.local_time {
            Some(anchor.sample_time + (local_time - anchor.local_time))
        } else {
            Some(
                anchor
                    .sample_time
                    .saturating_sub(anchor.local_time - local_time),
            )
        }
    }

    pub fn sample(&self, local_time: Instant) -> Option<Affine3A> {
        let sample_time = self.estimate_sample_time(local_time)?;
        match &self.algorithm {
            TransformFilterAlgorithm::Nearest(history) => history
                .find_surrounding_samples(sample_time)
                .map(|(older, newer, ratio)| if ratio < 0.5 { older } else { newer }),
            TransformFilterAlgorithm::Linear(history) => history
                .find_surrounding_samples(sample_time)
                .map(|(older, newer, ratio)| {
                    let (s0, r0, t0) = older.to_scale_rotation_translation();
                    let (s1, r1, t1) = newer.to_scale_rotation_translation();

                    let scale = s0.lerp(s1, ratio);
                    let rot = r0.slerp(r1, ratio);
                    let trans = t0.lerp(t1, ratio);

                    Affine3A::from_scale_rotation_translation(scale, rot, trans)
                }),
        }
    }

    /// Ingests a sample into the filter. The timestamp is the duration from an arbitrary but fixed
    /// instant on the host to when the sample was created, used to handle network jitter.
    /// The rx_time is the time at which the sample was received by this application, used to
    /// provide a fallback when no sender timestamp is available. Switching between explicit and
    /// fallback sample_time resets the filter.
    pub fn push_sample(
        &mut self,
        transform: Affine3A,
        sample_time: Option<Duration>,
        local_rx_time: Instant,
    ) -> bool {
        // Reset when switching between explicit and fallback timestamps
        if self
            .time_anchor
            .as_ref()
            .is_some_and(|a| a.is_fallback != sample_time.is_none())
        {
            self.time_anchor = None;
            match &mut self.algorithm {
                TransformFilterAlgorithm::Nearest(hist)
                | TransformFilterAlgorithm::Linear(hist) => {
                    hist.samples.clear();
                }
            }
        }

        let estimated_timestamp = self.estimate_sample_time(local_rx_time);
        let timestamp = sample_time.unwrap_or_else(|| estimated_timestamp.unwrap_or_default());

        match &mut self.algorithm {
            TransformFilterAlgorithm::Nearest(hist) | TransformFilterAlgorithm::Linear(hist) => {
                // Reject out-of-order or duplicate samples to keep history monotonic
                if hist.samples.front().is_some_and(|(t, _)| *t >= timestamp) {
                    warn!("Received out-of-order transform sample, ignoring");
                    return false;
                }

                if estimated_timestamp.is_none_or(|predicted| timestamp > predicted) {
                    self.time_anchor = Some(TimeAnchor {
                        local_time: local_rx_time,
                        sample_time: timestamp,
                        is_fallback: sample_time.is_none(),
                    });
                }

                hist.samples.push_front((timestamp, transform));

                let cutoff_time = timestamp.saturating_sub(hist.history_length);
                while hist.samples.back().is_some_and(|(t, _)| *t < cutoff_time) {
                    hist.samples.pop_back();
                }
                true
            }
        }
    }
}

#[derive(Debug)]
struct PacketHistory {
    /// Sliding window of the last received packets (new samples inserted at the front)
    samples: VecDeque<(Duration, Affine3A)>,
    history_length: Duration,
}

impl PacketHistory {
    /// Returns the previous and next samples, and a 0..1 interpolation factor.
    fn find_surrounding_samples(&self, sample_time: Duration) -> Option<(Affine3A, Affine3A, f32)> {
        if self.samples.is_empty() {
            return None;
        }

        if self.samples.len() == 1 {
            return Some((self.samples[0].1, self.samples[0].1, 0.0));
        }

        // Get surrounding indices
        let mut older_idx = self.samples.len() - 1;
        let mut newer_idx = older_idx - 1;
        for (i, &(t, _)) in self.samples.iter().enumerate() {
            if t <= sample_time {
                older_idx = i.max(1);
                newer_idx = older_idx - 1;
                break;
            }
        }
        // Get surrounding values
        let (older_time, older_mat) = self.samples[older_idx];
        let (newer_time, newer_mat) = self.samples[newer_idx];
        let dt = (newer_time - older_time).as_secs_f32();
        // Don't clamp to allow for extrapolation
        let ratio = if sample_time >= older_time {
            (sample_time - older_time).as_secs_f32() / dt
        } else {
            -((older_time - sample_time).as_secs_f32() / dt)
        };
        Some((older_mat, newer_mat, ratio))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec3;

    fn millis(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    fn pos(x: f32) -> Affine3A {
        Affine3A::from_translation(Vec3::new(x, 0.0, 0.0))
    }

    #[test]
    fn nearest() {
        let local_base = Instant::now();
        let sample_base = Duration::from_micros(0);
        let mut filter = TransformFilter::new_nearest(millis(500));

        assert!(filter.push_sample(pos(0.0), Some(sample_base), local_base));
        assert!(filter.push_sample(
            pos(2.0),
            Some(sample_base + millis(20)),
            local_base + millis(20),
        ));

        // Interpolate down
        let sample_at_9 = filter.sample(local_base + millis(9)).unwrap();
        assert_eq!(sample_at_9.translation.x, 0.0);
        // Interpolate mid (-> up)
        let sample_at_10 = filter.sample(local_base + millis(10)).unwrap();
        assert_eq!(sample_at_10.translation.x, 2.0);
        // Interpolate up
        let sample_at_11 = filter.sample(local_base + millis(11)).unwrap();
        assert_eq!(sample_at_11.translation.x, 2.0);
        // Exact
        let sample_at_20 = filter.sample(local_base + millis(20)).unwrap();
        assert_eq!(sample_at_20.translation.x, 2.0);
        // (Don't) Extrapolate
        let sample_at_40 = filter.sample(local_base + millis(40)).unwrap();
        assert_eq!(sample_at_40.translation.x, 2.0);
    }

    #[test]
    fn linear() {
        let local_base = Instant::now();
        let sample_base = Duration::from_micros(0);
        let mut filter = TransformFilter::new_linear(millis(500));

        assert!(filter.push_sample(pos(0.0), Some(sample_base), local_base));
        assert!(filter.push_sample(
            pos(2.0),
            Some(sample_base + millis(20)),
            local_base + millis(20),
        ));

        // Interpolate
        let sample_at_15 = filter.sample(local_base + millis(15)).unwrap();
        assert!((sample_at_15.translation.x - 1.5).abs() < 1e-4);
        // Exact
        let sample_at_20 = filter.sample(local_base + millis(20)).unwrap();
        assert!((sample_at_20.translation.x - 2.0).abs() < 1e-4);
        // Extrapolate
        let sample_at_30 = filter.sample(local_base + millis(30)).unwrap();
        assert!((sample_at_30.translation.x - 3.0).abs() < 1e-4);
    }

    #[test]
    fn reject_nonmonotonic() {
        let local_base = Instant::now();
        let sample_base = Duration::from_micros(0);
        let mut filter = TransformFilter::new_nearest(millis(500));

        assert!(filter.push_sample(pos(0.0), Some(sample_base), local_base));
        assert!(filter.push_sample(
            pos(2.0),
            Some(sample_base + millis(20)),
            local_base + millis(20),
        ));

        // Duplicate timestamps should be rejected
        assert!(!filter.push_sample(
            pos(99.0),
            Some(sample_base + millis(20)),
            local_base + millis(20),
        ));

        // Out-of-order sample should be rejected
        assert!(!filter.push_sample(
            pos(99.0),
            Some(sample_base + millis(10)),
            local_base + millis(25),
        ));

        // Nearest should still clamp down to the second sample
        let sample_at_30 = filter.sample(local_base + millis(30)).unwrap();
        assert_eq!(sample_at_30.translation.x, 2.0);
    }

    #[test]
    fn prune_history() {
        let local_base = Instant::now();
        let sample_base = Duration::from_micros(0);
        let mut filter = TransformFilter::new_nearest(millis(500));

        assert!(filter.push_sample(pos(0.0), Some(sample_base), local_base));
        assert!(filter.push_sample(
            pos(40.0),
            Some(sample_base + millis(400)),
            local_base + millis(400),
        ));
        // This should evict the first sample at 0.0
        assert!(filter.push_sample(
            pos(60.0),
            Some(sample_base + millis(600)),
            local_base + millis(600),
        ));

        // Nearest should now clamp up to the second sample
        let sample_at_0 = filter.sample(local_base).unwrap();
        assert_eq!(sample_at_0.translation.x, 40.0);
    }

    #[test]
    fn adjust_time_anchor() {
        let local_base = Instant::now();
        let sample_base = Duration::from_micros(0);
        let mut filter = TransformFilter::new_linear(millis(500));

        // First two packets are 20ms late
        assert!(filter.push_sample(pos(0.0), Some(sample_base), local_base + millis(20)));
        assert!(filter.push_sample(
            pos(2.0),
            Some(sample_base + millis(20)),
            local_base + millis(40),
        ));
        // local_time 20 should map exactly to the first sample with timestamp 0
        let sample_offset = filter.sample(local_base + millis(20)).unwrap();
        assert!((sample_offset.translation.x - 0.0).abs() < 1e-4);

        // Third packet arrives on-time
        assert!(filter.push_sample(
            pos(6.0),
            Some(sample_base + millis(60)),
            local_base + millis(60),
        ));
        // The new on-time packet also shifts the sampling offset for earlier
        let sample_shift = filter.sample(local_base + millis(20)).unwrap();
        assert!((sample_shift.translation.x - 2.0).abs() < 1e-4);

        // Fourth packet arrives 20ms late again
        assert!(filter.push_sample(
            pos(8.0),
            Some(sample_base + millis(80)),
            local_base + millis(100),
        ));
        // The offset should never shift back, so it should still be at 0
        let sample_noshift = filter.sample(local_base + millis(20)).unwrap();
        assert!((sample_noshift.translation.x - 2.0).abs() < 1e-4);
    }

    #[test]
    fn reset_on_timestamp_switch() {
        for new_filter in [
            TransformFilter::new_linear as fn(Duration) -> TransformFilter,
            TransformFilter::new_nearest,
        ] {
            let local_base = Instant::now();
            let sample_base = Duration::from_micros(0);
            let mut filter = new_filter(millis(500));

            // Push samples without explicit timestamp
            assert!(filter.push_sample(pos(0.0), None, local_base));
            assert!(filter.push_sample(pos(4.0), None, local_base + millis(40)));

            assert!(filter.time_anchor.as_ref().is_some_and(|a| a.is_fallback));
            // Sampling should work with fallback timestamps
            let sample_fallback = filter.sample(local_base + millis(40)).unwrap();
            assert!((sample_fallback.translation.x - 4.0).abs() < 1e-4);

            // Push sample with explicit timestamp
            assert!(filter.push_sample(pos(8.0), Some(sample_base), local_base + millis(50)));

            assert_eq!(
                filter.time_anchor,
                Some(TimeAnchor {
                    local_time: local_base + millis(50),
                    sample_time: sample_base,
                    is_fallback: false
                })
            );
            match &filter.algorithm {
                TransformFilterAlgorithm::Nearest(history)
                | TransformFilterAlgorithm::Linear(history) => assert_eq!(history.samples.len(), 1),
            };
            // Previous samples should have been discarded
            assert_eq!(filter.sample(local_base).unwrap().translation.x, 8.0);
        }
    }
}
