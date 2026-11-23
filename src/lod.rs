use crate::geometry::{
    Bounds, Error, Mesh, Plant, Result, Vertex, add, cross, dot, mul, normalize, sub,
};

/// Renderer-independent screen-space approximation request.
///
/// All projected dimensions refer to the bounding sphere of the complete
/// plant. Compute the diameter from the camera's focal length and depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenLod {
    pub projected_diameter: f32,
    /// Maximum permitted radial chord error on visible woody stems.
    pub stem_chord_error: f32,
    /// Aggregate foliage recursively until a group's bounds exceed this size.
    pub foliage_patch_diameter: f32,
    /// Upper limit on separate spatial foliage proxies for one plant.
    pub max_clusters: usize,
}

impl ScreenLod {
    /// # Errors
    /// Every parameter must be positive and finite.
    pub fn new(
        projected_diameter: f32,
        stem_chord_error: f32,
        foliage_patch_diameter: f32,
    ) -> Result<Self> {
        if ![projected_diameter, stem_chord_error, foliage_patch_diameter]
            .into_iter()
            .all(|v| v.is_finite() && v > 0.0)
        {
            return Err(Error::Invalid("screen-space LOD parameters"));
        }
        Ok(Self {
            projected_diameter,
            stem_chord_error,
            foliage_patch_diameter,
            max_clusters: 32,
        })
    }
    /// Limit independent foliage aggregates without losing any source cards.
    ///
    /// # Errors
    /// Rejects a zero cluster budget.
    pub const fn with_cluster_budget(mut self, maximum: usize) -> Result<Self> {
        if maximum == 0 {
            return Err(Error::Invalid("zero foliage cluster budget"));
        }
        self.max_clusters = maximum;
        Ok(self)
    }
}

impl Plant {
    /// Tessellate this immutable plant at one screen-space detail request.
    ///
    /// No branches, seeds or leaf locations are regenerated in this call.
    /// The returned full-source bounds are identical for all requested LODs.
    ///
    /// # Errors
    /// Rejects invalid screen targets or an unrepresentable mesh/vertex count.
    pub fn mesh(&self, target: ScreenLod, wood_color: [f32; 4]) -> Result<Mesh> {
        // Public fields allow callers to construct ScreenLod directly: validate
        // here as well, rather than relying on the optional constructor.
        ScreenLod::new(
            target.projected_diameter,
            target.stem_chord_error,
            target.foliage_patch_diameter,
        )?;
        if target.max_clusters == 0 {
            return Err(Error::Invalid("zero foliage cluster budget"));
        }
        if !wood_color[..3].iter().all(|c| c.is_finite() && *c >= 0.0)
            || !wood_color[3].is_finite()
            || !(0.0..=1.0).contains(&wood_color[3])
        {
            return Err(Error::Invalid("wood material color"));
        }
        let scale = target.projected_diameter / self.source_frame().diameter();
        if !scale.is_finite() || scale <= 0.0 {
            return Err(Error::Invalid("unrepresentable projected scale"));
        }

        let stems = self.stems();
        let mut mesh = Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
            source_bounds: self.bounds(),
            source_frame: self.source_frame(),
        };
        for stem in stems {
            let projected_radius = stem.base_radius.max(stem.tip_radius) * scale;
            if crate::modules::select_stem(stem, scale, target.stem_chord_error) {
                let sides = radial_sides(projected_radius, target.stem_chord_error)?;
                cylinder(&mut mesh, stem, sides, wood_color)?;
            }
        }

        // All foliage follows one deterministic spatial hierarchy, not the
        // branch attachment graph (which may contain a single giant root).
        for cluster in self.foliage_clusters(target)? {
            canopy_octahedron(&mut mesh, cluster.bounds, cluster.color)?;
        }
        if mesh.vertices.iter().any(|v| {
            !v.position.into_iter().all(f32::is_finite) || !v.normal.into_iter().all(f32::is_finite)
        }) {
            return Err(Error::Invalid("unrepresentable tessellation coordinates"));
        }
        Ok(mesh)
    }
}

