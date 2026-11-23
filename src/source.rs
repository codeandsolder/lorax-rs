//! Fit stable procedural structure from original triangle components.
//!
//! Source card indices remain available for eventual coverage-preserving,
//! original-material cluster impostors, rather than dropping alpha cards.

use std::collections::BTreeMap;

use crate::geometry::{
    Bounds, Error, Foliage, Plant, Result, SourceFrame, Stem, add, dot, mul, sub, valid3,
};

/// Source positions must already share a single world-independent model frame.
/// Separate material surfaces retain independent color/texture ownership.
#[derive(Clone, Copy, Debug)]
pub struct SourceSurface<'a> {
    pub positions: &'a [[f32; 3]],
    pub indices: &'a [u32],
    pub color: [f32; 4],
}

/// Exact authoring triangles of one index-connected foliage component.
#[derive(Clone, Debug, PartialEq)]
pub struct FoliageCard {
    pub surface: usize,
    pub triangles: Vec<[u32; 3]>,
    pub bounds: Bounds,
    /// Unfiltered triangle area; source alpha textures must be accounted for
    /// separately when producing color/coverage-preserving impostors.
    pub area: f32,
}

/// Procedural approximation plus lossless topology references to source cards.
/// The order of cards matches the order of Plant foliage nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceFit {
    pub plant: Plant,
    pub cards: Vec<FoliageCard>,
    pub wood_components: usize,
}

struct Component {
    surface: usize,
    first: u32,
    triangles: Vec<[u32; 3]>,
    positions: Vec<[f32; 3]>,
    bounds: Bounds,
    area: f32,
    color: [f32; 4],
}

const fn root(parents: &mut [usize], mut index: usize) -> usize {
    // Iterative path compression: malicious/very large source meshes can
    // otherwise create a deep DSU chain and overflow the Rust call stack.
    let mut representative = index;
    while parents[representative] != representative {
        representative = parents[representative];
    }
    while parents[index] != index {
        let next = parents[index];
        parents[index] = representative;
        index = next;
    }
    representative
}
fn union(parents: &mut [usize], a: usize, b: usize) {
    let a = root(parents, a);
    let b = root(parents, b);
    if a != b {
        parents[a.max(b)] = a.min(b);
    }
}
fn sq(v: [f32; 3]) -> f32 {
    dot(v, v)
}
fn closest(point: [f32; 3], stem: &Stem) -> [f32; 3] {
    let delta = sub(stem.end, stem.start);
    let t = (dot(sub(point, stem.start), delta) / sq(delta)).clamp(0.0, 1.0);
    add(stem.start, mul(delta, t))
}
fn distance2(point: [f32; 3], stem: &Stem) -> f32 {
    sq(sub(point, closest(point, stem)))
}

