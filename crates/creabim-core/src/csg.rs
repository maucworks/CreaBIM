//! Boolean operations on closed polygon meshes (union, difference,
//! intersection) with BSP trees, after Evan Wallace's csg.js. Architectural
//! solids have few faces, so the simple, robust BSP approach is fast enough
//! and needs no exact arithmetic.

use glam::DVec3;

/// csg.js's tolerance, and for the same reason: scene meshes carry f32
/// noise of about 1e-6 (a metre-scale coordinate has no finer f32), so
/// faces that are coincident in fact — every wall top level with a slab
/// top, every stacked layer — must still classify as coplanar. Tighter,
/// and they graze instead: each cut then breeds two full-size copies of
/// the face, which multiply at every further cut (a scene grew five
/// million triangles off ten wall cuts this way).
const EPS: f64 = 1e-5;

/// Pieces thinner than this (mean width, twice the area over the
/// perimeter) are numerical noise from grazing splits, not geometry:
/// dropped, or they breed slivers that multiply at every further cut
/// (a wall brushing a slab once grew a scene's booleans into millions
/// of triangles and hung the 3D view).
const THIN: f64 = 1e-5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
}

impl BoolOp {
    pub const ALL: [BoolOp; 3] = [BoolOp::Difference, BoolOp::Union, BoolOp::Intersection];

    pub fn label(self) -> &'static str {
        match self {
            BoolOp::Union => "Union",
            BoolOp::Difference => "Subtract",
            BoolOp::Intersection => "Intersect",
        }
    }
}

/// A convex or concave planar polygon, its corners counter-clockwise seen
/// from outside.
#[derive(Clone, Debug)]
pub struct Polygon {
    pub points: Vec<DVec3>,
    plane: Plane,
}

/// Mean width of a polygon: twice its area over its perimeter (the
/// Newell sum is twice the area). Thin by this measure whatever its
/// length, so metre-long grazing slivers are caught too.
pub fn thinness(points: &[DVec3]) -> f64 {
    let mut n = DVec3::ZERO;
    let mut per = 0.0;
    for i in 0..points.len() {
        let (a, b) = (points[i], points[(i + 1) % points.len()]);
        n += DVec3::new(
            (a.y - b.y) * (a.z + b.z),
            (a.z - b.z) * (a.x + b.x),
            (a.x - b.x) * (a.y + b.y),
        );
        per += a.distance(b);
    }
    if per <= 0.0 {
        return 0.0;
    }
    n.length() / per
}

#[derive(Clone, Copy, Debug)]
struct Plane {
    n: DVec3,
    w: f64,
}

impl Plane {
    fn from_points(p: &[DVec3]) -> Option<Plane> {
        // Newell's method copes with slightly non-planar and concave faces
        let mut n = DVec3::ZERO;
        for i in 0..p.len() {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            n += DVec3::new(
                (a.y - b.y) * (a.z + b.z),
                (a.z - b.z) * (a.x + b.x),
                (a.x - b.x) * (a.y + b.y),
            );
        }
        let n = n.try_normalize()?;
        let c = p.iter().copied().sum::<DVec3>() / p.len() as f64;
        Some(Plane { n, w: n.dot(c) })
    }

    fn flip(&mut self) {
        self.n = -self.n;
        self.w = -self.w;
    }

