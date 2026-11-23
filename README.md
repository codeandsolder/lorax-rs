# lorax-rs

Procedural trees and foliage hierarchies for **i-love-proland**, independent of
wgpu or a particular game engine.

The representation is built once from deterministic seeds (or fitted from source
assets) and tessellated at a given projected-size/error target. Every LOD shares
the same physical bounds. Tree identities, collision and world placement remain
the caller's responsibility.

- Pure Rust, no external dependencies, no unsafe.
- Deterministic stems and foliage clusters, with explicit input and size validation.
- Conservative fixed envelopes; hierarchical canopy aggregation at coarser LOD.
- Screen-space radial stem tessellation and pruning of subpixel woody geometry.
- An authored-tree entry point using Plant::from_parts.
- Standalone mesh output with positions, normals, UVs and RGBA colors.
- Source-space coordinate normalization and GPU upload handled in i-love-proland.

This is a **new geometry generator**. It does not yet reconstruct the actual
Parallax OakTree skeleton or preserve its authored leaf textures; the existing
source assets remain the visual reference until that validation is completed.

See docs/architecture.md for the LOD and stability contract.

## Licensing

Dual-licensed under MIT or Apache-2.0, at your option. See LICENSE-MIT and
LICENSE-APACHE.
