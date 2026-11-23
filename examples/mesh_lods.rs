use lorax_rs::{Plant, ScreenLod, Species};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let oak = Species::default();
    let tree = Plant::generate(&oak, 0x5eed)?;
    for diameter in [4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0] {
        let mesh = tree.mesh(ScreenLod::new(diameter, 0.75, 12.0)?, oak.wood_color)?;
        println!(
            "{diameter:>5.0} px: {:>5} triangles, {:>5} vertices",
            mesh.triangle_count(),
            mesh.vertices.len()
        );
    }
    Ok(())
}