    /// Put `poly` (or its pieces) into the right lists.
    fn split(
        &self,
        poly: &Polygon,
        coplanar_front: &mut Vec<Polygon>,
        coplanar_back: &mut Vec<Polygon>,
        front: &mut Vec<Polygon>,
        back: &mut Vec<Polygon>,
    ) {
        const COPLANAR: u8 = 0;
        const FRONT: u8 = 1;
        const BACK: u8 = 2;
        let mut kind = 0u8;
        let types: Vec<u8> = poly
            .points
            .iter()
            .map(|p| {
                let t = self.n.dot(*p) - self.w;
                let k = if t < -EPS {
                    BACK
                } else if t > EPS {
                    FRONT
                } else {
                    COPLANAR
                };
                kind |= k;
                k
            })
            .collect();
        match kind {
            COPLANAR => {
                if self.n.dot(poly.plane.n) > 0.0 {
                    coplanar_front.push(poly.clone());
                } else {
                    coplanar_back.push(poly.clone());
                }
            }
            FRONT => front.push(poly.clone()),
            BACK => back.push(poly.clone()),
            _ => {
                let (mut f, mut b) = (Vec::new(), Vec::new());
                let n = poly.points.len();
                for i in 0..n {
                    let j = (i + 1) % n;
                    let (ti, tj) = (types[i], types[j]);
                    let (vi, vj) = (poly.points[i], poly.points[j]);
                    if ti != BACK {
                        f.push(vi);
                    }
                    if ti != FRONT {
                        b.push(vi);
                    }
                    if (ti | tj) == (FRONT | BACK) {
                        let t = (self.w - self.n.dot(vi)) / self.n.dot(vj - vi);
                        let v = vi + (vj - vi) * t;
                        f.push(v);
                        b.push(v);
                    }
                }
                // a piece thinner than THIN is numerical noise: dropped,
                // or slivers breed through every further cut
                if f.len() >= 3 && thinness(&f) > THIN {
                    front.push(Polygon {
                        points: f,
                        plane: poly.plane,
                    });
                }
                if b.len() >= 3 && thinness(&b) > THIN {
                    back.push(Polygon {
                        points: b,
                        plane: poly.plane,
                    });
                }
            }
        }
    }
}

impl Polygon {
    pub fn new(points: Vec<DVec3>) -> Option<Polygon> {
        if points.len() < 3 {
            return None;
        }
        // degenerate slivers poison the tree: a graze splits them without
        // end (MU39's crash, met again as a hang)
        if thinness(&points) <= THIN {
            return None;
        }
        let plane = Plane::from_points(&points)?;
        Some(Polygon { points, plane })
    }

    fn flip(&mut self) {
        self.points.reverse();
        self.plane.flip();
    }
}

#[derive(Default)]
struct Node {
    plane: Option<Plane>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
    polygons: Vec<Polygon>,
}

impl Node {
    fn new(polys: Vec<Polygon>) -> Node {
        let mut n = Node::default();
        n.build(polys);
        n
    }

    fn invert(&mut self) {
        for p in &mut self.polygons {
            p.flip();
        }
        if let Some(pl) = &mut self.plane {
            pl.flip();
        }
        if let Some(f) = &mut self.front {
            f.invert();
        }
        if let Some(b) = &mut self.back {
            b.invert();
        }
        std::mem::swap(&mut self.front, &mut self.back);
    }

    /// The parts of `polys` outside this solid.
    fn clip_polygons(&self, polys: Vec<Polygon>) -> Vec<Polygon> {
        let Some(plane) = self.plane else {
            return polys;
        };
        let (mut front, mut back) = (Vec::new(), Vec::new());
        for p in &polys {
            let (mut cf, mut cb) = (Vec::new(), Vec::new());
            plane.split(p, &mut cf, &mut cb, &mut front, &mut back);
            front.extend(cf);
            back.extend(cb);
        }
        let mut front = match &self.front {
            Some(f) => f.clip_polygons(front),
            None => front,
        };
        let back = match &self.back {
            Some(b) => b.clip_polygons(back),
            None => vec![],
        };
        front.extend(back);
        front
    }

    fn clip_to(&mut self, other: &Node) {
        self.polygons = other.clip_polygons(std::mem::take(&mut self.polygons));
        if let Some(f) = &mut self.front {
            f.clip_to(other);
        }
        if let Some(b) = &mut self.back {
            b.clip_to(other);
        }
    }

    fn all_polygons(&self) -> Vec<Polygon> {
        let mut out = self.polygons.clone();
        if let Some(f) = &self.front {
            out.extend(f.all_polygons());
        }
        if let Some(b) = &self.back {
            out.extend(b.all_polygons());
        }
        out
    }

    fn build(&mut self, polys: Vec<Polygon>) {
        self.build_at(polys, 0);
    }

