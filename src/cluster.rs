//! Deterministic spatial clustering of original foliage components.
//!
//! A branch-parent hierarchy is not useful for screen-error LOD: the root can
//! jump from a single giant octahedron to hundreds of unrelated leaf cards.
//! The spatial hierarchy refines the largest error first within a hard budget.

use crate::{Bounds, Error, Plant, ScreenLod, geometry::Result};

/// Conservative proxy with exact original component identities.
/// Indices refer to plant foliage and source card arrays, not GPU vertices.
#[derive(Clone, Debug, PartialEq)]
pub struct FoliageCluster {
    pub bounds: Bounds,
    pub color: [f32; 4],
    pub indices: Vec<u32>,
}

fn leaf_bounds(plant: &Plant, id: usize) -> Bounds {
    let leaf = plant.foliage()[id];
    Bounds::around(leaf.center, leaf.radii)
}

fn aggregate_bounds(plant: &Plant, ids: &[usize]) -> Bounds {
    let mut bounds = leaf_bounds(plant, ids[0]);
    for &id in &ids[1..] {
        bounds.union(leaf_bounds(plant, id));
    }
    bounds
}

fn split_axis(plant: &Plant, ids: &[usize]) -> usize {
    let mut low = [f32::INFINITY; 3];
    let mut high = [f32::NEG_INFINITY; 3];
    for &id in ids {
        let center = plant.foliage()[id].center;
        for axis in 0..3 {
            low[axis] = low[axis].min(center[axis]);
            high[axis] = high[axis].max(center[axis]);
        }
    }
    (0..3)
        .max_by(|&a, &b| (high[a] - low[a]).total_cmp(&(high[b] - low[b])))
        .unwrap_or(1)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "an average of finite source f32 colors remains in the input f32 range"
)]
fn average_color(plant: &Plant, ids: &[usize]) -> [f32; 4] {
    let mut sum = [0.0_f64; 4];
    for &id in ids {
        for (result, channel) in sum.iter_mut().zip(plant.foliage()[id].color) {
            *result += f64::from(channel);
        }
    }
    sum.map(|value| (value / ids.len() as f64) as f32)
}

impl Plant {
    /// Select source-card clusters using projected error and a hard proxy
    /// budget. Every original component belongs to exactly one output group.
    ///
    /// The hierarchy is deterministic: increasing detail refines the same
    /// clusters. Selected groups expose original card indices so a renderer
    /// can bake proper textured/alpha-filtered directional cluster impostors.
    ///
    /// # Errors
    /// Rejects invalid pixel targets, zero budgets, or index overflow.
    pub fn foliage_clusters(&self, target: ScreenLod) -> Result<Vec<FoliageCluster>> {
        ScreenLod::new(
            target.projected_diameter,
            target.stem_chord_error,
            target.foliage_patch_diameter,
        )?;
        if target.max_clusters == 0 {
            return Err(Error::Invalid("zero foliage cluster budget"));
        }
        if self.foliage().is_empty() {
            return Ok(Vec::new());
        }
        let threshold = target.foliage_patch_diameter * self.source_frame().diameter()
            / target.projected_diameter;
        if !threshold.is_finite() || threshold <= 0.0 {
            return Err(Error::Invalid("invalid projected foliage scale"));
        }
        let ids: Vec<usize> = (0..self.foliage().len()).collect();
        let bounds = aggregate_bounds(self, &ids);
        let mut frontier = vec![(ids, bounds)];

        while frontier.len() < target.max_clusters {
            let best = frontier
                .iter()
                .enumerate()
                .filter(|(_, (ids, bounds))| ids.len() > 1 && bounds.diameter() > threshold)
                .max_by(|(_, left), (_, right)| left.1.diameter().total_cmp(&right.1.diameter()))
                .map(|(index, _)| index);
            let Some(best) = best else {
                break;
            };
            let (mut ids, _) = frontier.swap_remove(best);
            let axis = split_axis(self, &ids);
            ids.sort_unstable_by(|&a, &b| {
                self.foliage()[a].center[axis]
                    .total_cmp(&self.foliage()[b].center[axis])
                    .then_with(|| a.cmp(&b))
            });
            let right = ids.split_off(ids.len() / 2);
            let left_bounds = aggregate_bounds(self, &ids);
            let right_bounds = aggregate_bounds(self, &right);
            frontier.push((ids, left_bounds));
            frontier.push((right, right_bounds));
        }

        frontier.sort_unstable_by_key(|(ids, _)| ids.iter().min().copied());
        frontier
            .into_iter()
            .map(|(ids, bounds)| {
                Ok(FoliageCluster {
                    bounds,
                    color: average_color(self, &ids),
                    indices: ids
                        .into_iter()
                        .map(|id| u32::try_from(id).map_err(|_| Error::TooLarge))
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Foliage, Stem};

    #[test]
    fn refinement_never_drops_original_cards_or_exceeds_budget() -> Result<()> {
        let plant = Plant::from_parts(
            vec![Stem {
                start: [0.0, 0.0, 0.0],
                end: [0.0, 10.0, 0.0],
                base_radius: 0.2,
                tip_radius: 0.1,
                parent: None,
            }],
            (0_u16..64)
                .map(|i| Foliage {
                    center: [f32::from(i % 8), 5.0, f32::from(i / 8)],
                    radii: [0.45, 0.7, 0.45],
                    color: [0.1, 0.4, 0.2, 1.0],
                    stem: 0,
                })
                .collect(),
        )?;
        let mut previous = 0;
        for projected in [2.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0] {
            let target = ScreenLod::new(projected, 0.75, 12.0)?.with_cluster_budget(12)?;
            let a = plant.foliage_clusters(target)?;
            let b = plant.foliage_clusters(target)?;
            assert_eq!(a, b);
            assert!(a.len() >= previous && a.len() <= 12);
            let mut found = a
                .iter()
                .flat_map(|group| group.indices.iter().copied())
                .collect::<Vec<_>>();
            found.sort_unstable();
            assert_eq!(found, (0_u32..64).collect::<Vec<_>>());
            previous = a.len();
        }
        Ok(())
    }
}
