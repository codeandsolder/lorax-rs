use std::fmt;

/// An invalid model, non-finite input, or exhausted representation budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Invalid(&'static str),
    TooLarge,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => write!(f, "invalid procedural foliage: {message}"),
            Self::TooLarge => write!(
                f,
                "procedural foliage exceeds representable geometry budget"
            ),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stem {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub base_radius: f32,
    pub tip_radius: f32,
    /// A parent must precede its children. None permits multi-stem plants.
    pub parent: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foliage {
    pub center: [f32; 3],
    pub radii: [f32; 3],
    pub color: [f32; 4],
    pub stem: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Bounds {
    #[must_use = "Use the calculated plant value"]
    pub fn center(self) -> [f32; 3] {
        std::array::from_fn(|axis| 0.5_f32.mul_add(self.max[axis], 0.5 * self.min[axis]))
    }

    #[must_use = "Use the calculated plant value"]
    pub fn radius(self) -> f32 {
        let center = self.center();
        let half = std::array::from_fn::<_, 3, _>(|axis| {
            (self.max[axis] - center[axis]).max(center[axis] - self.min[axis])
        });
        half.into_iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    #[must_use = "Use the calculated plant value"]
    pub fn diameter(self) -> f32 {
        self.radius() * 2.0
    }

    pub fn union(&mut self, rhs: Self) {
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(rhs.min[axis]);
            self.max[axis] = self.max[axis].max(rhs.max[axis]);
        }
    }

    #[must_use = "Use the calculated plant value"]
    pub fn around(center: [f32; 3], radii: [f32; 3]) -> Self {
        Self {
            min: std::array::from_fn(|axis| center[axis] - radii[axis]),
            max: std::array::from_fn(|axis| center[axis] + radii[axis]),
        }
    }

    #[must_use = "Use the calculated plant value"]
    pub fn valid(self) -> bool {
        self.min
            .iter()
            .zip(self.max.iter())
            .all(|(a, b)| a.is_finite() && b.is_finite() && a <= b)
            && self.radius().is_finite()
            && self.diameter() > 0.0
    }
}

/// The authored normalization frame is independent of the loose AABB.
///
/// The same camera-derived projected diameter must correspond to the source
/// sphere even if a fitted approximation extends slightly beyond it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceFrame {
    pub center: [f32; 3],
    pub radius: f32,
}

impl SourceFrame {
    /// # Errors
    /// Rejects invalid center coordinates and degenerate/non-finite radius.
    pub fn new(center: [f32; 3], radius: f32) -> Result<Self> {
        if !valid3(center)
            || !radius.is_finite()
            || radius <= 0.0
            || !radius.mul_add(2.0, 0.0).is_finite()
        {
            return Err(Error::Invalid("source normalization frame"));
        }
        Ok(Self { center, radius })
    }

    #[must_use = "Use the model-space normalization diameter"]
    pub fn diameter(self) -> f32 {
        self.radius * 2.0
    }
}

/// A local-space branch graph with terminal foliage and a fixed source envelope.
///
/// The graph may come from procedural rules or an authored asset fitted to stems.
/// Source coordinates never depend on the camera, and LOD meshes all share bounds.
#[derive(Clone, Debug, PartialEq)]
pub struct Plant {
    stems: Vec<Stem>,
    foliage: Vec<Foliage>,
    bounds: Bounds,
    frame: SourceFrame,
}

impl Plant {
    /// Construct any finite, ordered stem forest, not only generated trees.
    ///
    /// # Errors
    /// Requires non-degenerate finite stems, positive finite foliage radii,
    /// valid colors, earlier parent indices and valid foliage attachments.
    pub fn from_parts(stems: Vec<Stem>, foliage: Vec<Foliage>) -> Result<Self> {
        if stems.is_empty() {
            return Err(Error::Invalid("missing stems"));
        }
        let mut bounds: Option<Bounds> = None;
        for (index, stem) in stems.iter().enumerate() {
            if stem
                .parent
                .is_some_and(|p| usize::try_from(p).map_or(true, |p| p >= index))
                || !valid3(stem.start)
                || !valid3(stem.end)
                || !stem.base_radius.is_finite()
                || !stem.tip_radius.is_finite()
                || stem.base_radius <= 0.0
                || stem.tip_radius <= 0.0
                || dot(sub(stem.end, stem.start), sub(stem.end, stem.start)) <= f32::MIN_POSITIVE
            {
                return Err(Error::Invalid("stem geometry or parent"));
            }
            let extent = stem.base_radius.max(stem.tip_radius);
            for endpoint in [stem.start, stem.end] {
                merge(&mut bounds, Bounds::around(endpoint, [extent; 3]));
            }
        }
        for leaf in &foliage {
            if usize::try_from(leaf.stem).map_or(true, |i| i >= stems.len())
                || !valid3(leaf.center)
                || !leaf.radii.iter().all(|v| v.is_finite() && *v > 0.0)
                || !leaf.color[..3].iter().all(|v| v.is_finite() && *v >= 0.0)
                || !leaf.color[3].is_finite()
                || !(0.0..=1.0).contains(&leaf.color[3])
            {
                return Err(Error::Invalid("foliage geometry or attachment"));
            }
            merge(&mut bounds, Bounds::around(leaf.center, leaf.radii));
        }
        let bounds = bounds.ok_or(Error::Invalid("empty bounds"))?;
        if !bounds.valid() {
            return Err(Error::Invalid("unrepresentable source bounds"));
        }
        // Reserve u32::MAX for checked index overflow in the mesher.
        if stems.len() > u32::MAX as usize || foliage.len() > u32::MAX as usize {
            return Err(Error::TooLarge);
        }
        Ok(Self {
            stems,
            foliage,
            bounds,
            frame: SourceFrame::new(bounds.center(), bounds.radius())?,
        })
    }

    /// Override the authored normalization sphere without changing geometry.
    ///
    /// Fitted proxies may protrude slightly beyond the source art; culling
    /// must use `required_culling_radius`, not assume this radius is conservative.
    ///
    /// # Errors
    /// Rejects invalid model-space frames.
    pub fn with_source_frame(mut self, frame: SourceFrame) -> Result<Self> {
        self.frame = SourceFrame::new(frame.center, frame.radius)?;
        Ok(self)
    }

    #[must_use = "Use a single normalization frame for every LOD"]
    pub const fn source_frame(&self) -> SourceFrame {
        self.frame
    }

    /// Conservative source-space culling radius for the fitted geometry.
    /// It is deliberately distinct from the original authored normalization
    /// radius, so a coarse branch cylinder cannot corrupt instance scales.
    #[must_use = "Apply this bound when culling generated procedural meshes"]
    pub fn required_culling_radius(&self) -> f32 {
        let center = self.frame.center;
        let mut radius = self.frame.radius;
        for stem in &self.stems {
            let thickness = stem.base_radius.max(stem.tip_radius);
            for endpoint in [stem.start, stem.end] {
                radius = radius
                    .max(dot(sub(endpoint, center), sub(endpoint, center)).sqrt() + thickness);
            }
        }
        for leaf in &self.foliage {
            let displacement = sub(leaf.center, center);
            for axis in 0..3 {
                for direction in [-1.0_f32, 1.0_f32] {
                    let mut vertex = displacement;
                    vertex[axis] = direction.mul_add(leaf.radii[axis], vertex[axis]);
                    radius = radius.max(dot(vertex, vertex).sqrt());
                }
            }
        }
        radius
    }

    #[must_use = "Use the source stems"]
    pub fn stems(&self) -> &[Stem] {
        &self.stems
    }

    #[must_use = "Use the calculated plant value"]
    pub fn foliage(&self) -> &[Foliage] {
        &self.foliage
    }

    #[must_use = "Use the calculated plant value"]
    pub const fn bounds(&self) -> Bounds {
        self.bounds
    }
}

pub fn valid3(v: [f32; 3]) -> bool {
    v.into_iter().all(f32::is_finite)
}

fn merge(existing: &mut Option<Bounds>, next: Bounds) {
    match existing {
        Some(value) => value.union(next),
        None => *existing = Some(next),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    /// Original plant-space bounds, identical across every LOD.
    pub source_bounds: Bounds,
    pub source_frame: SourceFrame,
}

impl Mesh {
    #[must_use = "Use the calculated plant value"]
    pub const fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

pub fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
pub fn mul(a: [f32; 3], scale: f32) -> [f32; 3] {
    a.map(|v| v * scale)
}
pub fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}
pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -a[2] * b[1]),
        a[2].mul_add(b[0], -a[0] * b[2]),
        a[0].mul_add(b[1], -a[1] * b[0]),
    ]
}
pub fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    mul(v, 1.0 / length)
}