fn components(surfaces: &[SourceSurface<'_>]) -> Result<Vec<Component>> {
    let mut output = Vec::new();
    for (surface, source) in surfaces.iter().enumerate() {
        if source.positions.is_empty()
            || source.indices.is_empty()
            || !source.indices.len().is_multiple_of(3)
            || source.positions.len() > u32::MAX as usize
            || source.positions.iter().any(|&v| !valid3(v))
            || source.color[..3].iter().any(|&v| !v.is_finite() || v < 0.0)
            || !source.color[3].is_finite()
            || !(0.0..=1.0).contains(&source.color[3])
        {
            return Err(Error::Invalid("invalid source triangle surface"));
        }
        let mut parents: Vec<usize> = (0..source.positions.len()).collect();
        for tri in source.indices.as_chunks::<3>().0 {
            for &id in tri {
                if usize::try_from(id).map_or(true, |i| i >= parents.len()) {
                    return Err(Error::Invalid("source index exceeds vertex count"));
                }
            }
            union(&mut parents, tri[0] as usize, tri[1] as usize);
            union(&mut parents, tri[0] as usize, tri[2] as usize);
        }
        let mut parts = BTreeMap::<usize, Vec<[u32; 3]>>::new();
        for tri in source.indices.as_chunks::<3>().0 {
            let key = root(&mut parents, tri[0] as usize);
            parts.entry(key).or_default().push([tri[0], tri[1], tri[2]]);
        }
        for (first, triangles) in parts {
            let mut indices: Vec<usize> =
                triangles.iter().flatten().map(|&id| id as usize).collect();
            indices.sort_unstable();
            indices.dedup();
            let mut bounds = Bounds {
                min: [f32::INFINITY; 3],
                max: [f32::NEG_INFINITY; 3],
            };
            let positions: Vec<_> = indices
                .iter()
                .map(|&id| {
                    let pos = source.positions[id];
                    for axis in 0..3 {
                        bounds.min[axis] = bounds.min[axis].min(pos[axis]);
                        bounds.max[axis] = bounds.max[axis].max(pos[axis]);
                    }
                    pos
                })
                .collect();
            let mut area = 0.0_f32;
            for tri in &triangles {
                let [a, b, c] = tri.map(|i| source.positions[i as usize]);
                let vector = crate::geometry::cross(sub(b, a), sub(c, a));
                area = 0.5_f32.mul_add(sq(vector).sqrt(), area);
            }
            if !area.is_finite() || !bounds.min.iter().all(|x| x.is_finite()) {
                return Err(Error::Invalid("unrepresentable source component"));
            }
            output.push(Component {
                surface,
                first: u32::try_from(first).map_err(|_| Error::TooLarge)?,
                triangles,
                positions,
                bounds,
                area,
                color: source.color,
            });
        }
    }
    Ok(output)
}

/// Approximate an arbitrary connected wood triangle component by a tapering
/// segment. A covariance power iteration finds its dominant axis, including
/// lateral branches. It does not require a vertical trunk or named nodes.
fn wood_segment(component: &Component) -> Result<Stem> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "component is limited to u32 vertex count"
    )]
    let inv = 1.0 / component.positions.len() as f32;
    let mean = mul(
        component
            .positions
            .iter()
            .fold([0.0; 3], |sum, &p| add(sum, p)),
        inv,
    );
    let mut matrix = [[0.0_f64; 3]; 3];
    for &pos in &component.positions {
        let p = sub(pos, mean);
        for (i, row) in matrix.iter_mut().enumerate() {
            for (j, value) in row.iter_mut().enumerate() {
                *value = f64::from(p[i]).mul_add(f64::from(p[j]), *value);
            }
        }
    }
    let span =
        std::array::from_fn::<_, 3, _>(|i| component.bounds.max[i] - component.bounds.min[i]);
    let dominant = (0..3)
        .max_by(|&a, &b| span[a].total_cmp(&span[b]))
        .ok_or(Error::Invalid("source axis"))?;
    let mut dir = [0.0_f64; 3];
    dir[dominant] = 1.0;
    for _ in 0..24 {
        let next =
            std::array::from_fn::<_, 3, _>(|i| (0..3).map(|j| matrix[i][j] * dir[j]).sum::<f64>());
        let length = next.iter().map(|n| n * n).sum::<f64>().sqrt();
        if !length.is_finite() || length <= f64::MIN_POSITIVE {
            break;
        }
        dir = next.map(|n| n / length);
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "principal axis coefficients are within [-1,1]"
    )]
    let dir = dir.map(|value| value as f32);
    let length = sq(dir).sqrt();
    if !length.is_finite() || length <= f32::MIN_POSITIVE {
        return Err(Error::Invalid("degenerate covariance axis"));
    }
    let dir = mul(dir, 1.0 / length);
    let (minimum, maximum) =
        component
            .positions
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), &p| {
                let projection = dot(sub(p, mean), dir);
                (min.min(projection), max.max(projection))
            });
    let start = add(mean, mul(dir, minimum));
    let end = add(mean, mul(dir, maximum));
    let length = sq(sub(end, start)).sqrt();
    if !length.is_finite() || length <= f32::MIN_POSITIVE {
        return Err(Error::Invalid("degenerate source wood component"));
    }
    let radius = component
        .positions
        .iter()
        .map(|&p| {
            sq(sub(
                p,
                closest(
                    p,
                    &Stem {
                        start,
                        end,
                        base_radius: 1.0,
                        tip_radius: 1.0,
                        parent: None,
                    },
                ),
            ))
            .sqrt()
        })
        .fold(0.0_f32, f32::max)
        .max(length * 0.001);
    if !radius.is_finite() {
        return Err(Error::Invalid("invalid fitted wood radius"));
    }
    Ok(Stem {
        start,
        end,
        base_radius: radius,
        tip_radius: (radius * 0.3).max(f32::MIN_POSITIVE),
        parent: None,
    })
}

