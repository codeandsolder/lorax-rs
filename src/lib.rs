//! Deterministic procedural vegetation, independent of the renderer and camera.
//!
//! The authored or generated stem/foliage hierarchy is immutable. A view chooses
//! an approximation of that hierarchy; it never changes the plant's identity.
//! Lod meshes retain the same model-space origin and conservative source bounds.

mod cluster;
mod conifer;
mod generation;
mod geometry;
mod lod;
mod modules;
mod source;

pub use conifer::WhorledConifer;
pub use generation::Species;
pub use geometry::{Bounds, Error, Foliage, Mesh, Plant, SourceFrame, Stem, Vertex};
pub use lod::ScreenLod;

pub use cluster::FoliageCluster;
pub use source::{FoliageCard, SourceFit, SourceSurface};

pub use modules::{Module, ModuleTier};
