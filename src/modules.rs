//! Immutable stem and canopy modules for GPU expansion at intermediate LOD.
//! Original source IDs and normalization do not depend on the camera.

use crate::{
    Bounds, Error, Plant, ScreenLod, SourceFrame, Stem,
    geometry::{Result, dot},
};

/// One source-space branch or spatially aggregated foliage group.
#[derive(Clone, Debug, PartialEq)]
pub enum Module {
    Stem {
        source_index: u32,
        start: [f32; 3],
        end: [f32; 3],
        base_radius: f32,
        tip_radius: f32,
        color: [f32; 4],
    },
    Canopy {
        source_index: u32,
        /// Complete original membership for source-aware alpha/material baking.
        source_ids: Vec<u32>,
        /// A conservative spatial envelope, not necessarily a visual alpha mask.
        bounds: Bounds,
        color: [f32; 4],
    },
}

impl Module {
    #[must_use = "Retain stable original module identity for visual variation"]
    pub const fn source_index(&self) -> u32 {
        match self {
            Self::Stem { source_index, .. } | Self::Canopy { source_index, .. } => *source_index,
        }
    }

    #[must_use = "Preserve conservative module geometry bounds"]
    pub fn bounds(&self) -> Bounds {
        match self {
            Self::Stem {
                start,
                end,
                base_radius,
                tip_radius,
                ..
            } => {
                let radius = base_radius.max(*tip_radius);
                let mut bounds = Bounds::around(*start, [radius; 3]);
                bounds.union(Bounds::around(*end, [radius; 3]));
                bounds
            }
            Self::Canopy { bounds, .. } => *bounds,
        }
    }
}

/// A source-variant representation at one immutable projected-size LOD.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleTier {
    pub modules: Vec<Module>,
    pub source_frame: SourceFrame,
    pub source_bounds: Bounds,
    pub required_culling_radius: f32,
}

impl ModuleTier {
    #[must_use = "Use the count to account for GPU geometry"]
    pub fn stem_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|m| matches!(m, Module::Stem { .. }))
            .count()
    }

    #[must_use = "Use the count to account for GPU geometry"]
    pub fn canopy_count(&self) -> usize {
        self.modules.len() - self.stem_count()
    }
}

/// Share the exact stem visibility decision between triangle and module LOD.
pub fn select_stem(stem: &Stem, scale: f32, error: f32) -> bool {
    stem.parent.is_none() || stem.base_radius.max(stem.tip_radius) * scale >= error
}

