//! Deterministic procedural vegetation, independent of the renderer and camera.
//!
//! The authored or generated stem/foliage hierarchy is immutable. A view chooses
//! an approximation of that hierarchy; it never changes the plant's identity.
//! Lod meshes retain the same model-space origin and conservative source bounds.

mod generation;
mod geometry;
mod lod;

pub use generation::Species;
pub use geometry::{Bounds, Error, Foliage, Mesh, Plant, Stem, Vertex};
pub use lod::ScreenLod;