    /// As [`Node::build`], `depth` deep: past a depth no sound solid
    /// needs, what is left stays in this node rather than recursing on
    /// until the stack runs out.
    fn build_at(&mut self, polys: Vec<Polygon>, depth: usize) {
        if polys.is_empty() {
            return;
        }
        if depth > 600 {
            self.polygons.extend(polys);
            return;
        }
        // a fresh node splits by the plane that cuts the set most evenly:
        // the first polygon's plane leans the tree into a long spine, and
        // clipping against a spine shatters cutters into slivers
        let plane = *self.plane.get_or_insert(Self::balanced(&polys));
        let (mut front, mut back) = (Vec::new(), Vec::new());
        let (mut cf, mut cb) = (Vec::new(), Vec::new());
        for p in &polys {
            plane.split(p, &mut cf, &mut cb, &mut front, &mut back);
        }
        self.polygons.extend(cf);
        self.polygons.extend(cb);
        if !front.is_empty() {
            self.front
                .get_or_insert_with(Default::default)
                .build_at(front, depth + 1);
        }
        if !back.is_empty() {
            self.back
                .get_or_insert_with(Default::default)
                .build_at(back, depth + 1);
        }
    }

    /// Of a few spread candidate planes, the one that cuts `polys` most
    /// evenly: a straddling polygon feeds both sides, so it counts
    /// against the balance too.
    fn balanced(polys: &[Polygon]) -> Plane {
        if polys.len() <= 8 {
            return polys[0].plane;
        }
        let mut best = polys[0].plane;
        let mut best_score = usize::MAX;
        for k in 0..8 {
            let plane = polys[k * (polys.len() - 1) / 7].plane;
            let (mut f, mut b) = (0usize, 0usize);
            for p in polys {
                let mut kind = 0u8;
                for pt in &p.points {
                    let t = plane.n.dot(*pt) - plane.w;
                    kind |= if t < -EPS {
                        2
                    } else if t > EPS {
                        1
                    } else {
                        0
                    };
                }
                match kind {
                    1 => f += 1,
                    2 => b += 1,
                    3 => {
                        f += 1;
                        b += 1;
                    }
                    _ => {}
                }
            }
            let score = f.abs_diff(b);
            if score < best_score {
                best_score = score;
                best = plane;
            }
        }
        best
    }
}

/// `a op b`, both closed and outward-facing.
pub fn boolean(a: Vec<Polygon>, b: Vec<Polygon>, op: BoolOp) -> Vec<Polygon> {
    let mut a = Node::new(a);
    let mut b = Node::new(b);
    match op {
        BoolOp::Union => {
            a.clip_to(&b);
            b.clip_to(&a);
            b.invert();
            b.clip_to(&a);
            b.invert();
            a.build(b.all_polygons());
            a.all_polygons()
        }
        BoolOp::Difference => {
            a.invert();
            a.clip_to(&b);
            b.clip_to(&a);
            b.invert();
            b.clip_to(&a);
            b.invert();
            a.build(b.all_polygons());
            a.invert();
            a.all_polygons()
        }
        BoolOp::Intersection => {
            a.invert();
            b.clip_to(&a);
            b.invert();
            a.clip_to(&b);
            b.clip_to(&a);
            a.build(b.all_polygons());
            a.invert();
            a.all_polygons()
        }
    }
}

/// Polygons of an edit mesh, moved by `offset`.
pub fn from_mesh(m: &crate::EditMesh, offset: DVec3) -> Vec<Polygon> {
    let mut out = vec![];
    for f in &m.faces {
        let pts: Vec<DVec3> = f.iter().map(|&v| m.vertices[v as usize] + offset).collect();
        let Some(p) = Polygon::new(pts.clone()) else {
            continue;
        };
        // a face no longer flat (a corner dragged off its plane) goes in as
        // triangles, which always are: cut as one, it split without end
        // and the stack ran out (MU39 crash)
        let bent = pts
            .iter()
            .any(|q| (p.plane.n.dot(*q) - p.plane.w).abs() > 1e-7);
        if bent && pts.len() > 3 {
            for i in 1..pts.len() - 1 {
                out.extend(Polygon::new(vec![pts[0], pts[i], pts[i + 1]]));
            }
        } else {
            out.push(p);
        }
    }
    out
}

