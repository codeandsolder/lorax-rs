//! Excurrent crowns: a central leader and successively shorter radial whorls.
//! The same immutable graph can be used by any of the existing LOD algorithms.

use crate::{
    Foliage, Plant, Stem,
    generation::{Random, valid_color},
    geometry::{Error, Result},
};

/// A whorled conifer with a single leader. Lengths are model-space metres;
/// crown base is the fraction of leader height occupied by bare trunk.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WhorledConifer {
    pub height: f32,
    pub trunk_radius: f32,
    /// Maximum radial crown extent, including terminal needle clusters.
    pub crown_radius: f32,
    pub crown_base: f32,
    pub whorls: u8,
    pub branches_per_whorl: u8,
    pub taper_power: f32,
    /// Elevation of lateral branches above horizontal, in radians.
    pub branch_elevation: f32,
    pub leaf_color: [f32; 4],
}

impl Default for WhorledConifer {
    fn default() -> Self {
        Self {
            height: 20.0,
            trunk_radius: 0.36,
            crown_radius: 4.5,
            crown_base: 0.16,
            whorls: 8,
            branches_per_whorl: 5,
            taper_power: 0.85,
            branch_elevation: 0.12,
            leaf_color: [0.051, 0.128, 0.074, 1.0],
        }
    }
}

impl WhorledConifer {
    /// Generate a reproducible single-leader tree and its needle clusters.
    /// Branch whorls ascend the trunk, with shorter radial reach toward the
    /// apex. Cluster envelopes overlap along the branches, rather than
    /// representing every needle or terminal twig individually.
    ///
    /// # Errors
    /// Rejects invalid parameters and unrepresentable stem or foliage geometry.
    pub fn generate(self, seed: u64) -> Result<Plant> {
        let positive = |v: f32| v.is_finite() && v > 0.0;
        if !positive(self.height)
            || !positive(self.trunk_radius)
            || !positive(self.crown_radius)
            || !positive(self.taper_power)
            || !(0.0..0.8).contains(&self.crown_base)
            || self.whorls == 0
            || self.branches_per_whorl == 0
            || !(-1.0..=1.0).contains(&self.branch_elevation)
            || !valid_color(self.leaf_color)
        {
            return Err(Error::Invalid("invalid whorled conifer parameters"));
        }
        let branches = usize::from(self.whorls) * usize::from(self.branches_per_whorl);
        let mut stems = Vec::with_capacity(branches + 1);
        let mut foliage = Vec::with_capacity(branches * 3 + 1);
        stems.push(Stem {
            start: [0.0; 3],
            end: [0.0, self.height, 0.0],
            base_radius: self.trunk_radius,
            tip_radius: self.trunk_radius * 0.04,
            parent: None,
        });
        let mut random = Random(seed);
        let spacing = self.height * (1.0 - self.crown_base) / f32::from(self.whorls);
        let base = self.crown_base * self.height;
        for whorl in 0..self.whorls {
            let fraction = f32::from(whorl) / f32::from(self.whorls);
            let reach = self.crown_radius * (1.0 - fraction).powf(self.taper_power);
            let height = spacing.mul_add(f32::from(whorl) + 0.35, base);
            let azimuth = core::f32::consts::TAU * random.unit();
            for slot in 0..self.branches_per_whorl {
                let phase = (random.unit() - 0.5).mul_add(0.38, f32::from(slot))
                    / f32::from(self.branches_per_whorl);
                let angle = core::f32::consts::TAU.mul_add(phase, azimuth);
                let (sin, cos) = angle.sin_cos();
                let length = reach * random.unit().mul_add(0.15, 0.68);
                let y = spacing.mul_add((random.unit() - 0.5) * 0.27, height);
                let rise =
                    (length * self.branch_elevation.tan()).clamp(-spacing * 0.45, spacing * 0.45);
                let radius = self.trunk_radius * (1.0 - fraction) * 0.10;
                let stem = u32::try_from(stems.len()).map_err(|_| Error::TooLarge)?;
                stems.push(Stem {
                    start: [0.0, y, 0.0],
                    end: [length * cos, y + rise, length * sin],
                    base_radius: radius,
                    tip_radius: radius * 0.15,
                    parent: Some(0),
                });
                for along in [0.25_f32, 0.55, 0.85] {
                    let radial = reach * 0.24;
                    let transverse = reach * along.mul_add(-0.13, 0.33);
                    let vertical = spacing * random.unit().mul_add(0.16, 0.37);
                    let distance = along * length;
                    foliage.push(Foliage {
                        center: [distance * cos, along.mul_add(rise, y), distance * sin],
                        // Bounding extents of a radial horizontal ellipsoid.
                        radii: [
                            (radial * cos).hypot(transverse * sin),
                            vertical,
                            (radial * sin).hypot(transverse * cos),
                        ],
                        color: varied_color(self.leaf_color, &mut random),
                        stem,
                    });
                }
            }
        }
        foliage.push(Foliage {
            center: [0.0, spacing.mul_add(-0.24, self.height), 0.0],
            radii: [
                self.crown_radius * 0.11,
                spacing * 0.55,
                self.crown_radius * 0.11,
            ],
            color: varied_color(self.leaf_color, &mut random),
            stem: 0,
        });
        Plant::from_parts(stems, foliage)
    }
}

