//! Reads a real glTF/GLB file through the exchange reader.
//!
//! Gated: runs only when `CREABIM_GLB_TEST_FILE` points at a file, so
//! the suite stays hermetic while making real-world exports testable:
//!
//! ```sh
//! CREABIM_GLB_TEST_FILE=/path/to/model.glb cargo test -p creabim-exchange --test glb_read
//! ```

use std::path::Path;

#[test]
fn reads_real_glb() {
    let Some(path) = std::env::var_os("CREABIM_GLB_TEST_FILE") else {
        eprintln!("skipped: set CREABIM_GLB_TEST_FILE to a .glb/.gltf file");
        return;
    };
    let (incoming, materials) =
        creabim_exchange::gltf_read::read_file(Path::new(&path)).expect("glb should read");
    assert!(!incoming.is_empty(), "no objects read");
    for obj in &incoming {
        assert!(!obj.mesh.vertices.is_empty(), "{}: no vertices", obj.name);
        assert!(!obj.mesh.faces.is_empty(), "{}: no faces", obj.name);
        if let Some(class) = obj.hint.as_ref() {
            eprintln!(
                "  {} -> {:?} ({} tris)",
                obj.name,
                class,
                obj.mesh.faces.len()
            );
        }
    }
    eprintln!(
        "read {} object(s), {} material(s) from {}",
        incoming.len(),
        materials.len(),
        path.display()
    );
}
