//! Decurrent broadleaf crowns with ascending scaffold limbs and layered,
//! independently seeded terminal lobes. The immutable graph feeds the
//! same renderer-independent geometry and module LOD hierarchy.

use crate::{
    Foliage, Plant, Stem,
    generation::{Random, valid_color},
    geometry::{Error, Result, add, mul, sub},
};

/// A broadleaf crown with ascending scaffold limbs. Lengths are in metres;
/// crown base is the fraction of trunk height before the first scaffold limb.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Broadleaf {
    pub height: f32,
    pub trunk_radius: f32,
    pub crown_radius: f32,
    pub crown_base: f32,
    pub scaffold_limbs: u8,
    pub forks_per_limb: u8,
    pub leaf_color: [f32; 4],
}

impl Default for Broadleaf {
    fn default() -> Self {
        Self {
            height: 16.0,
            trunk_radius: 0.65,
            crown_radius: 9.0,
            crown_base: 0.24,
            scaffold_limbs: 8,
            forks_per_limb: 4,
            leaf_color: [0.105, 0.19, 0.045, 1.0],
        }
    }
}

impl Broadleaf {
    /// Generate an irregular, connected broadleaf skeleton and layered crown.
    /// The central axis, branch attachment and crown envelope never depend
    /// on the requested mesh or impostor LOD.
    ///
    /// # Errors
    /// Rejects invalid or unrepresentable plant parameters.
    pub fn generate(self, seed: u64) -> Result<Plant> {
        let positive = |value: f32| value.is_finite() && value > 0.0;
        if !positive(self.height)
            || !positive(self.trunk_radius)
            || !positive(self.crown_radius)
            || !(0.0..0.65).contains(&self.crown_base)
            || self.scaffold_limbs < 3
            || self.forks_per_limb < 2
            || !valid_color(self.leaf_color)
        {
            return Err(Error::Invalid("invalid broadleaf scaffold"));
        }
        let limbs = usize::from(self.scaffold_limbs);
        let forks = limbs * usize::from(self.forks_per_limb);
        let mut random = Random(seed);
        let leader = [
            (random.unit() - 0.5) * self.crown_radius * 0.14,
            self.height * 0.91,
            (random.unit() - 0.5) * self.crown_radius * 0.14,
        ];
        let mut stems = Vec::with_capacity(1 + limbs + forks);
        let mut foliage = Vec::with_capacity(forks * 2);
        stems.push(Stem {
            start: [0.0; 3],
            end: leader,
            base_radius: self.trunk_radius,
            tip_radius: self.trunk_radius * 0.09,
            parent: None,
        });
        let phase = random.unit() * core::f32::consts::TAU;
        // Golden-angle phyllotaxis avoids vertically aligned limb rows.
        let golden_angle = core::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
        for limb in 0..self.scaffold_limbs {
            let rank = (f32::from(limb) + 0.5) / f32::from(self.scaffold_limbs);
            let t = rank.mul_add(0.70 - self.crown_base, self.crown_base);
            let angle =
                (random.unit() - 0.5).mul_add(0.38, f32::from(limb).mul_add(golden_angle, phase));
            let origin = mul(leader, t);
            let reach =
                self.crown_radius * rank.mul_add(-0.5, 1.0) * random.unit().mul_add(0.16, 0.56);
            let tip_y = self.height * rank.mul_add(0.34, 0.48);
            let (sin, cos) = angle.sin_cos();
            let end = add(origin, [reach * cos, tip_y - origin[1], reach * sin]);
            // A scaffold is a substantial fraction of the tapering trunk
            // at its actual attachment, rather than a fraction of its tip.
            let radius = self.trunk_radius * t.mul_add(-0.91, 1.0) * 0.59;
            let parent = u32::try_from(stems.len()).map_err(|_| Error::TooLarge)?;
            stems.push(Stem {
                start: origin,
                end,
                base_radius: radius,
                tip_radius: radius * 0.35,
                parent: Some(0),
            });
            self.fork_limb(&mut stems, &mut foliage, parent, angle, rank, &mut random)?;
        }
        Plant::from_parts(stems, foliage)
    }

