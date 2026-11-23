# Source-fitted foliage and material-preserving LOD

This is a renderer-independent fitter, not a replacement for existing authored
textures, shaders, wind maps, or normals. The full KSP tree is the visual
reference. The source geometry belongs to the host.

## Input

Use SourceSurface with transformed model-space XYZ positions, triangle indices
and RGBA tint, separately for trunk/wood and foliage material surfaces.
Index-connected components are extracted independently *per material*. Source
vertex order is stable, and all component triangles remain addressable by their
original vertex indices: no leaf card is silently thrown away.

Wood triangle components are fitted by a principal-axis segment, ranked by
geometric area and linked to the nearest earlier segment where their fitted
radii overlap. The fitter supports multiple stems and nonwoody vegetation,
and never assumes a world vertical direction for lateral branches.

The foliage output contains:
- A stable Foliage node per original index-connected source component.
- A FoliageCard entry with the exact original triangle index triples,
  source material ordinal and original triangle-area measurement.
- The fitted stem attachment and original model-space component bounds.

The original renderer is responsible for source UV, alpha cutoff, wind, and
multi-material texture ownership. These are not recoverable from geometry
alone, so no uniform-color proxy is presented as equivalent to source art.

## Geometry LOD versus source normalization

A source-authored SourceFrame (center and sphere radius) is independent of the
axis-aligned geometric bounds. For actual Parallax oak, the source sphere is
approximately 12.452 metres, whereas the loose fitted AABB enclosing
branches and disconnected foliage has a much larger circumsphere. Do not use
AABB.radius() for the original renderer's source scaling.

A fitted branch cylinder may extend slightly outside the original source
sphere. Plant.required_culling_radius() returns a conservative radius for
generated geometry while Plant.source_frame() keeps the original transform
normalization invariant. This separation matters to GPU compaction.

## Adaptive spatial clustering

Plant.foliage_clusters(ScreenLod) refines the largest conservative spatial
error first, then stops at both the requested projected patch error and an
explicit maximum cluster count (default 32; adjustable). Every source foliage
component belongs to exactly one resulting group. The grouping is stable for a
given source and projected size, and can be used as a texture-preserving atlas
bake partition. Plant.mesh emits temporary octahedral color proxy geometry
for these clusters, but **does not preserve source material alpha coverage**.

For source-accurate rendering, bake each group from its exact source material
triangles/UVs into multiple angular views, attach depth and normals, and
render those source-textured cluster impostors instead of the monochrome
geometry proxy. Keep the original whole-tree impostor for smaller distances.

## Parallax oak (Acer fixture; original assets not committed)

The original stock OakTree middle tier has 179 trunk triangles in 22 connected
components and 727 leaf triangles in 430 connected components (297 quads and
133 lone triangles). This first fitter recovers all 22 + 430 components.

At 16-cluster budget and projected 0.75px woody chord error:

| Crown | Fitted triangles | Clusters |
|---|---:|---:|
| 32px, patch 24px | 148 | 16 |
| 64px, patch 24px | 222 | 16 |
| 80px, patch 24px | 278 | 16 |
| 128px, patch 24px | 310 | 16 |

These are geometric complexity measurements only, **not** appearance-equivalent
renderings or GPU FPS. Original 906-triangle middle LOD and 4-triangle authored
far LOD remain untouched until source-texture A/B tests approve a replacement.