fn radial_sides(projected_radius: f32, max_error: f32) -> Result<u32> {
    // Polygonal circle sagitta error: radius * (1 - cos(PI / sides)).
    // Avoid dividing by a zero radius and retain at least a triangular stem.
    if projected_radius <= max_error {
        return Ok(3);
    }
    let angle = (1.0 - max_error / projected_radius).acos();
    let sides = (core::f32::consts::PI / angle).ceil().max(3.0);
    if !sides.is_finite() || sides > 4096.0 {
        return Err(Error::TooLarge);
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "finite positive side count <= 4096 verified"
    )]
    #[expect(
        clippy::cast_sign_loss,
        reason = "finite nonnegative sides bounded to 4096"
    )]
    Ok(sides as u32)
}

fn append_vertices(mesh: &mut Mesh, vertices: impl IntoIterator<Item = Vertex>) -> Result<u32> {
    let base = u32::try_from(mesh.vertices.len()).map_err(|_| Error::TooLarge)?;
    let vertices = vertices.into_iter();
    mesh.vertices
        .try_reserve(vertices.size_hint().0)
        .map_err(|_| Error::TooLarge)?;
    mesh.vertices.extend(vertices);
    if mesh.vertices.len() > u32::MAX as usize {
        return Err(Error::TooLarge);
    }
    Ok(base)
}

fn append_faces(mesh: &mut Mesh, base: u32, faces: &[[u32; 3]]) -> Result<()> {
    mesh.indices
        .try_reserve(faces.len().checked_mul(3).ok_or(Error::TooLarge)?)
        .map_err(|_| Error::TooLarge)?;
    for triangle in faces {
        for corner in triangle {
            mesh.indices
                .push(base.checked_add(*corner).ok_or(Error::TooLarge)?);
        }
    }
    Ok(())
}

fn cylinder(mesh: &mut Mesh, stem: &crate::Stem, sides: u32, color: [f32; 4]) -> Result<()> {
    let delta = sub(stem.end, stem.start);
    let tangent = normalize(delta);
    let reference = if tangent[1].abs() > 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let axis_u = normalize(cross(tangent, reference));
    let axis_v = cross(tangent, axis_u);
    let length = dot(delta, delta).sqrt();
    let slope = (stem.base_radius - stem.tip_radius) / length;

    let mut vertices = Vec::new();
    vertices
        .try_reserve(usize::try_from(sides).map_err(|_| Error::TooLarge)? * 2)
        .map_err(|_| Error::TooLarge)?;
    for side in 0..sides {
        #[expect(clippy::cast_precision_loss, reason = "side count <= 4096")]
        let turns = side as f32 / sides as f32;
        let angle = turns * core::f32::consts::TAU;
        let (sin, cos) = angle.sin_cos();
        let radial = add(mul(axis_u, cos), mul(axis_v, sin));
        let normal = normalize(add(radial, mul(tangent, slope)));
        for (endpoint, radius, v) in [
            (stem.start, stem.base_radius, 0.0),
            (stem.end, stem.tip_radius, 1.0),
        ] {
            vertices.push(Vertex {
                position: add(endpoint, mul(radial, radius)),
                normal,
                uv: [turns, v],
                color,
            });
        }
    }
    let base = append_vertices(mesh, vertices)?;
    mesh.indices
        .try_reserve(usize::try_from(sides).map_err(|_| Error::TooLarge)? * 6)
        .map_err(|_| Error::TooLarge)?;
    for side in 0..sides {
        let a = base.checked_add(side * 2).ok_or(Error::TooLarge)?;
        let b = base
            .checked_add(((side + 1) % sides) * 2)
            .ok_or(Error::TooLarge)?;
        mesh.indices
            .extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
    }
    Ok(())
}