fn varied_color(mut color: [f32; 4], random: &mut Random) -> [f32; 4] {
    let variation = random.unit().mul_add(0.30, 0.85);
    for channel in &mut color[..3] {
        *channel *= variation;
    }
    color
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScreenLod;

    #[test]
    fn whorled_crown_repeats_tapers_and_preserves_all_members_at_every_lod() -> Result<()> {
        let species = WhorledConifer::default();
        for seed in 0..24 {
            let tree = species.generate(seed)?;
            assert_eq!(tree, species.generate(seed)?);
            assert_ne!(tree, species.generate(seed + 1)?);
            assert_eq!(tree.stems().len(), 41);
            assert_eq!(tree.foliage().len(), 121);
            let reach = |branches: &[Stem]| {
                branches
                    .iter()
                    .map(|stem| stem.end[0].hypot(stem.end[2]))
                    .fold(0.0_f32, f32::max)
            };
            assert!(reach(&tree.stems()[36..41]) < 0.3 * reach(&tree.stems()[1..6]));
            assert!(tree.bounds().max[1] > species.height);
            assert!(
                tree.foliage()
                    .iter()
                    .all(|leaf| leaf.center[1] > species.height * 0.1)
            );
            for (pixels, budget) in [(96.0, 24), (256.0, 64)] {
                let tier = tree.module_tier(
                    ScreenLod::new(pixels, 0.75, 0.01)?.with_cluster_budget(budget)?,
                    [0.2, 0.1, 0.05, 1.0],
                )?;
                assert_eq!(tier.stem_count(), 1);
                assert_eq!(tier.canopy_count(), budget);
                let mut members = tier
                    .modules
                    .iter()
                    .flat_map(|module| match module {
                        crate::Module::Canopy { source_ids, .. } => source_ids.as_slice(),
                        crate::Module::Stem { .. } => &[],
                    })
                    .copied()
                    .collect::<Vec<_>>();
                members.sort_unstable();
                assert_eq!(members, (0..121).collect::<Vec<_>>());
                assert_eq!(tier.source_frame, tree.source_frame());
            }
        }
        Ok(())
    }

    #[test]
    fn rejects_invalid_whorl_parameters() {
        for species in [
            WhorledConifer {
                whorls: 0,
                ..WhorledConifer::default()
            },
            WhorledConifer {
                crown_base: f32::NAN,
                ..WhorledConifer::default()
            },
            WhorledConifer {
                branch_elevation: 2.0,
                ..WhorledConifer::default()
            },
        ] {
            assert!(species.generate(0).is_err());
        }
    }
}