    fn fork_limb(
        self,
        stems: &mut Vec<Stem>,
        foliage: &mut Vec<Foliage>,
        parent: u32,
        angle: f32,
        rank: f32,
        random: &mut Random,
    ) -> Result<()> {
        let limb = stems[usize::try_from(parent).map_err(|_| Error::TooLarge)?];
        for slot in 0..self.forks_per_limb {
            let f = (f32::from(slot) + 0.5) / f32::from(self.forks_per_limb);
            let along = f.mul_add(0.63, 0.35);
            let start = add(limb.start, mul(sub(limb.end, limb.start), along));
            let direction = (random.unit() - 0.5).mul_add(0.55, angle + f.mul_add(2.4, -1.2));
            let (sin, cos) = direction.sin_cos();
            let reach =
                self.crown_radius * random.unit().mul_add(0.13, 0.17) * rank.mul_add(-0.42, 1.0);
            let rise = self.height * random.unit().mul_add(0.11, 0.08);
            let end = add(start, [reach * cos, rise, reach * sin]);
            let radius = limb.base_radius * along.mul_add(-0.65, 1.0) * 0.53;
            let stem = u32::try_from(stems.len()).map_err(|_| Error::TooLarge)?;
            stems.push(Stem {
                start,
                end,
                base_radius: radius,
                tip_radius: radius * 0.22,
                parent: Some(parent),
            });
            // Two overlapping lobes: one inward/lower and one outward/upper.
            // Vary their proportions so the crown isn't a uniform sphere.
            for position in [0.48_f32, 0.95] {
                let center = add(start, mul(sub(end, start), position));
                let radius = self.crown_radius
                    * random.unit().mul_add(0.075, 0.135)
                    * rank.mul_add(-0.24, 1.0);
                let variation = random.unit().mul_add(0.36, 0.82);
                let mut color = self.leaf_color;
                for channel in &mut color[..3] {
                    *channel *= variation;
                }
                foliage.push(Foliage {
                    center,
                    radii: [
                        radius * random.unit().mul_add(0.28, 0.84),
                        radius * random.unit().mul_add(0.45, 0.59),
                        radius * random.unit().mul_add(0.28, 0.84),
                    ],
                    color,
                    stem,
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Module, ScreenLod};

    #[test]
    fn scaffold_is_connected_and_crown_is_deep_across_seeds() -> Result<()> {
        let species = Broadleaf::default();
        for seed in 0..64 {
            let tree = species.generate(seed)?;
            assert_eq!(tree, species.generate(seed)?);
            assert_ne!(tree, species.generate(seed + 1)?);
            assert_eq!(tree.stems().len(), 41);
            assert_eq!(tree.foliage().len(), 64);
            for stem in tree.stems().iter().skip(1) {
                let parent = tree.stems()[stem.parent.unwrap() as usize];
                let axis = sub(parent.end, parent.start);
                let relative = sub(stem.start, parent.start);
                let t = crate::geometry::dot(relative, axis) / crate::geometry::dot(axis, axis);
                assert!((0.0..=1.0).contains(&t));
                let residual = sub(relative, mul(axis, t));
                assert!(crate::geometry::dot(residual, residual) < 1e-8);
            }
            assert!(tree.bounds().max[1] >= species.height * 0.9);
            assert!(
                tree.foliage()
                    .iter()
                    .any(|leaf| leaf.center[1] < species.height * 0.5)
            );
            assert!(
                tree.foliage()
                    .iter()
                    .any(|leaf| leaf.center[1] > species.height * 0.75)
            );
            for (pixels, budget) in [(96.0, 24), (256.0, 64)] {
                let tier = tree.module_tier(
                    ScreenLod::new(pixels, 0.45, 0.01)?.with_cluster_budget(budget)?,
                    [0.2, 0.1, 0.05, 1.0],
                )?;
                assert!(tier.stem_count() > 1);
                let mut members = tier
                    .modules
                    .iter()
                    .flat_map(|m| match m {
                        Module::Canopy { source_ids, .. } => source_ids.as_slice(),
                        Module::Stem { .. } => &[],
                    })
                    .copied()
                    .collect::<Vec<_>>();
                members.sort_unstable();
                assert_eq!(members, (0..64).collect::<Vec<_>>());
                assert_eq!(tier.source_frame, tree.source_frame());
            }
        }
        Ok(())
    }

    #[test]
    fn invalid_parameters_fail() {
        for species in [
            Broadleaf {
                scaffold_limbs: 2,
                ..Broadleaf::default()
            },
            Broadleaf {
                forks_per_limb: 0,
                ..Broadleaf::default()
            },
            Broadleaf {
                crown_base: 0.8,
                ..Broadleaf::default()
            },
            Broadleaf {
                trunk_radius: f32::NAN,
                ..Broadleaf::default()
            },
        ] {
            assert!(species.generate(0).is_err());
        }
    }
}
