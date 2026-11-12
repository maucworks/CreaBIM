//! NURBS kernel adapter: `geometrykit-core` wrapped for CreaBIM.
//!
//! G0 of the scripting/geometrykit track: a versioned dependency on the
//! published kernel plus glam-typed wrappers, so 0.x API churn lands here
//! and nowhere else. No behaviour change anywhere yet; consumers (curved
//! parts, section linework, the script host API) come later.

pub use geometrykit_core;

use glam::DVec3;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NurbsError {
    #[error("kernel rejected the input: {0}")]
    Kernel(String),
    #[error("degenerate input: {0}")]
    Degenerate(&'static str),
}

/// A tessellated surface: points, per-vertex normals, index triples.
pub struct TriMesh {
    pub verts: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub tris: Vec<[usize; 3]>,
}

/// Bilinear (degree 1 x 1) patch from a corner and two edge vectors.
pub fn plane_surface(
    origin: DVec3,
    u: DVec3,
    v: DVec3,
) -> Result<geometrykit_core::eval::Surface, NurbsError> {
    if u.length() <= 0.0 || v.length() <= 0.0 {
        return Err(NurbsError::Degenerate("edge vectors must be non-zero"));
    }
    // Row-major poles[i * nv + j]: i runs along u, j along v.
    let poles = vec![
        origin.to_array(),
        (origin + v).to_array(),
        (origin + u).to_array(),
        (origin + u + v).to_array(),
    ];
    Ok(geometrykit_core::eval::Surface::new(
        poles,
        2,
        2,
        None,
        1,
        1,
        &[0.0, 1.0],
        &[2, 2],
        &[0.0, 1.0],
        &[2, 2],
    ))
}

/// Tessellate a surface to tolerance; depths are G0 defaults (base 1,
/// max 8), tunable when a consumer needs them.
pub fn tessellate(
    surf: &geometrykit_core::eval::Surface,
    deflection: f64,
    angle_tol: f64,
) -> Result<TriMesh, NurbsError> {
    let mesh = geometrykit_core::tessellate::adaptive_mesh(surf, deflection, angle_tol, 8, 1)
        .map_err(NurbsError::Kernel)?;
    let verts = mesh
        .verts
        .chunks_exact(3)
        .map(|c| DVec3::new(c[0], c[1], c[2]))
        .collect();
    let normals = mesh
        .normals
        .chunks_exact(3)
        .map(|c| DVec3::new(c[0], c[1], c[2]))
        .collect();
    let tris = mesh
        .tris
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    Ok(TriMesh {
        verts,
        normals,
        tris,
    })
}

/// Cut a surface with a plane: one point list per chained contour.
pub fn section_chains(
    surf: &geometrykit_core::eval::Surface,
    origin: DVec3,
    normal: DVec3,
    nu: usize,
    nv: usize,
) -> Result<Vec<Vec<DVec3>>, NurbsError> {
    if normal.length() <= 0.0 {
        return Err(NurbsError::Degenerate("plane normal must be non-zero"));
    }
    let chains = geometrykit_core::section::section_plane(
        surf,
        origin.to_array(),
        normal.to_array(),
        nu,
        nv,
    )
    .map_err(NurbsError::Kernel)?;
    Ok(chains
        .into_iter()
        .map(|c| {
            c.pts
                .into_iter()
                .map(|p| DVec3::new(p[0], p[1], p[2]))
                .collect()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_plane() -> geometrykit_core::eval::Surface {
        plane_surface(DVec3::ZERO, DVec3::X * 2.0, DVec3::Y).unwrap()
    }

    #[test]
    fn plane_tessellation_stays_in_plane() {
        let mesh = tessellate(&unit_plane(), 1e-4, 1e-3).unwrap();
        assert!(!mesh.tris.is_empty());
        assert_eq!(mesh.verts.len(), mesh.normals.len());
        for (v, n) in mesh.verts.iter().zip(&mesh.normals) {
            assert!(v.z.abs() < 1e-9, "{v:?}");
            assert!((v.x >= 0.0) && (v.x <= 2.0) && (v.y >= 0.0) && (v.y <= 1.0));
            assert!((n - DVec3::Z).length() < 1e-9, "{n:?}");
        }
    }

    #[test]
    fn vertical_section_of_plane_lies_on_the_plane() {
        let chains = section_chains(&unit_plane(), DVec3::X, DVec3::X, 8, 8).unwrap();
        let n: usize = chains.iter().map(Vec::len).sum();
        assert!(n > 0);
        for chain in &chains {
            for p in chain {
                assert!((p.x - 1.0).abs() < 1e-6, "{p:?}");
                assert!(p.z.abs() < 1e-6, "{p:?}");
            }
        }
    }

    #[test]
    fn kernel_rejects_a_non_positive_deflection() {
        assert!(matches!(
            tessellate(&unit_plane(), 0.0, 1e-3),
            Err(NurbsError::Kernel(_))
        ));
    }
}