/// An edit mesh from polygons, welding corners that coincide.
pub fn to_mesh(polys: &[Polygon], offset: DVec3) -> crate::EditMesh {
    use std::collections::HashMap;
    let mut m = crate::EditMesh::default();
    let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
    // faces seen already, by their corner set: a boolean of solids that
    // touch keeps both solids' faces where they are coplanar, and a
    // coincident pair rides into the next boolean, which splits each
    // against the other — a wall level with a slab grew five million
    // triangles that way
    let mut seen: std::collections::HashSet<Vec<u32>> = std::collections::HashSet::new();
    let key = |p: DVec3| {
        (
            (p.x * 1e6).round() as i64,
            (p.y * 1e6).round() as i64,
            (p.z * 1e6).round() as i64,
        )
    };
    for p in polys {
        let mut face: Vec<u32> = Vec::new();
        for &q in &p.points {
            let id = *ids.entry(key(q)).or_insert_with(|| {
                m.vertices.push(q - offset);
                m.vertices.len() as u32 - 1
            });
            if face.last() != Some(&id) && face.first() != Some(&id) || face.is_empty() {
                face.push(id);
            }
        }
        if face.len() >= 3 {
            // the same corners in the same turn is the face again
            let mut set = face.clone();
            let min = *set.iter().min().unwrap();
            let at = set.iter().position(|&v| v == min).unwrap();
            set.rotate_left(at);
            if seen.insert(set) {
                m.faces.push(face);
            }
        }
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditMesh;

    fn cube(size: f64, at: DVec3) -> Vec<Polygon> {
        from_mesh(&EditMesh::cuboid(DVec3::splat(size)), at)
    }

    fn volume(p: &[Polygon]) -> f64 {
        to_mesh(p, DVec3::ZERO).volume()
    }

    #[test]
    fn cutting_a_cube_out_of_a_cube_removes_its_volume() {
        let a = cube(2.0, DVec3::ZERO);
        let b = cube(1.0, DVec3::new(1.0, 0.5, 0.5));
        // half of b is inside a
        let d = boolean(a.clone(), b.clone(), BoolOp::Difference);
        assert!((volume(&d) - (8.0 - 0.5)).abs() < 1e-6, "{}", volume(&d));
        let u = boolean(a.clone(), b.clone(), BoolOp::Union);
        assert!((volume(&u) - 8.5).abs() < 1e-6);
        let i = boolean(a, b, BoolOp::Intersection);
        assert!((volume(&i) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_hole_through_a_wall() {
        let wall = from_mesh(&EditMesh::cuboid(DVec3::new(4.0, 0.2, 3.0)), DVec3::ZERO);
        let hole = from_mesh(
            &EditMesh::cuboid(DVec3::new(1.0, 1.0, 1.0)),
            DVec3::new(1.5, -0.4, 1.0),
        );
        let d = boolean(wall, hole, BoolOp::Difference);
        assert!((volume(&d) - (4.0 * 0.2 * 3.0 - 0.2)).abs() < 1e-6);
    }

    // The script-walls hang: walls whose top face is coplanar with the
    // slab's top face, punched one after another into the same slab. Each
    // cut adds a notch's worth of faces (about nine here) and no more.
    #[test]
    fn repeated_wall_cuts_do_not_explode() {
        // the FF slab of the demo: 10×6, 0.3 thick, top at z=3.0
        let slab = from_mesh(
            &EditMesh::cuboid(DVec3::new(10.0, 6.0, 0.3)),
            DVec3::new(5.0, 3.0, 2.85),
        );
        let slab_tris = to_mesh(&slab, DVec3::ZERO).faces.len();
        let mut current = slab;
        // ten script walls: 0.1 thick, full level height (top face z=3.0
        // coplanar with the slab's top face), one every 0.889 m
        for i in 0..10 {
            let x = i as f64 * 8.0 / 9.0;
            let wall = from_mesh(
                &EditMesh::cuboid(DVec3::new(0.1, 4.0, 3.0)),
                DVec3::new(x, 2.0, 1.5),
            );
            current = boolean(current, wall, BoolOp::Difference);
            let n = to_mesh(&current, DVec3::ZERO).faces.len();
            // a clean cut adds a bounded few faces; sliver breeding runs
            // away by times four and more per cut
            assert!(
                n <= slab_tris + 20 * (i + 1),
                "cut {i} blew up: {slab_tris} -> {n}"
            );
        }
    }
}
