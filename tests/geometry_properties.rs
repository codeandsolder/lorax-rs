use lorax_rs::{Foliage, Plant, ScreenLod, Species, Stem};

#[test]
fn generated_species_cross_seed_and_lod_matrix() -> Result<(), Box<dyn std::error::Error>> {
    for fanout in 1..=5 {
        for levels in 0..=3 {
            let species = Species {
                fanout,
                levels,
                ..Species::default()
            };
            for seed in 0..16_u64 {
                let plant = Plant::generate(&species, seed)?;
                let bounds = plant.bounds();
                let mut prev_triangles = 0;
                for projected in [0.5, 2.0, 8.0, 16.0, 32.0, 64.0, 256.0] {
                    let mesh =
                        plant.mesh(ScreenLod::new(projected, 0.75, 12.0)?, species.wood_color)?;
                    assert_eq!(mesh.source_bounds, bounds);
                    assert!(mesh.triangle_count() >= prev_triangles);
                    assert!(mesh.indices.len().is_multiple_of(3));
                    assert!(
                        mesh.indices
                            .iter()
                            .all(|&i| usize::try_from(i).is_ok_and(|i| i < mesh.vertices.len()))
                    );
                    for vertex in &mesh.vertices {
                        for axis in 0..3 {
                            let scale = 8.0 * f32::EPSILON * bounds.radius();
                            assert!(vertex.position[axis] >= bounds.min[axis] - scale);
                            assert!(vertex.position[axis] <= bounds.max[axis] + scale);
                        }
                        assert!(vertex.normal.iter().all(|v| v.is_finite()));
                        assert!(vertex.color[..3].iter().all(|v| v.is_finite() && *v >= 0.0));
                    }
                    prev_triangles = mesh.triangle_count();
                }
            }
        }
    }
    Ok(())
}

#[test]
fn arbitrary_multistem_foliage_graph_is_supported() -> Result<(), Box<dyn std::error::Error>> {
    let plant = Plant::from_parts(
        vec![
            Stem {
                start: [-1.0, 0.0, 0.0],
                end: [-1.0, 4.0, 0.0],
                base_radius: 0.25,
                tip_radius: 0.09,
                parent: None,
            },
            Stem {
                start: [1.0, 0.0, 0.0],
                end: [1.0, 3.0, 0.0],
                base_radius: 0.20,
                tip_radius: 0.07,
                parent: None,
            },
        ],
        vec![Foliage {
            center: [1.0, 3.0, 0.0],
            radii: [1.0; 3],
            color: [0.1, 0.4, 0.1, 1.0],
            stem: 1,
        }],
    )?;
    let mesh = plant.mesh(ScreenLod::new(12.0, 0.75, 12.0)?, [0.4, 0.2, 0.1, 1.0])?;
    assert!(mesh.triangle_count() >= 6 + 8);
    assert_eq!(mesh.source_bounds, plant.bounds());
    Ok(())
}

#[test]
fn rejects_invalid_parents_and_unrepresentable_geometry() {
    let stem = Stem {
        start: [0.0, 0.0, 0.0],
        end: [0.0, 1.0, 0.0],
        base_radius: 0.1,
        tip_radius: 0.05,
        parent: Some(0),
    };
    assert!(Plant::from_parts(vec![stem], vec![]).is_err());
    let parent = Stem {
        parent: None,
        ..stem
    };
    assert!(Plant::from_parts(vec![parent, stem], vec![]).is_ok());
    assert!(
        Plant::from_parts(
            vec![Stem {
                start: [f32::INFINITY; 3],
                ..parent
            }],
            vec![]
        )
        .is_err()
    );
    assert!(
        Plant::from_parts(
            vec![Stem {
                end: [0.0; 3],
                ..parent
            }],
            vec![]
        )
        .is_err()
    );
    assert!(
        Plant::from_parts(
            vec![parent],
            vec![Foliage {
                center: [0.0; 3],
                radii: [1.0; 3],
                color: [0.2; 4],
                stem: 5
            }]
        )
        .is_err()
    );
}
