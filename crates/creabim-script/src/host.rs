//! Host API stub (M-S0): one `creabim_host` module proving the
//! Rust → Python → Rust loop. The full element vocabulary comes later.

use creabim_core::EditMesh;
use glam::DVec3;
use pyo3::prelude::*;

#[pymodule]
mod creabim_host {
    use super::*;

    /// Vertices and face loops of an axis-aligned cuboid.
    #[pyfunction]
    fn cuboid(size: (f64, f64, f64)) -> (Vec<[f64; 3]>, Vec<Vec<u32>>) {
        let mesh = EditMesh::cuboid(DVec3::new(size.0, size.1, size.2));
        let verts = mesh.vertices.iter().map(DVec3::to_array).collect();
        (verts, mesh.faces)
    }
}

/// Register before first initialization (panics once running).
pub(crate) fn register() {
    pyo3::append_to_inittab!(creabim_host);
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{Document, Element, ElementKind, FreeMesh, Model, Transaction};
    use std::time::Instant;

    #[test]
    fn scripted_cuboid_round_trip() {
        let home = crate::ensure_provisioned().expect("provision the bundle");
        creabim_provision::ensure_packages(&["numpy"]).expect("install numpy");
        let t0 = Instant::now();
        crate::ensure_initialized(&home).expect("boot the interpreter");
        let (verts, faces): (Vec<Vec<f64>>, Vec<Vec<u32>>) = Python::attach(|py| {
            py.run(
                c"import numpy as np
import creabim_host as ch
v, f = ch.cuboid((2.0, 1.0, 0.5))
a = np.asarray(v, dtype=float) * 2.0
a[:, 2] += 0.25",
                None,
                None,
            )
            .expect("run the script");
            py.eval(c"(a.tolist(), f)", None, None)
                .expect("read the result")
                .extract()
                .expect("convert the result")
        });
        assert_eq!(verts.len(), 8);
        // Doubled footprint, lifted base: x in [-2, 2], z from 0.25.
        assert!((verts[0][0] + 2.0).abs() < 1e-9);
        assert!((verts[0][2] - 0.25).abs() < 1e-9);
        let mesh = EditMesh {
            vertices: verts
                .into_iter()
                .map(|p| DVec3::new(p[0], p[1], p[2]))
                .collect(),
            faces,
        };
        mesh.validate().expect("rebuilt mesh is valid");
        let element = Element::new(
            "script-cuboid",
            ElementKind::FreeMesh(FreeMesh::new(None, DVec3::ZERO, mesh)),
        );
        let mut doc = Document::from_model(Model::new());
        doc.commit(Transaction::new("script:cuboid").insert(element))
            .expect("commit the ops");
        assert_eq!(doc.model().iter().count(), 1);
        println!("round trip: {:?}", t0.elapsed());
    }
}
