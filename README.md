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

## Source-fitted tree LOD

Original models can be fitted without importing an engine-specific parser:
pass transformed material-specific XYZ positions and triangle indices in
SourceSurface slices. SourceFit::fit recovers connected woody segments and
every original foliage card, including its exact material and triangle indices.
Provide the source normalization center/radius as SourceFrame when available.
Plant.required_culling_radius() provides the separate conservative bound for
the fitted proxy geometry.

The same Plant can return spatially clustered foliage for multiple projected
sizes via foliage_clusters(ScreenLod). Clusters have a hard configurable
budget and stable original-card indices suitable for directional source-texture
impostor baking; octahedral meshes are diagnostic solid-color proxies, not
texture-preserving final art. See docs/source-fitting.md.

## Continuous integration

Every pull request, default-branch push and merge-group candidate runs the
fleet-pinned strict Rust source, formatting and Clippy policy, the duplicate-code
regression gate, and the complete Cargo test suite (including doctests) through
shared CI with mandatory sccache. The pinned toolchain is in
`rust-toolchain.toml`; production code is held to the strict lint policy while
ordinary test assertions are permitted. The shared Rust gate enforces the
72-hour release-age quarantine for newly introduced external registry crates.
