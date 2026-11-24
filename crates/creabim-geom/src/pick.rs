//! Ray picking against scene geometry, for clicking elements in 3D.

use crate::SceneItem;
use creabim_core::ElementId;
use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub id: ElementId,
    /// Distance along the ray.
    pub t: f32,
    /// Which item of the scene was hit. An element can have several items
    /// (a terrain's surface and its skirt, a wall's layers), so the id
    /// alone does not say whose mesh `triangle` counts in.
    pub item: usize,
    /// Index of the hit triangle within that item's mesh.
    pub triangle: usize,
}

/// Closest element hit by a ray, using Möller–Trumbore intersection.
pub fn pick(scene: &[SceneItem], origin: Vec3, dir: Vec3) -> Option<Hit> {
    pick_where(scene, origin, dir, &|_| true)
}

/// [`pick`], of the elements `keep` takes only.
pub fn pick_where(
    scene: &[SceneItem],
    origin: Vec3,
    dir: Vec3,
    keep: &dyn Fn(ElementId) -> bool,
) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for (at, item) in scene.iter().enumerate().filter(|(_, i)| keep(i.id)) {
        let p = &item.mesh.positions;
        for (tri, idx) in item.mesh.indices.chunks(3).enumerate() {
            let (a, b, c) = (
                Vec3::from(p[idx[0] as usize]),
                Vec3::from(p[idx[1] as usize]),
                Vec3::from(p[idx[2] as usize]),
            );
            if let Some(t) = ray_triangle(origin, dir, a, b, c) {
                if best.is_none_or(|h| t < h.t) {
                    best = Some(Hit {
                        id: item.id,
                        t,
                        item: at,
                        triangle: tri,
                    });
                }
            }
        }
    }
    best
}

/// Every element a ray passes through, nearest first, each once with its
/// nearest hit: what is behind or inside the closest one, to cycle to.
pub fn pick_all(scene: &[SceneItem], origin: Vec3, dir: Vec3) -> Vec<Hit> {
    let mut best: Vec<Hit> = Vec::new();
    for (at, item) in scene.iter().enumerate() {
        let p = &item.mesh.positions;
        for (tri, idx) in item.mesh.indices.chunks(3).enumerate() {
            let (a, b, c) = (
                Vec3::from(p[idx[0] as usize]),
                Vec3::from(p[idx[1] as usize]),
                Vec3::from(p[idx[2] as usize]),
            );
            let Some(t) = ray_triangle(origin, dir, a, b, c) else {
                continue;
            };
            let hit = Hit {
                id: item.id,
                t,
                item: at,
                triangle: tri,
            };
            match best.iter_mut().find(|h| h.id == item.id) {
                Some(h) if t < h.t => *h = hit,
                Some(_) => {}
                None => best.push(hit),
            }
        }
    }
    best.sort_by(|a, b| a.t.total_cmp(&b.t));
    best
}

fn ray_triangle(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 1e-4).then_some(t)
}

/// The flat face a ray hits: the hit, where on it, its outward normal and
/// every triangle of the element's mesh in the same plane (for a highlight).
#[derive(Clone, Debug)]
pub struct FaceHit {
    pub hit: Hit,
    pub point: glam::DVec3,
    pub normal: glam::DVec3,
    pub triangles: Vec<[glam::DVec3; 3]>,
}

/// The face under a ray, as [`FaceHit`].
pub fn pick_face(scene: &[SceneItem], origin: Vec3, dir: Vec3) -> Option<FaceHit> {
    pick_face_where(scene, origin, dir, &|_| true)
}

/// The face of element `id` under a ray, even where others are in front
/// of it: what Tab moved the pre-selection to.
pub fn pick_face_of(
    scene: &[SceneItem],
    origin: Vec3,
    dir: Vec3,
    id: ElementId,
) -> Option<FaceHit> {
    pick_face_where(scene, origin, dir, &|i| i == id)
}

fn pick_face_where(
    scene: &[SceneItem],
    origin: Vec3,
    dir: Vec3,
    keep: &dyn Fn(ElementId) -> bool,
) -> Option<FaceHit> {
    let hit = pick_where(scene, origin, dir, keep)?;
    let m = &scene.get(hit.item)?.mesh;
    if hit.triangle >= m.triangle_count() {
        return None;
    }
    let tri = |t: usize| -> [glam::DVec3; 3] {
        let i = &m.indices[t * 3..t * 3 + 3];
        [0, 1, 2].map(|k| Vec3::from(m.positions[i[k] as usize]).as_dvec3())
    };
    let [a, b, c] = tri(hit.triangle);
    let mut n = (b - a).cross(c - a).normalize_or_zero();
    let stored = Vec3::from(m.normals[m.indices[hit.triangle * 3] as usize]).as_dvec3();
    if n.dot(stored) < 0.0 {
        n = -n;
    }
    let point = (origin + dir * hit.t).as_dvec3();
    let triangles = (0..m.triangle_count())
        .map(tri)
        .filter(|[p, q, r]| {
            let tn = (*q - *p).cross(*r - *p).normalize_or_zero();
            tn.dot(n) > 0.999 && [p, q, r].iter().all(|v| (**v - a).dot(n).abs() < 1e-4)
        })
        .collect();
    Some(FaceHit {
        hit,
        point,
        normal: n,
        triangles,
    })
}