fn canopy_octahedron(mesh: &mut Mesh, bounds: Bounds, color: [f32; 4]) -> Result<()> {
    let center = bounds.center();
    let radii = std::array::from_fn::<_, 3, _>(|axis| bounds.max[axis] - center[axis]);
    let directions = [
        [1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, -1.0],
    ];
    let base = append_vertices(
        mesh,
        directions.into_iter().map(|normal| Vertex {
            position: add(center, std::array::from_fn(|i| normal[i] * radii[i])),
            normal,
            uv: [0.5, 0.5],
            color,
        }),
    )?;
    append_faces(
        mesh,
        base,
        &[
            [2, 4, 0],
            [2, 1, 4],
            [2, 5, 1],
            [2, 0, 5],
            [3, 0, 4],
            [3, 4, 1],
            [3, 1, 5],
            [3, 5, 0],
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Foliage, Species, Stem};

    #[test]
    fn detail_rises_with_projected_crown_and_bounds_remain_constant() -> Result<()> {
        let species = Species::default();
        let tree = Plant::generate(&species, 7347)?;
        let mut previous = 0;
        for diameter in [4.0, 8.0, 16.0, 32.0, 80.0, 200.0, 600.0] {
            let lod = ScreenLod::new(diameter, 0.75, 12.0)?;
            let mesh = tree.mesh(lod, species.wood_color)?;
            assert_eq!(mesh.source_bounds, tree.bounds());
            assert!(mesh.triangle_count() >= previous);
            assert!(
                mesh.indices
                    .iter()
                    .all(|i| usize::try_from(*i).is_ok_and(|i| i < mesh.vertices.len()))
            );
            assert!(
                mesh.vertices
                    .iter()
                    .all(|v| v.position.into_iter().all(f32::is_finite))
            );
            previous = mesh.triangle_count();
        }
        Ok(())
    }

    #[test]
    fn mesh_repeated_and_all_foliage_covered() -> Result<()> {
        let source = Plant::from_parts(
            vec![
                Stem {
                    start: [0.0, 0.0, 0.0],
                    end: [0.0, 5.0, 0.0],
                    base_radius: 0.3,
                    tip_radius: 0.1,
                    parent: None,
                },
                Stem {
                    start: [0.0, 3.0, 0.0],
                    end: [2.0, 5.0, 0.0],
                    base_radius: 0.1,
                    tip_radius: 0.03,
                    parent: Some(0),
                },
                Stem {
                    start: [0.0, 3.0, 0.0],
                    end: [-2.0, 5.0, 0.0],
                    base_radius: 0.1,
                    tip_radius: 0.03,
                    parent: Some(0),
                },
            ],
            vec![
                Foliage {
                    center: [2.0, 5.0, 0.0],
                    radii: [1.0, 1.0, 1.0],
                    color: [0.2, 0.6, 0.2, 1.0],
                    stem: 1,
                },
                Foliage {
                    center: [-2.0, 5.0, 0.0],
                    radii: [1.0, 1.0, 1.0],
                    color: [0.2, 0.6, 0.2, 1.0],
                    stem: 2,
                },
            ],
        )?;
        let far = source.mesh(ScreenLod::new(4.0, 0.75, 12.0)?, [0.3, 0.2, 0.1, 1.0])?;
        let near = source.mesh(ScreenLod::new(200.0, 0.75, 12.0)?, [0.3, 0.2, 0.1, 1.0])?;
        assert_eq!(
            far.indices,
            source
                .mesh(ScreenLod::new(4.0, 0.75, 12.0)?, [0.3, 0.2, 0.1, 1.0])?
                .indices
        );
        assert!(far.triangle_count() < near.triangle_count());
        // The aggregate covers both extremal canopy lobes, not just the root tip.
        assert!(far.vertices.iter().any(|v| v.position[0] >= 3.0));
        assert!(far.vertices.iter().any(|v| v.position[0] <= -3.0));
        Ok(())
    }

    #[test]
    fn finite_and_valid_inputs_are_enforced_at_api_boundaries() -> Result<()> {
        let tree = Plant::generate(&Species::default(), 0)?;
        assert!(ScreenLod::new(0.0, 1.0, 2.0).is_err());
        assert!(
            tree.mesh(
                ScreenLod {
                    projected_diameter: f32::INFINITY,
                    stem_chord_error: 1.0,
                    foliage_patch_diameter: 10.0,
                    max_clusters: 32
                },
                [1.0; 4]
            )
            .is_err()
        );
        assert!(
            tree.mesh(ScreenLod::new(20.0, 1.0, 12.0)?, [1.0, -1.0, 1.0, 1.0])
                .is_err()
        );
        assert!(
            tree.mesh(ScreenLod::new(20.0, f32::MIN_POSITIVE, 12.0)?, [1.0; 4])
                .is_err()
        );
        Ok(())
    }
}
