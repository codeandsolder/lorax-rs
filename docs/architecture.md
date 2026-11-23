# Foliage representation and level of detail

## Stability

A Plant owns ordered stems and attached foliage. Its source bounds are computed
when constructed, then remain invariant as meshes are generated at differing
screen sizes. A source may be generated with a stable seed or fitted from
artist-authored geometry. The caller's canonical scatter ID, position, rotation,
scale, collisions and saved changes must never depend on screen LOD.

## Geometry

Stems are tapering cylinders. Radial side count is derived from an upper bound
on projected polygon sagitta error. Stems thinner than the screen-error budget
are omitted, except for root stems. Foliage is represented by conservative
octahedral volume proxies. Attached foliage is merged along the stem tree until
a group exceeds the configured projected patch diameter, at which point its
children are represented separately. The leaf volumes are not displaced to
arbitrary new positions during camera movement.

Each geometry LOD returns full source bounds independently of its simplified
triangle envelope. Shaders may use the bounds for consistent source-to-instance
normalization, conservative visibility and wind padding.

This is a first practical baseline, not a proof of optimal image error. The
octahedral foliage proxy should be superseded by relightable, coverage-filtered
material clusters and the renderer's directional impostor path for source art.

## Integration

i-love-proland has a public procedural adapter that maps normalized mesh
vertices to its existing GPU TreeVertex/TreeMesh format. It does not mix
canonical scatter with camera-dependent tessellation or generate a separate
model per individual scattered tree. The host can precompute shared species
tiers and instance them using existing GPU compacted LOD bins.

An eventual continuous GPU mesher must preserve this representation, bounds
and seed contract, then move evaluation of the parametric model to the GPU.
