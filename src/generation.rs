use crate::geometry::{Error, Foliage, Plant, Result, Stem, add, mul, normalize, sub};

/// Branching parameters in model-space metres and radians.
///
/// The count budget bounds pathological exponential branching before allocating.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Species {
    pub height: f32,
    pub trunk_radius: f32,
    pub crown_radius: f32,
    /// Number of branching generations after the trunk (zero is valid).
    pub levels: u8,
    pub fanout: u8,
    pub length_decay: f32,
    pub radius_decay: f32,
    /// Outward angle from local vertical, in radians.
    pub spread_radians: f32,
    pub wood_color: [f32; 4],
    pub leaf_color: [f32; 4],
}

impl Default for Species {
    fn default() -> Self {
        Self {
            height: 12.0,
            trunk_radius: 0.42,
            crown_radius: 4.2,
            levels: 3,
            fanout: 4,
            length_decay: 0.56,
            radius_decay: 0.53,
            spread_radians: 0.85,
            wood_color: [0.32, 0.18, 0.09, 1.0],
            leaf_color: [0.21, 0.46, 0.13, 1.0],
        }
    }
}

impl Species {
    /// Verify the complete model can be represented and bounded.
    ///
    /// # Errors
    /// Returns `Invalid` for non-finite/degenerate geometry and `TooLarge` if
    /// the branching fanout would exhaust the configured 262144-stem budget.
    pub fn validate(&self) -> Result<()> {
        let finite_positive = |x: f32| x.is_finite() && x > 0.0;
        if !finite_positive(self.height)
            || !finite_positive(self.trunk_radius)
            || !finite_positive(self.crown_radius)
            || !finite_positive(self.length_decay)
            || self.length_decay > 1.0
            || !finite_positive(self.radius_decay)
            || self.radius_decay > 1.0
            || !self.spread_radians.is_finite()
            || !(0.0..=core::f32::consts::PI).contains(&self.spread_radians)
            || self.fanout == 0
            || !valid_color(self.wood_color)
            || !valid_color(self.leaf_color)
        {
            return Err(Error::Invalid("invalid species parameters"));
        }
        let mut total = 1usize;
        let mut generation = 1usize;
        for _ in 0..self.levels {
            generation = generation
                .checked_mul(usize::from(self.fanout))
                .ok_or(Error::TooLarge)?;
            total = total.checked_add(generation).ok_or(Error::TooLarge)?;
            if total > 262_144 {
                return Err(Error::TooLarge);
            }
        }
        Ok(())
    }
}

fn valid_color(v: [f32; 4]) -> bool {
    v[..3].iter().all(|x| x.is_finite() && *x >= 0.0)
        && v[3].is_finite()
        && (0.0..=1.0).contains(&v[3])
}

struct Random(u64);

impl Random {
    const fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut x = self.0;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^ (x >> 31)
    }

    // Exactly representable 24-bit mantissa fraction in [0, 1).
    #[expect(
        clippy::cast_precision_loss,
        reason = "masked integer is at most 24 bits"
    )]
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 * (1.0 / 16_777_216.0)
    }
}