impl SourceFit {
    /// Recover a stem graph and the exact index-connected authored leaf cards.
    ///
    /// Wood components are ordered by original triangle area; parents precede
    /// children. Each leaf is attached to its nearest fitted wood component.
    /// Nonwoody vegetation receives a single finite surrogate root. A supplied
    /// canonical source frame is used for all visual LODs; generated bounds
    /// are separately supplied for conservative culling.
    ///
    /// # Errors
    /// Rejects missing source geometry, bad indices/colors, non-finite input,
    /// malformed components, and insufficient source bounds.
    pub fn fit(
        wood: &[SourceSurface<'_>],
        leaves: &[SourceSurface<'_>],
        source_frame: Option<SourceFrame>,
    ) -> Result<Self> {
        if wood.is_empty() && leaves.is_empty() {
            return Err(Error::Invalid("no source components"));
        }
        let mut woody = components(wood)?;
        let leafy = components(leaves)?;
        woody.sort_by(|a, b| {
            b.area
                .total_cmp(&a.area)
                .then_with(|| a.surface.cmp(&b.surface))
                .then_with(|| a.first.cmp(&b.first))
        });
        let wood_components = woody.len();
        let mut stems = Vec::<Stem>::with_capacity(wood_components.max(1));
        for part in &woody {
            let mut stem = wood_segment(part)?;
            if stems.is_empty() {
                if stem.start[1] > stem.end[1] {
                    std::mem::swap(&mut stem.start, &mut stem.end);
                }
            } else {
                let nearest = stems
                    .iter()
                    .enumerate()
                    .flat_map(|(id, parent)| {
                        [(false, stem.start), (true, stem.end)]
                            .map(move |(swap, point)| (distance2(point, parent), id, swap))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((distance, id, swap)) = nearest {
                    if swap {
                        std::mem::swap(&mut stem.start, &mut stem.end);
                    }
                    let margin = (stem.base_radius + stems[id].base_radius) * 1.5;
                    if distance <= margin * margin {
                        stem.parent = Some(u32::try_from(id).map_err(|_| Error::TooLarge)?);
                    }
                }
            }
            stems.push(stem);
        }
        if stems.is_empty() {
            let mut bounds: Option<Bounds> = None;
            for part in &leafy {
                match bounds.as_mut() {
                    Some(previous) => previous.union(part.bounds),
                    None => bounds = Some(part.bounds),
                }
            }
            let bounds = bounds.ok_or(Error::Invalid("no foliage components"))?;
            let center = bounds.center();
            let height = (bounds.max[1] - bounds.min[1]).max(0.01);
            let radius = ((bounds.max[0] - bounds.min[0]).max(bounds.max[2] - bounds.min[2])
                * 0.01)
                .max(0.001);
            stems.push(Stem {
                start: [center[0], (-0.5_f32).mul_add(height, center[1]), center[2]],
                end: [center[0], 0.5_f32.mul_add(height, center[1]), center[2]],
                base_radius: radius,
                tip_radius: radius,
                parent: None,
            });
        }
        let mut cards = Vec::with_capacity(leafy.len());
        let mut foliage = Vec::with_capacity(leafy.len());
        for part in leafy {
            let center = part.bounds.center();
            let nearest = stems
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| distance2(center, a).total_cmp(&distance2(center, b)))
                .ok_or(Error::Invalid("missing stem"))?
                .0;
            foliage.push(Foliage {
                center,
                radii: std::array::from_fn(|i| {
                    ((part.bounds.max[i] - part.bounds.min[i]) * 0.5).max(0.001)
                }),
                color: part.color,
                stem: u32::try_from(nearest).map_err(|_| Error::TooLarge)?,
            });
            cards.push(FoliageCard {
                surface: part.surface,
                triangles: part.triangles,
                bounds: part.bounds,
                area: part.area,
            });
        }
        let plant = Plant::from_parts(stems, foliage)?;
        let plant = match source_frame {
            Some(frame) => plant.with_source_frame(frame)?,
            None => plant,
        };
        Ok(Self {
            plant,
            cards,
            wood_components,
        })
    }
}