/// Which polygon face of an edit mesh a triangle from
/// [`crate::scene::edit_mesh_to_tri`] belongs to.
pub fn face_of_triangle(mesh: &creabim_core::EditMesh, triangle: usize) -> Option<usize> {
    let mut seen = 0;
    for (f, face) in mesh.faces.iter().enumerate() {
        seen += face.len() - 2;
        if triangle < seen {
            return Some(f);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    #[test]
    fn ray_down_the_middle_hits_the_first_floor_slab() {
        let model = sample_house();
        let scene = crate::build_scene(&model);
        let hit = pick(&scene, Vec3::new(5.0, 3.0, 50.0), -Vec3::Z).unwrap();
        assert_eq!(model.get(hit.id).unwrap().name, "FF Slab");
        assert!((hit.t - 47.0).abs() < 1e-3);
    }

    #[test]
    fn ray_through_a_window_misses_the_wall() {
        let model = sample_house();
        let scene = crate::build_scene(&model);
        // GF wall 3 (y = 6) runs from x = 10 towards x = 0 with a window at
        // offset 1.5, so the opening spans x 7.3..8.5 and z 0.9..2.2. A ray
        // through it passes the wall and lands on the window panel.
        let hit = pick(&scene, Vec3::new(8.0, 20.0, 1.0), -Vec3::Y).unwrap();
        let name = &model.get(hit.id).unwrap().name;
        assert_eq!(name, "GF Window 3.1", "hit {name}");
    }

    #[test]
    fn a_ray_lists_everything_it_passes_nearest_first() {
        let model = sample_house();
        let scene = crate::build_scene(&model);
        let o = Vec3::new(5.0, 3.0, 50.0);
        let all = pick_all(&scene, o, -Vec3::Z);
        assert_eq!(
            all.first().map(|h| h.id),
            pick(&scene, o, -Vec3::Z).map(|h| h.id)
        );
        let names: Vec<&str> = all
            .iter()
            .map(|h| model.get(h.id).unwrap().name.as_str())
            .collect();
        assert!(names.contains(&"FF Slab") && names.len() >= 2, "{names:?}");
        assert!(all.windows(2).all(|w| w[0].t <= w[1].t));
        let mut ids: Vec<_> = all.iter().map(|h| h.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "each element once");
    }

    #[test]
    fn the_face_of_what_is_behind_can_be_picked() {
        let model = sample_house();
        let scene = crate::build_scene(&model);
        let o = Vec3::new(5.0, 3.0, 50.0);
        let all = pick_all(&scene, o, -Vec3::Z);
        let behind = all[1];
        let f = pick_face_of(&scene, o, -Vec3::Z, behind.id).unwrap();
        assert_eq!(f.hit.id, behind.id);
        assert!((f.hit.t - behind.t).abs() < 1e-4);
        // looked at from above, it is its top
        assert!(f.normal.z > 0.9, "{}", f.normal);
        assert!(!f.triangles.is_empty());
        assert_eq!(
            pick_face(&scene, o, -Vec3::Z).map(|f| f.hit.id),
            Some(all[0].id)
        );
    }

    /// An element drawn as two items: a small one first, then a larger one
    /// that the ray hits at a triangle the first does not have.
    fn two_items_of_one_element() -> (Vec<SceneItem>, ElementId) {
        let model = sample_house();
        let scene = crate::build_scene(&model);
        let slab = scene
            .iter()
            .find(|i| model.get(i.id).is_some_and(|e| e.name == "FF Slab"))
            .unwrap();
        let mut small = slab.clone();
        small.mesh.indices.truncate(3);
        // moved far off, so the ray can only hit the whole one
        for p in &mut small.mesh.positions {
            p[0] += 500.0;
        }
        (vec![small, slab.clone()], slab.id)
    }

    #[test]
    fn a_hit_names_the_item_it_landed_on_when_an_element_has_several() {
        let (scene, id) = two_items_of_one_element();
        let o = Vec3::new(5.0, 3.0, 50.0);
        let hit = pick(&scene, o, -Vec3::Z).unwrap();
        assert_eq!((hit.id, hit.item), (id, 1));
        // the triangle counts in that item's mesh, not in the first of the id
        assert!(hit.triangle < scene[hit.item].mesh.triangle_count());
        assert!(
            hit.triangle >= scene[0].mesh.triangle_count(),
            "the test must hit a triangle the first item lacks"
        );
        let all = pick_all(&scene, o, -Vec3::Z);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].item, 1);
        let face = pick_face(&scene, o, -Vec3::Z).unwrap();
        assert_eq!(face.hit.item, 1);
        assert!(face.normal.z > 0.9);
    }

    #[test]
    fn face_lookup_walks_fan_triangles() {
        let mut m = creabim_core::EditMesh::cuboid(glam::DVec3::ONE);
        m.faces.push(vec![0, 1, 2, 3, 4]); // a pentagon: 3 triangles
        assert_eq!(face_of_triangle(&m, 0), Some(0));
        assert_eq!(face_of_triangle(&m, 11), Some(5));
        assert_eq!(face_of_triangle(&m, 12), Some(6));
        assert_eq!(face_of_triangle(&m, 14), Some(6));
        assert_eq!(face_of_triangle(&m, 15), None);
    }
}