impl Plant {
    /// Generate a stable branch graph and foliage from seed and species.
    ///
    /// This operation is independent of view LOD, tile partitioning and
    /// number of subsequent mesh generations. Callers should cache results
    /// per species/seed and instance the same meshes for matching variants.
    ///
    /// # Errors
    /// Rejects invalid species, excessive branching, or unrepresentable
    /// (overflowed) branch geometry.
    pub fn generate(species: &Species, seed: u64) -> Result<Self> {
        species.validate()?;
        let mut random = Random(seed);
        let mut stems = vec![Stem {
            start: [0.0, 0.0, 0.0],
            end: [0.0, species.height * 0.82, 0.0],
            base_radius: species.trunk_radius,
            tip_radius: species.trunk_radius * 0.30,
            parent: None,
        }];
        // Process breadth-first to keep all parent indices before their children.
        let mut begin = 0usize;
        let mut end = 1usize;
        for generation in 0..species.levels {
            let width = usize::from(species.fanout);
            for parent_index in begin..end {
                let parent = stems[parent_index];
                let tangent = normalize(sub(parent.end, parent.start));
                for slot in 0..width {
                    let t = random.unit().mul_add(
                        0.5,
                        f32::from(u16::try_from(slot).map_err(|_| Error::TooLarge)?) + 0.25,
                    ) / f32::from(species.fanout);
                    let origin = add(
                        parent.start,
                        mul(sub(parent.end, parent.start), 0.6_f32.mul_add(t, 0.35)),
                    );
                    let jitter_a = random.unit();
                    let jitter_b = random.unit();
                    let azimuth =
                        core::f32::consts::TAU * jitter_b.mul_add(0.06, jitter_a.mul_add(0.08, t));
                    let (sin_az, cos_az) = azimuth.sin_cos();
                    let (sin_spread, cos_spread) = species.spread_radians.sin_cos();
                    let outward = [sin_az * sin_spread, cos_spread, cos_az * sin_spread];
                    let direction = normalize(add(mul(tangent, 0.30), outward));
                    let length = species.crown_radius
                        * species.length_decay.powi(i32::from(generation))
                        * (random.unit().mul_add(0.42, 0.62));
                    let radius = parent.tip_radius * species.radius_decay;
                    stems.push(Stem {
                        start: origin,
                        end: add(origin, mul(direction, length)),
                        base_radius: radius,
                        tip_radius: (radius * 0.38).max(f32::MIN_POSITIVE),
                        parent: Some(u32::try_from(parent_index).map_err(|_| Error::TooLarge)?),
                    });
                }
            }
            begin = end;
            end = stems.len();
        }
        // One terminal foliage lobe per tip. LOD aggregation later merges these
        // in the existing stem graph; no random resampling during camera motion.
        let mut foliage = Vec::with_capacity(end - begin);
        for (index, stem) in stems.iter().enumerate().skip(begin) {
            let variation = random.unit().mul_add(0.30, 0.85);
            let radius = species.crown_radius
                * (0.18 + 0.35 / f32::from(species.fanout).sqrt())
                * species.length_decay.powi(i32::from(species.levels) / 2);
            let center = add(
                stem.end,
                [
                    (random.unit() - 0.5) * radius * 0.35,
                    (random.unit() - 0.5) * radius * 0.35,
                    (random.unit() - 0.5) * radius * 0.35,
                ],
            );
            foliage.push(Foliage {
                center,
                radii: [radius, radius * (random.unit().mul_add(0.4, 0.75)), radius],
                color: [
                    species.leaf_color[0] * variation,
                    species.leaf_color[1] * variation,
                    species.leaf_color[2] * variation,
                    species.leaf_color[3],
                ],
                stem: u32::try_from(index).map_err(|_| Error::TooLarge)?,
            });
        }
        Self::from_parts(stems, foliage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeatable_across_multiple_meshing_requests() {
        let species = Species::default();
        let one = Plant::generate(&species, 123_456).unwrap();
        let two = Plant::generate(&species, 123_456).unwrap();
        assert_eq!(one, two);
        assert_ne!(one, Plant::generate(&species, 123_457).unwrap());
        assert_eq!(one.stems().len(), 85);
        assert_eq!(one.foliage().len(), 64);
    }

    #[test]
    fn branches_are_ordered_and_degenerate_inputs_are_rejected() {
        let mut species = Species {
            levels: 0,
            ..Species::default()
        };
        let trunk = Plant::generate(&species, 0).unwrap();
        assert_eq!(trunk.stems().len(), 1);
        assert_eq!(trunk.foliage().len(), 1);
        species.levels = u8::MAX;
        assert_eq!(Plant::generate(&species, 0).unwrap_err(), Error::TooLarge);
        species.levels = 2;
        species.length_decay = f32::NAN;
        assert!(Plant::generate(&species, 0).is_err());
    }
}
