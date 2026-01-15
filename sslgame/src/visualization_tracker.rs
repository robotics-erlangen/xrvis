use crate::proto::remote::vis_part::Geom;
use crate::proto::remote::{Visualization, VisualizationUpdate};
use bevy::prelude::Component;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Component, Debug, Default)]
pub struct VisualizationTracker {
    history: VecDeque<VisualizationUpdate>,
}

impl VisualizationTracker {
    // TODO: Filter in case the host doesn't properly handle filters server-side
    /// Collects all updated visualization since the last call
    /// and some information about which groups are affected.
    /// (group_count, updated_groups, new_visualizations)
    pub fn visualization_updates(&mut self) -> (u32, HashSet<u32>, Vec<Visualization>) {
        if self.history.is_empty() {
            return Default::default();
        }

        let group_count = self
            .history
            .front()
            .and_then(|v| v.visualization_group)
            .map(|g| g.group_count)
            .unwrap_or(1);

        // Save the set of already collected sources for each group
        let mut group_sources: HashMap<u32, HashSet<u32>> = HashMap::new();
        let mut visualizations = Vec::new();

        self.history
            .iter()
            .map(|v| (v.visualization_group.unwrap(), &v.visualization_set))
            .for_each(|(group, vis_sets)| {
                let seen_sources = group_sources.entry(group.group).or_default();

                for vis_set in vis_sets {
                    if vis_set
                        .source
                        .is_some_and(|source| seen_sources.contains(&source))
                    {
                        // Already collected this source from this group
                        continue;
                    } else if let Some(source) = vis_set.source {
                        // New source for this group
                        seen_sources.insert(source);
                    }

                    for vis in &vis_set.visualization {
                        visualizations.push(vis.clone());
                    }
                }
            });

        // Clear the history so that each update is only returned once
        self.history.clear();

        (
            group_count,
            group_sources.keys().copied().collect(),
            visualizations,
        )
    }

    pub fn push_update(&mut self, mut new_update: VisualizationUpdate) {
        remap_visualizations(&mut new_update);

        let new_group_count = new_update
            .visualization_group
            .map(|g| g.group_count)
            .unwrap_or(1);

        self.history.push_front(new_update);

        // Truncate so that each group in the current group range is contained at least once
        let mut seen_groups = HashSet::new();
        let mut truncate_at = None;

        for (i, update) in self.history.iter().enumerate() {
            if let Some(group) = update.visualization_group {
                if new_group_count == group.group_count {
                    seen_groups.insert(group.group);
                } else {
                    truncate_at = Some(i);
                    break;
                }
            }

            if seen_groups.len() >= new_group_count as usize {
                truncate_at = Some(i + 1);
                break;
            }
        }

        if let Some(index) = truncate_at {
            self.history.truncate(index);
        }
    }
}

/// Converts from the vision coordinate system (right-handed, z up, x towards blue goal, +x forward)
/// to bevy's coordinate system (right-handed, y up, x towards blue goal, -z forward) with y and z swapped
fn remap_visualizations(vis_update: &mut VisualizationUpdate) {
    for vis in vis_update
        .visualization_set
        .iter_mut()
        .flat_map(|set| &mut set.visualization)
    {
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
