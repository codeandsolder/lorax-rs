use lorax_rs::{ScreenLod, SourceFit, SourceFrame, SourceSurface};

type AnyResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn original_cards_and_material_identities_survive_fitting() -> AnyResult {
    let wood_pos = [[0.0, 0.0, 0.0], [0.0, 5.0, 0.0], [0.2, 0.0, 0.0]];
    let leaf_pos = [
        [-2.0, 3.0, 0.0],
        [-1.0, 3.0, 0.0],
        [-1.0, 4.0, 0.0],
        [-2.0, 4.0, 0.0],
        [1.0, 3.0, 0.0],
        [2.0, 3.0, 0.0],
        [1.5, 4.0, 0.0],
    ];
    let wood = [SourceSurface {
        positions: &wood_pos,
        indices: &[0, 1, 2],
        color: [0.3, 0.2, 0.1, 1.0],
    }];
    let leaves = [SourceSurface {
        positions: &leaf_pos,
        indices: &[0, 1, 2, 0, 2, 3, 4, 5, 6],
        color: [0.1, 0.4, 0.1, 1.0],
    }];
    let fit = SourceFit::fit(&wood, &leaves, None)?;
    assert_eq!(fit.wood_components, 1);
    assert_eq!(fit.cards.len(), 2);
    assert_eq!(fit.cards[0].triangles, vec![[0, 1, 2], [0, 2, 3]]);
    assert_eq!(fit.cards[1].triangles, vec![[4, 5, 6]]);
    assert_eq!(
        fit.cards.iter().map(|c| c.triangles.len()).sum::<usize>(),
        3
    );
    assert_eq!(fit.plant.stems().len(), 1);
    assert_eq!(fit.plant.foliage().len(), 2);
    let a = fit
        .plant
        .mesh(ScreenLod::new(8.0, 0.75, 12.0)?, [0.3, 0.2, 0.1, 1.0])?;
    let b = fit
        .plant
        .mesh(ScreenLod::new(180.0, 0.75, 12.0)?, [0.3, 0.2, 0.1, 1.0])?;
    assert!(b.triangle_count() > a.triangle_count());
    assert_eq!(a.source_bounds, b.source_bounds);
    Ok(())
}

#[test]
fn leaf_only_and_empty_wood_are_valid() -> AnyResult {
    let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let leaves = [SourceSurface {
        positions: &vertices,
        indices: &[0, 1, 2],
        color: [0.2, 0.5, 0.1, 1.0],
    }];
    let shrub = SourceFit::fit(&[], &leaves, None)?;
    assert_eq!(shrub.plant.stems().len(), 1);
    assert_eq!(shrub.cards.len(), 1);
    assert!(
        SourceFit::fit(
            &[],
            &leaves,
            Some(SourceFrame {
                center: [0.0; 3],
                radius: 10.0
            })
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn fitting_rejects_malformed_surfaces_and_invalid_frames() {
    let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]];
    let src = SourceSurface {
        positions: &vertices,
        indices: &[0, 1, 2],
        color: [0.2, 0.5, 0.1, 1.0],
    };
    assert!(SourceFit::fit(&[], &[], None).is_err());
    assert!(
        SourceFit::fit(
            &[],
            &[SourceSurface {
                indices: &[0, 1],
                ..src
            }],
            None
        )
        .is_err()
    );
    assert!(
        SourceFit::fit(
            &[],
            &[SourceSurface {
                indices: &[0, 1, 9],
                ..src
            }],
            None
        )
        .is_err()
    );
    assert!(
        SourceFit::fit(
            &[],
            &[SourceSurface {
                color: [f32::NAN; 4],
                ..src
            }],
            None
        )
        .is_err()
    );
    assert!(
        SourceFit::fit(
            &[],
            &[src],
            Some(SourceFrame {
                center: [0.0; 3],
                radius: f32::NAN
            })
        )
        .is_err()
    );
}

#[test]
fn source_cards_are_partitioned_across_every_refined_lod() -> AnyResult {
    let vertices = (0_u16..96)
        .flat_map(|i| {
            let x = f32::from(i % 16);
            let z = f32::from(i / 16);
            [[x, 1.0, z], [x + 0.3, 1.0, z], [x, 1.3, z]]
        })
        .collect::<Vec<_>>();
    let indices = (0_u32..96)
        .flat_map(|id| [id * 3, id * 3 + 1, id * 3 + 2])
        .collect::<Vec<_>>();
    let leaf = [SourceSurface {
        positions: &vertices,
        indices: &indices,
        color: [0.1, 0.6, 0.2, 1.0],
    }];
    let fit = SourceFit::fit(&[], &leaf, None)?;
    assert_eq!(fit.cards.len(), 96);
    for crown in [4.0, 16.0, 32.0, 64.0, 128.0, 512.0] {
        let request = ScreenLod::new(crown, 0.75, 8.0)?.with_cluster_budget(16)?;
        let a = fit.plant.foliage_clusters(request)?;
        let b = fit.plant.foliage_clusters(request)?;
        assert_eq!(a, b);
        assert!(a.len() <= 16);
        let mut seen = a
            .into_iter()
            .flat_map(|cluster| cluster.indices)
            .collect::<Vec<_>>();
        seen.sort_unstable();
        assert_eq!(seen, (0_u32..96).collect::<Vec<_>>());
    }
    Ok(())
}

#[test]
fn original_frame_is_preserved_separately_from_fitted_culling() -> AnyResult {
    let positions = [[0.0, 0.0, 0.0], [0.2, 10.0, 0.0], [0.2, 0.0, 0.0]];
    let trunk = [SourceSurface {
        positions: &positions,
        indices: &[0, 1, 2],
        color: [0.4, 0.25, 0.1, 1.0],
    }];
    let frame = SourceFrame::new([0.0, 5.0, 0.0], 5.1)?;
    let fit = SourceFit::fit(&trunk, &[], Some(frame))?;
    assert_eq!(fit.plant.source_frame(), frame);
    assert!(fit.plant.required_culling_radius() >= frame.radius);
    let near = fit
        .plant
        .mesh(ScreenLod::new(140.0, 0.5, 15.0)?, [0.4, 0.25, 0.1, 1.0])?;
    let far = fit
        .plant
        .mesh(ScreenLod::new(10.0, 0.5, 15.0)?, [0.4, 0.25, 0.1, 1.0])?;
    assert_eq!(near.source_frame, frame);
    assert_eq!(far.source_frame, frame);
    Ok(())
}