impl Plant {
    /// Produce a deterministic module tier, without constructing any triangles.
    /// Spatial foliage groups refine the same hierarchy as `foliage_clusters`;
    /// the graph never mutates when the camera changes. Each source component
    /// is assigned exactly once, with full original component IDs retained.
    ///
    /// # Errors
    /// Rejects an invalid target or wood color, index overflow, and malformed
    /// normalization envelopes.
    pub fn module_tier(&self, target: ScreenLod, wood_color: [f32; 4]) -> Result<ModuleTier> {
        ScreenLod::new(
            target.projected_diameter,
            target.stem_chord_error,
            target.foliage_patch_diameter,
        )?;
        if target.max_clusters == 0
            || !wood_color[..3].iter().all(|c| c.is_finite() && *c >= 0.0)
            || !wood_color[3].is_finite()
            || !(0.0..=1.0).contains(&wood_color[3])
        {
            return Err(Error::Invalid("invalid module tier parameters"));
        }
        let scale = target.projected_diameter / self.source_frame().diameter();
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::Invalid("unrepresentable projected module scale"));
        }
        let mut modules = Vec::new();
        for (index, stem) in self.stems().iter().enumerate() {
            if select_stem(stem, scale, target.stem_chord_error) {
                modules.push(Module::Stem {
                    source_index: u32::try_from(index).map_err(|_| Error::TooLarge)?,
                    start: stem.start,
                    end: stem.end,
                    base_radius: stem.base_radius,
                    tip_radius: stem.tip_radius,
                    color: wood_color,
                });
            }
        }
        for cluster in self.foliage_clusters(target)? {
            let source_index = cluster
                .indices
                .iter()
                .copied()
                .min()
                .ok_or(Error::Invalid("empty canopy group"))?;
            modules.push(Module::Canopy {
                source_index,
                source_ids: cluster.indices,
                bounds: cluster.bounds,
                color: cluster.color,
            });
        }
        let frame = self.source_frame();
        let required_culling_radius = self.required_culling_radius();
        if !required_culling_radius.is_finite() || required_culling_radius < frame.radius {
            return Err(Error::Invalid("invalid module culling bound"));
        }
        for module in &modules {
            let b = module.bounds();
            let farthest = std::array::from_fn::<_, 3, _>(|i| {
                (b.max[i] - frame.center[i])
                    .abs()
                    .max((b.min[i] - frame.center[i]).abs())
            });
            if !b.valid() || !dot(farthest, farthest).is_finite() {
                return Err(Error::Invalid("invalid module extent"));
            }
        }
        Ok(ModuleTier {
            modules,
            source_frame: frame,
            source_bounds: self.bounds(),
            required_culling_radius,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Foliage, Species};

    fn memberships(tier: &ModuleTier) -> Vec<Vec<u32>> {
        tier.modules
            .iter()
            .filter_map(|m| match m {
                Module::Canopy { source_ids, .. } => Some(source_ids.clone()),
                Module::Stem { .. } => None,
            })
            .collect()
    }

    #[test]
    fn all_source_foliage_is_owned_once_and_tiers_are_nested() -> Result<()> {
        let species = Species::default();
        for seed in 0..20 {
            let plant = Plant::generate(&species, seed)?;
            let mut previous: Vec<Vec<u32>> = Vec::new();
            for maximum in [8, 24, 64] {
                let target = ScreenLod::new(256.0, 0.75, 0.01)?.with_cluster_budget(maximum)?;
                let tier = plant.module_tier(target, species.wood_color)?;
                assert_eq!(tier.source_frame, plant.source_frame());
                assert_eq!(tier.source_bounds, plant.bounds());
                assert_eq!(
                    tier.required_culling_radius,
                    plant.required_culling_radius()
                );
                let members = memberships(&tier);
                let mut all = members.iter().flatten().copied().collect::<Vec<_>>();
                all.sort_unstable();
                assert_eq!(
                    all,
                    (0..u32::try_from(plant.foliage().len()).map_err(|_| Error::TooLarge)?)
                        .collect::<Vec<_>>()
                );
                assert!(members.len() <= maximum);
                for source in &members {
                    if !previous.is_empty() {
                        assert!(
                            previous
                                .iter()
                                .any(|parent| source.iter().all(|id| parent.contains(id)))
                        );
                    }
                }
                previous = members;
            }
        }
        Ok(())
    }

    #[test]
    fn displaced_fitted_source_frame_and_module_bounds_stay_separate() -> Result<()> {
        let tree = Plant::from_parts(
            vec![Stem {
                start: [0.0, -5.0, 0.0],
                end: [0.0, 5.0, 0.0],
                base_radius: 0.3,
                tip_radius: 0.08,
                parent: None,
            }],
            vec![Foliage {
                center: [6.0, 3.0, 0.0],
                radii: [1.0, 0.8, 1.5],
                color: [0.2, 0.6, 0.1, 1.0],
                stem: 0,
            }],
        )?
        .with_source_frame(SourceFrame::new([0.0; 3], 5.2)?)?;
        let tier = tree.module_tier(ScreenLod::new(50.0, 0.75, 1.0)?, [0.3, 0.2, 0.1, 1.0])?;
        assert!(tier.required_culling_radius > tier.source_frame.radius);
        assert_eq!(tier.canopy_count(), 1);
        assert_eq!(tier.stem_count(), 1);
        assert_eq!(tier.source_frame, tree.source_frame());
        assert!(tier.modules.iter().all(|m| m.bounds().valid()));
        Ok(())
    }

    #[test]
    fn invalid_requests_rejected_and_module_order_deterministic() -> Result<()> {
        let species = Species::default();
        let plant = Plant::generate(&species, 1234)?;
        let target = ScreenLod::new(128.0, 0.75, 0.01)?.with_cluster_budget(24)?;
        assert_eq!(
            plant.module_tier(target, species.wood_color)?,
            plant.module_tier(target, species.wood_color)?
        );
        assert!(plant.module_tier(target, [f32::NAN; 4]).is_err());
        let mut invalid = target;
        invalid.max_clusters = 0;
        assert!(plant.module_tier(invalid, species.wood_color).is_err());
        let other = Plant::generate(&species, 1235)?;
        assert_ne!(
            plant.module_tier(target, species.wood_color)?,
            other.module_tier(target, species.wood_color)?
        );
        Ok(())
    }
}
