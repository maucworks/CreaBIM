//! IFC geometry to meshes: placements, curves, profiles and the solid and
//! surface representations IFC files use in practice.
//!
//! Everything here works in metres and world coordinates: lengths are scaled
//! by the file's length unit as they are read, and every item is
//! transformed by the matrix its caller passes in.

use crate::step_read::{Entity, StepFile, Value};
use crate::tess::{add_face, area2, clean_loop};
use creabim_core::EditMesh;
use glam::{DMat3, DMat4, DVec2, DVec3};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::f64::consts::{PI, TAU};

/// Warnings, grouped by message so a file with 10 000 unsupported items
/// reports one line, not 10 000; those about one element also go in a
/// detailed list naming it, so each can be found and fixed.
#[derive(Debug, Default)]
pub(crate) struct Warnings {
    groups: IndexMap<String, usize>,
    details: Vec<String>,
}

impl Warnings {
    pub fn add(&mut self, msg: impl Into<String>) {
        *self.groups.entry(msg.into()).or_insert(0) += 1;
    }

    /// A note about one element: counted under `group`, and listed with
    /// `detail`, which names the element and says why.
    pub fn element(&mut self, group: impl Into<String>, detail: impl Into<String>) {
        self.add(group);
        self.details.push(detail.into());
    }

    /// The grouped summary, then the detailed list.
    pub fn into_parts(self) -> (Vec<String>, Vec<String>) {
        let summary = self
            .groups
            .into_iter()
            .map(|(m, n)| if n > 1 { format!("{m} (×{n})") } else { m })
            .collect();
        (summary, self.details)
    }
}

/// Recursion limit for placements, mapped items and booleans, which a
/// malformed file could make cyclic.
const MAX_DEPTH: usize = 32;

/// A planar region: an outer loop (counter-clockwise) and holes (clockwise).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Region {
    pub outer: Vec<DVec2>,
    pub holes: Vec<Vec<DVec2>>,
}

/// A revolution CreaBIM can build as one native arc sweep: the product's
/// body a single `IfcRevolvedAreaSolid` whose profile plane holds its
/// turn axis (a beam bent in a plane). All in world coordinates; `sweep`
/// in radians, right-handed about `axis`; the profile in the sweep's
/// frame, `x` to the path's left, `y` along the axis. `z` is the world
/// height of the profile's centre of mass, for picking the storey.
#[derive(Debug)]
pub(crate) struct RevolvedSweep {
    pub center: DVec3,
    pub axis: DVec3,
    pub start: DVec3,
    pub sweep: f64,
    pub profile: Vec<DVec2>,
    pub z: f64,
}

impl Region {
    fn new(outer: Vec<DVec2>, holes: Vec<Vec<DVec2>>) -> Option<Self> {
        let same = |a: DVec2, b: DVec2| a.distance_squared(b) < 1e-18;
        let mut outer = clean_loop(&outer, same);
        if outer.len() < 3 || area2(&outer).abs() < 1e-14 {
            return None;
        }
        if area2(&outer) < 0.0 {
            outer.reverse();
        }
        let holes = holes
            .into_iter()
            .map(|h| clean_loop(&h, same))
            .filter(|h| h.len() >= 3 && area2(h).abs() > 1e-14)
            .map(|mut h| {
                if area2(&h) > 0.0 {
                    h.reverse();
                }
                h
            })
            .collect();
        Some(Self { outer, holes })
    }

    fn transformed(&self, m: &DMat4) -> Option<Self> {
        let tf = |l: &Vec<DVec2>| -> Vec<DVec2> {
            l.iter()
                .map(|p| m.transform_point3(p.extend(0.0)).truncate())
                .collect()
        };
        Region::new(tf(&self.outer), self.holes.iter().map(tf).collect())
    }
}

/// An extruded area solid, unpacked: regions in the solid's own frame,
/// the frame (`position`, already combined with the caller's transform)
/// and the extrusion vector in that frame.
#[derive(Clone, Debug)]
pub(crate) struct Extrusion {
    pub regions: Vec<Region>,
    pub position: DMat4,
    pub vector: DVec3,
}

/// One shape representation of a product.
#[derive(Clone, Debug)]
pub(crate) struct Rep {
    pub identifier: String,
    pub kind: String,
    pub items: Vec<u64>,
}

pub(crate) struct Geo<'a> {
    pub f: &'a StepFile,
    /// Metres per file length unit.
    pub len: f64,
    /// Radians per file plane angle unit.
    pub angle: f64,
    pub warnings: Warnings,
    placements: HashMap<u64, DMat4>,
}

impl<'a> Geo<'a> {
    pub fn new(f: &'a StepFile, len: f64, angle: f64) -> Self {
        Self {
            f,
            len,
            angle,
            warnings: Warnings::default(),
            placements: HashMap::new(),
        }
    }

    pub fn get(&self, v: &Value) -> Option<&'a Entity> {
        self.f.deref(v)
    }

    pub fn length(&self, v: &Value) -> Option<f64> {
        v.as_f64().map(|x| x * self.len).filter(|x| x.is_finite())
    }

    // ---- points, directions, placements ----

    pub fn point(&self, v: &Value) -> Option<DVec3> {
        let e = self.get(v)?;
        if !e.is("IFCCARTESIANPOINT") {
            return None;
        }
        let c: Vec<f64> = e
            .arg(0)
            .as_list()
            .iter()
            .filter_map(Value::as_f64)
            .collect();
        let p = DVec3::new(
            c.first().copied().unwrap_or(0.0),
            c.get(1).copied().unwrap_or(0.0),
            c.get(2).copied().unwrap_or(0.0),
        ) * self.len;
        p.is_finite().then_some(p)
    }

    pub fn direction(&self, v: &Value) -> Option<DVec3> {
        let e = self.get(v)?;
        let ratios = match e.class.as_str() {
            "IFCDIRECTION" => e.arg(0),
            // IfcVector(Orientation, Magnitude): only the direction matters here
            "IFCVECTOR" => return self.direction(e.arg(0)),
            _ => return None,
        };
        let c: Vec<f64> = ratios.as_list().iter().filter_map(Value::as_f64).collect();
        let d = DVec3::new(
            c.first().copied().unwrap_or(0.0),
            c.get(1).copied().unwrap_or(0.0),
            c.get(2).copied().unwrap_or(0.0),
        );
        (d.is_finite() && d.length_squared() > 1e-20).then(|| d.normalize())
    }

    /// An IfcAxis2Placement2D/3D or IfcAxis1Placement as a matrix.
    pub fn axis_placement(&self, v: &Value) -> DMat4 {
        let Some(e) = self.get(v) else {
            return DMat4::IDENTITY;
        };
        let origin = self.point(e.arg(0)).unwrap_or(DVec3::ZERO);
        let (z, x) = match e.class.as_str() {
            "IFCAXIS2PLACEMENT3D" => (self.direction(e.arg(1)), self.direction(e.arg(2))),
            "IFCAXIS2PLACEMENT2D" => (None, self.direction(e.arg(1))),
            "IFCAXIS1PLACEMENT" => (self.direction(e.arg(1)), None),
            _ => return DMat4::IDENTITY,
        };
        frame(origin, z.unwrap_or(DVec3::Z), x)
    }

    /// An IfcCartesianTransformationOperator (2D or 3D, uniform or not).
    pub fn transform_operator(&self, v: &Value) -> DMat4 {
        let Some(e) = self.get(v) else {
            return DMat4::IDENTITY;
        };
        let three_d = e.class.starts_with("IFCCARTESIANTRANSFORMATIONOPERATOR3D");
        let two_d = e.class.starts_with("IFCCARTESIANTRANSFORMATIONOPERATOR2D");
        if !three_d && !two_d {
            return DMat4::IDENTITY;
        }
        let a1 = self.direction(e.arg(0));
        let a2 = self.direction(e.arg(1));
        let origin = self.point(e.arg(2)).unwrap_or(DVec3::ZERO);
        let s = e.arg(3).as_f64().unwrap_or(1.0);
        let (d3, s2, s3) = if three_d {
            let nonuniform = e.class.ends_with("NONUNIFORM");
            (
                self.direction(e.arg(4)).unwrap_or(DVec3::Z),
                if nonuniform { e.arg(5).as_f64() } else { None }.unwrap_or(s),
                if nonuniform { e.arg(6).as_f64() } else { None }.unwrap_or(s),
            )
        } else {
            let nonuniform = e.class.ends_with("NONUNIFORM");
            (
                DVec3::Z,
                if nonuniform { e.arg(4).as_f64() } else { None }.unwrap_or(s),
                1.0,
            )
        };
        let m = frame(DVec3::ZERO, d3, a1);
        let (d1, mut d2) = (m.x_axis.truncate(), m.y_axis.truncate());
        if let Some(a2) = a2 {
            let y = a2 - d3 * a2.dot(d3) - d1 * a2.dot(d1);
            if y.length_squared() > 1e-12 {
                d2 = y.normalize();
            }
        }
        DMat4::from_cols(
            (d1 * s).extend(0.0),
            (d2 * s2).extend(0.0),
            (d3 * s3).extend(0.0),
            origin.extend(1.0),
        )
    }

    /// The world matrix of an IfcObjectPlacement, following the chain of
    /// relative placements.
    pub fn placement(&mut self, v: &Value) -> DMat4 {
        self.placement_at(v, 0)
    }

    fn placement_at(&mut self, v: &Value, depth: usize) -> DMat4 {
        let Some(id) = v.as_ref() else {
            return DMat4::IDENTITY;
        };
        if let Some(m) = self.placements.get(&id) {
            return *m;
        }
        let Some(e) = self.f.get(id) else {
            return DMat4::IDENTITY;
        };
        if depth > MAX_DEPTH {
            self.warnings.add("placement chain too deep or cyclic");
            return DMat4::IDENTITY;
        }
        let m = match e.class.as_str() {
            "IFCLOCALPLACEMENT" => {
                let parent = self.placement_at(e.arg(0), depth + 1);
                parent * self.axis_placement(e.arg(1))
            }
            other => {
                self.warnings.add(format!(
                    "{other} is not supported; its elements are placed at the origin"
                ));
                DMat4::IDENTITY
            }
        };
        self.placements.insert(id, m);
        m
    }

    // ---- curves ----

    /// Points along a curve, in the curve's own coordinates (2D curves get z = 0).
    pub fn curve(&mut self, v: &Value) -> Option<Vec<DVec3>> {
        self.curve_at(v, 0)
    }

    fn curve_at(&mut self, v: &Value, depth: usize) -> Option<Vec<DVec3>> {
        let e = self.get(v)?;
        if depth > MAX_DEPTH {
            return None;
        }
        match e.class.as_str() {
            "IFCPOLYLINE" => Some(
                e.arg(0)
                    .as_list()
                    .iter()
                    .filter_map(|p| self.point(p))
                    .collect(),
            ),
            "IFCINDEXEDPOLYCURVE" => self.indexed_poly_curve(e),
            "IFCCOMPOSITECURVE" | "IFCCOMPOSITECURVEONSURFACE" => {
                let mut out: Vec<DVec3> = Vec::new();
                for seg in e.arg(0).as_list() {
                    let Some(s) = self.get(seg) else { continue };
                    let (same_sense, parent) = match s.class.as_str() {
                        "IFCCOMPOSITECURVESEGMENT" | "IFCREPARAMETRISEDCOMPOSITECURVESEGMENT" => {
                            (s.arg(1).as_bool().unwrap_or(true), s.arg(2))
                        }
                        _ => continue,
                    };
                    let Some(mut pts) = self.curve_at(parent, depth + 1) else {
                        self.warnings
                            .add("composite curve segment could not be read");
                        continue;
                    };
                    if !same_sense {
                        pts.reverse();
                    }
                    for p in pts {
                        if out.last().is_none_or(|q| q.distance_squared(p) > 1e-16) {
                            out.push(p);
                        }
                    }
                }
                Some(out)
            }
            "IFCTRIMMEDCURVE" => self.trimmed_curve(e, depth),
            "IFCCIRCLE" | "IFCELLIPSE" => {
                let (m, rx, ry) = self.conic(e)?;
                Some(arc_points(&m, rx, ry, 0.0, TAU, true))
            }
            "IFCLINE" => {
                self.warnings.add("an unbounded IfcLine was skipped");
                None
            }
            other => {
                self.warnings
                    .add(format!("{other} curves are not supported"));
                None
            }
        }
    }

    fn conic(&self, e: &Entity) -> Option<(DMat4, f64, f64)> {
        let m = self.axis_placement(e.arg(0));
        let rx = self.length(e.arg(1))?;
        let ry = if e.is("IFCELLIPSE") {
            self.length(e.arg(2))?
        } else {
            rx
        };
        (rx > 0.0 && ry > 0.0).then_some((m, rx, ry))
    }

    fn indexed_poly_curve(&self, e: &Entity) -> Option<Vec<DVec3>> {
        let pts = self.point_list(e.arg(0))?;
        let at = |i: &Value| -> Option<DVec3> {
            let i = i.as_f64()? as usize;
            pts.get(i.checked_sub(1)?).copied()
        };
        if e.arg(1).is_null() {
            return Some(pts);
        }
        let mut out: Vec<DVec3> = Vec::new();
        let push = |p: DVec3, out: &mut Vec<DVec3>| {
            if out.last().is_none_or(|q| q.distance_squared(p) > 1e-16) {
                out.push(p);
            }
        };
        for seg in e.arg(1).as_list() {
            let Value::Typed(kind, inner) = seg else {
                continue;
            };
            let idx: Vec<DVec3> = inner.as_list().iter().filter_map(at).collect();
            match kind.as_str() {
                "IFCARCINDEX" if idx.len() == 3 => {
                    for p in arc_through(idx[0], idx[1], idx[2]) {
                        push(p, &mut out);
                    }
                }
                _ => {
                    for p in idx {
                        push(p, &mut out);
                    }
                }
            }
        }
        Some(out)
    }

    /// An IfcCartesianPointList2D/3D, scaled.
    pub fn point_list(&self, v: &Value) -> Option<Vec<DVec3>> {
        let e = self.get(v)?;
        if !e.class.starts_with("IFCCARTESIANPOINTLIST") {
            return None;
        }
        Some(
            e.arg(0)
                .as_list()
                .iter()
                .map(|c| {
                    let c: Vec<f64> = c.as_list().iter().filter_map(Value::as_f64).collect();
                    DVec3::new(
                        c.first().copied().unwrap_or(0.0),
                        c.get(1).copied().unwrap_or(0.0),
                        c.get(2).copied().unwrap_or(0.0),
                    ) * self.len
                })
                .collect(),
        )
    }

    fn trimmed_curve(&mut self, e: &Entity, depth: usize) -> Option<Vec<DVec3>> {
        let basis = self.get(e.arg(0))?;
        let sense = e.arg(3).as_bool().unwrap_or(true);
        let prefer_cartesian = e.arg(4).as_enum() == Some("CARTESIAN");
        // a trim is a list holding a point, a parameter, or both
        let trim = |t: &Value| -> (Option<DVec3>, Option<f64>) {
            let mut p = None;
            let mut param = None;
            for x in t.as_list() {
                match x {
                    Value::Ref(_) => p = self.point(x),
                    Value::Typed(_, inner) => param = inner.as_f64(),
                    Value::Real(r) => param = Some(*r),
                    _ => {}
                }
            }
            (p, param)
        };
        let (t1, t2) = (trim(e.arg(1)), trim(e.arg(2)));
        match basis.class.as_str() {
            "IFCCIRCLE" | "IFCELLIPSE" => {
                let (m, rx, ry) = self.conic(basis)?;
                let inv = m.inverse();
                let angle_of = |(p, param): (Option<DVec3>, Option<f64>)| -> Option<f64> {
                    let from_point = p.map(|p| {
                        let l = inv.transform_point3(p);
                        (l.y / ry).atan2(l.x / rx)
                    });
                    let from_param = param.map(|a| a * self.angle);
                    if prefer_cartesian {
                        from_point.or(from_param)
                    } else {
                        from_param.or(from_point)
                    }
                };
                let (a1, a2) = (angle_of(t1)?, angle_of(t2)?);
                Some(arc_points(&m, rx, ry, a1, a2, sense))
            }
            "IFCLINE" => {
                let origin = self.point(basis.arg(0))?;
                let vec = self.get(basis.arg(1))?;
                let dir = self.direction(vec.arg(0))?;
                let mag = vec.arg(1).as_f64().unwrap_or(1.0) * self.len;
                let at = |(p, param): (Option<DVec3>, Option<f64>)| -> Option<DVec3> {
                    let from_param = param.map(|t| origin + dir * mag * t);
                    if prefer_cartesian {
                        p.or(from_param)
                    } else {
                        from_param.or(p)
                    }
                };
                Some(vec![at(t1)?, at(t2)?])
            }
            _ => {
                // a trimmed polyline or similar: fall back to the points given
                let (p1, p2) = (t1.0, t2.0);
                match (p1, p2) {
                    (Some(a), Some(b)) => Some(vec![a, b]),
                    _ => self.curve_at(e.arg(0), depth + 1),
                }
            }
        }
    }

    // ---- profiles ----

    /// The regions of a profile definition, in the profile's coordinates.
    pub fn profile(&mut self, v: &Value) -> Vec<Region> {
        self.profile_at(v, 0)
    }

    fn profile_at(&mut self, v: &Value, depth: usize) -> Vec<Region> {
        let Some(e) = self.get(v) else {
            return vec![];
        };
        if depth > MAX_DEPTH {
            return vec![];
        }
        let pos = |g: &Self| g.axis_placement(e.arg(2));
        let l = |g: &Self, i: usize| g.length(e.arg(i)).unwrap_or(0.0);
        let rect = |w: f64, h: f64| {
            vec![
                DVec2::new(-w / 2.0, -h / 2.0),
                DVec2::new(w / 2.0, -h / 2.0),
                DVec2::new(w / 2.0, h / 2.0),
                DVec2::new(-w / 2.0, h / 2.0),
            ]
        };
        let shape = |g: &Self, outer: Vec<DVec2>, holes: Vec<Vec<DVec2>>| -> Vec<Region> {
            Region::new(outer, holes)
                .and_then(|r| r.transformed(&pos(g)))
                .into_iter()
                .collect()
        };
        match e.class.as_str() {
            "IFCRECTANGLEPROFILEDEF" | "IFCROUNDEDRECTANGLEPROFILEDEF" => {
                shape(self, rect(l(self, 3), l(self, 4)), vec![])
            }
            "IFCRECTANGLEHOLLOWPROFILEDEF" => {
                let (w, h, t) = (l(self, 3), l(self, 4), l(self, 5));
                shape(self, rect(w, h), vec![rect(w - 2.0 * t, h - 2.0 * t)])
            }
            "IFCCIRCLEPROFILEDEF" => {
                let r = l(self, 3);
                shape(self, circle(r), vec![])
            }
            "IFCCIRCLEHOLLOWPROFILEDEF" => {
                let (r, t) = (l(self, 3), l(self, 4));
                shape(self, circle(r), vec![circle(r - t)])
            }
            "IFCELLIPSEPROFILEDEF" => {
                let (a, b) = (l(self, 3), l(self, 4));
                let m = DMat4::from_scale(DVec3::new(a, b, 1.0));
                shape(
                    self,
                    arc_points(&m, 1.0, 1.0, 0.0, TAU, true)
                        .iter()
                        .map(|p| p.truncate())
                        .collect(),
                    vec![],
                )
            }
            "IFCISHAPEPROFILEDEF" | "IFCASYMMETRICISHAPEPROFILEDEF" => {
                let (w, d, tw, tf) = (l(self, 3), l(self, 4), l(self, 5), l(self, 6));
                let (x, y, a, b) = (w / 2.0, d / 2.0, tw / 2.0, d / 2.0 - tf);
                let pts = [
                    (-x, -y),
                    (x, -y),
                    (x, -b),
                    (a, -b),
                    (a, b),
                    (x, b),
                    (x, y),
                    (-x, y),
                    (-x, b),
                    (-a, b),
                    (-a, -b),
                    (-x, -b),
                ];
                shape(
                    self,
                    pts.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
                    vec![],
                )
            }
            "IFCLSHAPEPROFILEDEF" => {
                let d = l(self, 3);
                let w = self.length(e.arg(4)).unwrap_or(d);
                let t = l(self, 5);
                let (x, y) = (w / 2.0, d / 2.0);
                let pts = [
                    (-x, -y),
                    (x, -y),
                    (x, -y + t),
                    (-x + t, -y + t),
                    (-x + t, y),
                    (-x, y),
                ];
                shape(
                    self,
                    pts.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
                    vec![],
                )
            }
            "IFCUSHAPEPROFILEDEF" | "IFCCSHAPEPROFILEDEF" => {
                let d = l(self, 3);
                let w = l(self, 4);
                let (tw, tf) = if e.is("IFCUSHAPEPROFILEDEF") {
                    (l(self, 5), l(self, 6))
                } else {
                    (l(self, 5), l(self, 5))
                };
                let (x, y) = (w / 2.0, d / 2.0);
                let pts = [
                    (-x, -y),
                    (x, -y),
                    (x, -y + tf),
                    (-x + tw, -y + tf),
                    (-x + tw, y - tf),
                    (x, y - tf),
                    (x, y),
                    (-x, y),
                ];
                shape(
                    self,
                    pts.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
                    vec![],
                )
            }
            "IFCTSHAPEPROFILEDEF" => {
                let (d, w, tw, tf) = (l(self, 3), l(self, 4), l(self, 5), l(self, 6));
                let (x, y, a) = (w / 2.0, d / 2.0, tw / 2.0);
                let pts = [
                    (-a, -y),
                    (a, -y),
                    (a, y - tf),
                    (x, y - tf),
                    (x, y),
                    (-x, y),
                    (-x, y - tf),
                    (-a, y - tf),
                ];
                shape(
                    self,
                    pts.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
                    vec![],
                )
            }
            "IFCZSHAPEPROFILEDEF" => {
                let (d, w, tw, tf) = (l(self, 3), l(self, 4), l(self, 5), l(self, 6));
                let (y, a) = (d / 2.0, tw / 2.0);
                let pts = [
                    (-a, -y),
                    (w - a, -y),
                    (w - a, -y + tf),
                    (a, -y + tf),
                    (a, y),
                    (a - w, y),
                    (a - w, y - tf),
                    (-a, y - tf),
                ];
                shape(
                    self,
                    pts.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
                    vec![],
                )
            }
            "IFCARBITRARYCLOSEDPROFILEDEF" | "IFCARBITRARYPROFILEDEFWITHVOIDS" => {
                let Some(outer) = self.curve_at(e.arg(2), depth + 1) else {
                    return vec![];
                };
                let holes: Vec<Vec<DVec2>> = e
                    .arg(3)
                    .as_list()
                    .iter()
                    .filter_map(|c| self.curve_at(c, depth + 1))
                    .map(|c| c.iter().map(|p| p.truncate()).collect())
                    .collect();
                Region::new(outer.iter().map(|p| p.truncate()).collect(), holes)
                    .into_iter()
                    .collect()
            }
            "IFCDERIVEDPROFILEDEF" => {
                let m = self.transform_operator(e.arg(3));
                self.profile_at(e.arg(2), depth + 1)
                    .iter()
                    .filter_map(|r| r.transformed(&m))
                    .collect()
            }
            "IFCCOMPOSITEPROFILEDEF" => e
                .arg(2)
                .as_list()
                .iter()
                .flat_map(|p| self.profile_at(p, depth + 1))
                .collect(),
            other => {
                self.warnings
                    .add(format!("{other} profiles are not supported"));
                vec![]
            }
        }
    }

    // ---- representations ----

    /// The shape representations of a product.
    pub fn reps(&self, product: &Entity) -> Vec<Rep> {
        let Some(shape) = self.get(product.arg(6)) else {
            return vec![];
        };
        shape
            .arg(2)
            .as_list()
            .iter()
            .filter_map(|r| self.get(r))
            .map(|r| Rep {
                identifier: r.arg(1).as_str().unwrap_or("").to_string(),
                kind: r.arg(2).as_str().unwrap_or("").to_string(),
                items: r.arg(3).refs().collect(),
            })
            .collect()
    }

    /// The representation that describes the solid body.
    pub fn body(&self, product: &Entity) -> Option<Rep> {
        let reps = self.reps(product);
        let not_body = [
            "Axis",
            "FootPrint",
            "Box",
            "Annotation",
            "Profile",
            "Plan",
            "Reference",
            "Lighting",
            "CoG",
            "Clearance",
            "Surface",
        ];
        reps.iter()
            .find(|r| r.identifier.eq_ignore_ascii_case("Body"))
            .or_else(|| reps.iter().find(|r| r.identifier == "Facetation"))
            .or_else(|| {
                reps.iter().find(|r| {
                    !not_body.contains(&r.identifier.as_str())
                        && !matches!(
                            r.kind.as_str(),
                            "Curve2D"
                                | "Curve3D"
                                | "Curve"
                                | "GeometricCurveSet"
                                | "Annotation2D"
                                | "Point"
                                | "BoundingBox"
                        )
                })
            })
            .cloned()
    }

    /// The product's body as a mesh in world coordinates (`world` is the
    /// product's placement), falling back to its bounding box.
    pub fn product_mesh(&mut self, product: &Entity, world: &DMat4) -> EditMesh {
        let mut mesh = EditMesh::default();
        if let Some(body) = self.body(product) {
            for item in &body.items {
                self.item(*item, world, &mut mesh, 0);
            }
        }
        if mesh.faces.is_empty() {
            for rep in self.reps(product) {
                if rep.identifier == "Box" || rep.kind == "BoundingBox" {
                    for item in &rep.items {
                        self.item(*item, world, &mut mesh, 0);
                    }
                }
            }
        }
        mesh
    }

    /// Mesh one representation item into `out`. Returns whether it produced faces.
    pub fn item(&mut self, id: u64, tf: &DMat4, out: &mut EditMesh, depth: usize) -> bool {
        let before = out.faces.len();
        let Some(e) = self.f.get(id) else {
            return false;
        };
        if depth > MAX_DEPTH {
            self.warnings
                .add("representation nested too deeply or cyclic");
            return false;
        }
        let flip = tf.determinant() < 0.0;
        let mut local = EditMesh::default();
        match e.class.as_str() {
            "IFCEXTRUDEDAREASOLID" | "IFCEXTRUDEDAREASOLIDTAPERED" => {
                if e.is("IFCEXTRUDEDAREASOLIDTAPERED") {
                    self.warnings
                        .add("tapered extrusions are imported without their taper");
                }
                if let Some(x) = self.extrusion(id, tf) {
                    extrude_mesh(&x, out);
                }
                return out.faces.len() > before;
            }
            "IFCREVOLVEDAREASOLID" => {
                let regions = self.profile(e.arg(0));
                let pos = *tf * self.axis_placement(e.arg(1));
                let Some(axis) = self.get(e.arg(2)) else {
                    return false;
                };
                let a0 = self.point(axis.arg(0)).unwrap_or(DVec3::ZERO);
                let dir = self.direction(axis.arg(1)).unwrap_or(DVec3::Y);
                let angle = e.arg(3).as_f64().unwrap_or(TAU) * self.angle;
                for r in &regions {
                    revolve_mesh(r, a0, dir, angle, &pos, out);
                }
                return out.faces.len() > before;
            }
            "IFCSWEPTDISKSOLID" | "IFCSWEPTDISKSOLIDPOLYGONAL" => {
                let r = self.length(e.arg(1)).unwrap_or(0.0);
                if let Some(path) = self.curve(e.arg(0)) {
                    let path: Vec<DVec3> = path.iter().map(|p| tf.transform_point3(*p)).collect();
                    tube_mesh(&path, r * tf.x_axis.truncate().length(), out);
                }
                return out.faces.len() > before;
            }
            "IFCTRIANGULATEDFACESET" | "IFCTRIANGULATEDIRREGULARNETWORK" => {
                let Some(pts) = self.point_list(e.arg(0)) else {
                    return false;
                };
                let pn: Vec<usize> = e
                    .arg(4)
                    .as_list()
                    .iter()
                    .filter_map(|v| v.as_f64().map(|x| x as usize))
                    .collect();
                let at = |i: &Value| -> Option<DVec3> {
                    let mut i = (i.as_f64()? as usize).checked_sub(1)?;
                    if !pn.is_empty() {
                        i = pn.get(i)?.checked_sub(1)?;
                    }
                    pts.get(i).copied()
                };
                for t in e.arg(3).as_list() {
                    let face: Option<Vec<DVec3>> = t.as_list().iter().map(at).collect();
                    if let Some(face) = face.filter(|f| f.len() >= 3) {
                        add_face(&mut local, &face, &[]);
                    }
                }
            }
            "IFCPOLYGONALFACESET" => {
                let Some(pts) = self.point_list(e.arg(0)) else {
                    return false;
                };
                let pn: Vec<usize> = e
                    .arg(3)
                    .as_list()
                    .iter()
                    .filter_map(|v| v.as_f64().map(|x| x as usize))
                    .collect();
                let at = |i: &Value| -> Option<DVec3> {
                    let mut i = (i.as_f64()? as usize).checked_sub(1)?;
                    if !pn.is_empty() {
                        i = pn.get(i)?.checked_sub(1)?;
                    }
                    pts.get(i).copied()
                };
                for face in e.arg(2).as_list() {
                    let Some(fe) = self.get(face) else { continue };
                    let outer: Option<Vec<DVec3>> = fe.arg(0).as_list().iter().map(at).collect();
                    let holes: Vec<Vec<DVec3>> = fe
                        .arg(1)
                        .as_list()
                        .iter()
                        .filter_map(|h| h.as_list().iter().map(at).collect())
                        .collect();
                    if let Some(outer) = outer {
                        add_face(&mut local, &outer, &holes);
                    }
                }
            }
            "IFCFACETEDBREP"
            | "IFCFACETEDBREPWITHVOIDS"
            | "IFCADVANCEDBREP"
            | "IFCADVANCEDBREPWITHVOIDS" => {
                let mut shells = vec![e.arg(0)];
                shells.extend(e.arg(1).as_list());
                for s in shells {
                    self.shell(s, &mut local);
                }
                if local.faces.is_empty() && e.class.starts_with("IFCADVANCEDBREP") {
                    self.warnings
                        .add("advanced B-rep faces with curved surfaces are not supported");
                }
            }
            "IFCSHELLBASEDSURFACEMODEL" => {
                for s in e.arg(0).as_list() {
                    self.shell(s, &mut local);
                }
            }
            "IFCFACEBASEDSURFACEMODEL" => {
                for s in e.arg(0).as_list() {
                    self.shell(s, &mut local);
                }
            }
            "IFCBOOLEANRESULT" | "IFCBOOLEANCLIPPINGRESULT" => {
                let first = e.arg(1).as_ref();
                let second = e.arg(2).as_ref();
                if e.arg(0).as_enum() == Some("UNION") {
                    for op in [first, second].into_iter().flatten() {
                        self.item(op, tf, out, depth + 1);
                    }
                } else {
                    self.warnings.add(format!(
                        "boolean {} ignored: only the first operand is imported",
                        e.arg(0).as_enum().unwrap_or("operation").to_lowercase()
                    ));
                    if let Some(op) = first {
                        self.item(op, tf, out, depth + 1);
                    }
                }
                return out.faces.len() > before;
            }
            "IFCCSGSOLID" => {
                if let Some(op) = e.arg(0).as_ref() {
                    self.item(op, tf, out, depth + 1);
                }
                return out.faces.len() > before;
            }
            "IFCMAPPEDITEM" => {
                let Some(source) = self.get(e.arg(0)) else {
                    return false;
                };
                let origin = self.axis_placement(source.arg(0));
                let target = self.transform_operator(e.arg(1));
                let m = *tf * target * origin;
                let items: Vec<u64> = self
                    .get(source.arg(1))
                    .map(|r| r.arg(3).refs().collect())
                    .unwrap_or_default();
                for item in items {
                    self.item(item, &m, out, depth + 1);
                }
                return out.faces.len() > before;
            }
            "IFCBOUNDINGBOX" => {
                let corner = self.point(e.arg(0)).unwrap_or(DVec3::ZERO);
                let size = DVec3::new(
                    self.length(e.arg(1)).unwrap_or(0.0),
                    self.length(e.arg(2)).unwrap_or(0.0),
                    self.length(e.arg(3)).unwrap_or(0.0),
                );
                local = box_mesh(corner, size);
            }
            "IFCBLOCK" => {
                let m = self.axis_placement(e.arg(0));
                let size = DVec3::new(
                    self.length(e.arg(1)).unwrap_or(0.0),
                    self.length(e.arg(2)).unwrap_or(0.0),
                    self.length(e.arg(3)).unwrap_or(0.0),
                );
                local = box_mesh(DVec3::ZERO, size);
                transform_mesh(&mut local, &m);
            }
            "IFCRIGHTCIRCULARCYLINDER" => {
                let m = *tf * self.axis_placement(e.arg(0));
                let h = self.length(e.arg(1)).unwrap_or(0.0);
                let r = self.length(e.arg(2)).unwrap_or(0.0);
                if let Some(region) = Region::new(circle(r), vec![]) {
                    let x = Extrusion {
                        regions: vec![region],
                        position: m,
                        vector: DVec3::Z * h,
                    };
                    extrude_mesh(&x, out);
                }
                return out.faces.len() > before;
            }
            "IFCHALFSPACESOLID" | "IFCBOXEDHALFSPACE" | "IFCPOLYGONALBOUNDEDHALFSPACE" => {
                return false;
            }
            "IFCGEOMETRICSET"
            | "IFCGEOMETRICCURVESET"
            | "IFCPOLYLINE"
            | "IFCINDEXEDPOLYCURVE"
            | "IFCCOMPOSITECURVE"
            | "IFCTRIMMEDCURVE"
            | "IFCCARTESIANPOINT"
            | "IFCANNOTATIONFILLAREA"
            | "IFCTEXTLITERAL"
            | "IFCTEXTLITERALWITHEXTENT" => {
                return false;
            }
            other => {
                self.warnings
                    .add(format!("{other} geometry is not supported"));
                return false;
            }
        }
        transform_mesh(&mut local, tf);
        if flip {
            for f in &mut local.faces {
                f.reverse();
            }
        }
        append(out, local);
        out.faces.len() > before
    }

    fn shell(&mut self, v: &Value, out: &mut EditMesh) {
        let Some(s) = self.get(v) else { return };
        // IfcClosedShell / IfcOpenShell / IfcConnectedFaceSet all list faces first
        for face in s.arg(0).as_list() {
            let Some(fe) = self.get(face) else { continue };
            if !matches!(
                fe.class.as_str(),
                "IFCFACE" | "IFCFACESURFACE" | "IFCADVANCEDFACE"
            ) {
                continue;
            }
            let mut bounds: Vec<(Vec<DVec3>, bool)> = Vec::new();
            for b in fe.arg(0).as_list() {
                let Some(be) = self.get(b) else { continue };
                let Some(lp) = self.get(be.arg(0)) else {
                    continue;
                };
                if !lp.is("IFCPOLYLOOP") {
                    continue;
                }
                let mut pts: Vec<DVec3> = lp
                    .arg(0)
                    .as_list()
                    .iter()
                    .filter_map(|p| self.point(p))
                    .collect();
                if be.arg(1).as_bool() == Some(false) {
                    pts.reverse();
                }
                bounds.push((pts, be.is("IFCFACEOUTERBOUND")));
            }
            // the outer bound is the one marked so, or else the first
            let k = bounds.iter().position(|b| b.1).unwrap_or(0);
            let outer = (k < bounds.len()).then(|| bounds.remove(k).0);
            let mut holes: Vec<Vec<DVec3>> = bounds.into_iter().map(|b| b.0).collect();
            if let Some(mut outer) = outer {
                // a face surface with SameSense = false points the other way
                if fe.is("IFCFACESURFACE") && fe.arg(2).as_bool() == Some(false) {
                    outer.reverse();
                    for h in &mut holes {
                        h.reverse();
                    }
                }
                add_face(out, &outer, &holes);
            }
        }
    }

    /// Unpack an IfcExtrudedAreaSolid with the caller's transform applied.
    pub fn extrusion(&mut self, id: u64, tf: &DMat4) -> Option<Extrusion> {
        let e = self.f.get(id)?;
        if !e.class.starts_with("IFCEXTRUDEDAREASOLID") {
            return None;
        }
        let regions = self.profile(e.arg(0));
        if regions.is_empty() {
            self.warnings
                .add("an extrusion's profile could not be read");
            return None;
        }
        let position = *tf * self.axis_placement(e.arg(1));
        let dir = self.direction(e.arg(2)).unwrap_or(DVec3::Z);
        let depth = self.length(e.arg(3))?;
        if depth <= 0.0 {
            return None;
        }
        Some(Extrusion {
            regions,
            position,
            vector: dir * depth,
        })
    }

    /// The product's body as one native arc sweep, or why it is not one.
    /// `None` when the body is not a single `IfcRevolvedAreaSolid`, so the
    /// caller tries its other readings; `Some(Err)` when it is one that stays
    /// a mesh, with why.
    ///
    /// The mapping is exact. The file's profile points, placed by the solid's
    /// Position, are re-expressed in the sweep's frame at the arc's start
    /// (`x` to the path's left, `y` along the axis) — a rotation of the
    /// profile plane, so the swept solid is the same revolution the mesh
    /// path builds, rings and all. A placement that mirrors turns a
    /// right-hand revolve about the axis into one about the opposite of it,
    /// so the sweep runs the other way; a placement that scales non-uniformly
    /// would warp the rings, and stays a mesh.
    pub fn revolved_sweep(
        &mut self,
        product: &Entity,
        world: &DMat4,
    ) -> Option<Result<RevolvedSweep, String>> {
        let body = self.body(product)?;
        let [item] = body.items[..] else {
            return None;
        };
        // the solid, and the placement that carries it into world
        // coordinates: one mapped item (a type's shape placed per instance)
        // carries its own origin and operator first
        let e = self.f.get(item)?;
        let (solid, pre) = if e.is("IFCMAPPEDITEM") {
            let source = self.get(e.arg(0))?;
            let origin = self.axis_placement(source.arg(0));
            let target = self.transform_operator(e.arg(1));
            let items: Vec<u64> = self
                .get(source.arg(1))
                .map(|r| r.arg(3).refs().collect())
                .unwrap_or_default();
            let [only] = items[..] else {
                return None;
            };
            (self.f.get(only)?, *world * target * origin)
        } else {
            (e, *world)
        };
        if !solid.is("IFCREVOLVEDAREASOLID") {
            return None;
        }
        let pos = pre * self.axis_placement(solid.arg(1));
        let Some(axis_e) = self.get(solid.arg(2)) else {
            return Some(Err("has no turn axis".into()));
        };
        let a0 = self.point(axis_e.arg(0)).unwrap_or(DVec3::ZERO);
        let Some(dir) = self.direction(axis_e.arg(1)) else {
            return Some(Err("has no turn axis".into()));
        };
        let angle = solid.arg(3).as_f64().unwrap_or(TAU) * self.angle;
        let angle = angle.clamp(-TAU, TAU);
        if angle.abs() < 1e-9 {
            return Some(Err("turns through no angle".into()));
        }
        // the turn axis must lie in the profile's plane, which is the
        // solid's z = 0
        if a0.z.abs() > 1e-6 || dir.z.abs() > 1e-6 {
            return Some(Err(
                "turns about an axis its profile plane does not hold".into()
            ));
        }
        let regions = self.profile(solid.arg(0));
        let [region] = &regions[..] else {
            return Some(Err(if regions.len() > 1 {
                "sweeps more than one profile".to_string()
            } else {
                "has no profile CreaBIM can read".to_string()
            }));
        };
        if !region.holes.is_empty() {
            return Some(Err("its profile has holes".into()));
        }
        // the placement must not warp the rings: one scale, square axes
        let (ex, ey, ez) = (
            pos.transform_vector3(DVec3::X),
            pos.transform_vector3(DVec3::Y),
            pos.transform_vector3(DVec3::Z),
        );
        let s = ex.length();
        if s < 1e-9
            || (ey.length() - s).abs() > 1e-6 * s
            || (ez.length() - s).abs() > 1e-6 * s
            || ex.dot(ey).abs() > 1e-6 * s * s
            || ex.dot(ez).abs() > 1e-6 * s * s
            || ey.dot(ez).abs() > 1e-6 * s * s
        {
            return Some(Err("is placed with a scale that warps the turn".into()));
        }
        let sign = if ex.cross(ey).dot(ez) < 0.0 {
            -1.0
        } else {
            1.0
        };
        let sweep = angle * sign;
        let center = pos.transform_point3(a0);
        let axis = pos.transform_vector3(dir).normalize_or(DVec3::Y);
        // the arc starts at the profile's centre of mass, so the profile
        // rides centred on the path
        let n = region.outer.len() as f64;
        let c2 = region.outer.iter().fold(DVec2::ZERO, |a, p| a + *p) / n;
        let wc = pos.transform_point3(c2.extend(0.0));
        let out = wc - center;
        let radial = out - axis * out.dot(axis);
        let radius = radial.length();
        if radius < 1e-6 {
            return Some(Err("turns about its own profile".into()));
        }
        let u0 = radial / radius;
        let start = center + u0 * radius;
        // left of the travel: inwards on the arc, turned round when the
        // sweep runs the other way
        let across = -u0 * sweep.signum();
        let mut profile: Vec<DVec2> = region
            .outer
            .iter()
            .map(|p| {
                let d = pos.transform_point3(p.extend(0.0)) - start;
                DVec2::new(d.dot(across), d.dot(axis))
            })
            .collect();
        // stored counter-clockwise, like an authored profile
        if creabim_core::polygon::signed_area2(&profile) < 0.0 {
            profile.reverse();
        }
        Some(Ok(RevolvedSweep {
            center,
            axis,
            start,
            sweep,
            profile,
            z: wc.z,
        }))
    }
}

/// A right-handed frame at `origin` with z along `z` and x towards `x`, as
/// IFC builds axis placements (IfcBuildAxes).
pub(crate) fn frame(origin: DVec3, z: DVec3, x: Option<DVec3>) -> DMat4 {
    let z = z.normalize_or(DVec3::Z);
    let default_x = if (z - DVec3::X).length() < 1e-9 {
        DVec3::Y
    } else {
        DVec3::X
    };
    let mut xv = x.unwrap_or(default_x);
    xv -= z * xv.dot(z);
    if xv.length_squared() < 1e-12 {
        xv = default_x - z * default_x.dot(z);
        if xv.length_squared() < 1e-12 {
            xv = z.any_orthonormal_vector();
        }
    }
    let xv = xv.normalize();
    let y = z.cross(xv);
    DMat4::from_cols(
        xv.extend(0.0),
        y.extend(0.0),
        z.extend(0.0),
        origin.extend(1.0),
    )
}

/// Segments for a full circle of radius `r`: fine enough to look round,
/// coarse enough not to bloat small fillets.
fn circle_segments(r: f64) -> f64 {
    (16.0 + r.abs() * 32.0).clamp(16.0, 64.0)
}

fn circle(r: f64) -> Vec<DVec2> {
    if r <= 0.0 {
        return vec![];
    }
    let n = circle_segments(r) as usize;
    (0..n)
        .map(|i| {
            let a = TAU * i as f64 / n as f64;
            DVec2::new(r * a.cos(), r * a.sin())
        })
        .collect()
}

/// Points on an ellipse arc in frame `m`, from angle `a1` to `a2`,
/// counter-clockwise when `ccw`.
fn arc_points(m: &DMat4, rx: f64, ry: f64, a1: f64, a2: f64, ccw: bool) -> Vec<DVec3> {
    let mut sweep = if ccw { a2 - a1 } else { a1 - a2 };
    sweep = sweep.rem_euclid(TAU);
    if sweep < 1e-9 {
        sweep = TAU;
    }
    let n = ((sweep / TAU) * circle_segments(rx.max(ry)))
        .ceil()
        .max(1.0) as usize;
    let sign = if ccw { 1.0 } else { -1.0 };
    (0..=n)
        .map(|i| {
            let a = a1 + sign * sweep * i as f64 / n as f64;
            m.transform_point3(DVec3::new(rx * a.cos(), ry * a.sin(), 0.0))
        })
        .collect()
}

/// The circular arc from `a` through `b` to `c`.
fn arc_through(a: DVec3, b: DVec3, c: DVec3) -> Vec<DVec3> {
    let n = (b - a).cross(c - a);
    if n.length_squared() < 1e-20 {
        return vec![a, c]; // collinear: a straight segment
    }
    // circumcentre
    let ab = b - a;
    let ac = c - a;
    let center = a
        + (ac.length_squared() * n.cross(ab) + ab.length_squared() * ac.cross(n))
            / (2.0 * n.length_squared());
    let z = n.normalize();
    let x = (a - center).normalize();
    let m = DMat4::from_cols(
        x.extend(0.0),
        z.cross(x).extend(0.0),
        z.extend(0.0),
        center.extend(1.0),
    );
    let r = (a - center).length();
    let inv = m.inverse();
    let angle = |p: DVec3| {
        let l = inv.transform_point3(p);
        l.y.atan2(l.x).rem_euclid(TAU)
    };
    let end = angle(c);
    let mut pts = arc_points(&m, r, r, 0.0, end, true);
    if let Some(last) = pts.last_mut() {
        *last = c;
    }
    if let Some(first) = pts.first_mut() {
        *first = a;
    }
    pts
}

pub(crate) fn transform_mesh(mesh: &mut EditMesh, m: &DMat4) {
    for v in &mut mesh.vertices {
        *v = m.transform_point3(*v);
    }
}

pub(crate) fn append(out: &mut EditMesh, other: EditMesh) {
    let base = out.vertices.len() as u32;
    out.vertices.extend(other.vertices);
    out.faces.extend(
        other
            .faces
            .into_iter()
            .map(|f| f.into_iter().map(|v| v + base).collect()),
    );
}

fn box_mesh(corner: DVec3, size: DVec3) -> EditMesh {
    if size.min_element() <= 0.0 {
        return EditMesh::default();
    }
    let mut m = EditMesh::cuboid(size);
    let shift = corner + DVec3::new(size.x / 2.0, size.y / 2.0, 0.0);
    for v in &mut m.vertices {
        *v += shift;
    }
    m
}

/// Mesh an extrusion: caps plus a side quad per profile edge.
pub(crate) fn extrude_mesh(x: &Extrusion, out: &mut EditMesh) {
    let m = x.position;
    // the solid is inside out if the extrusion runs against the profile normal,
    // or the frame mirrors
    let flip = (x.vector.z < 0.0) != (m.determinant() < 0.0);
    let to3 = |p: &DVec2, top: bool| {
        let local = p.extend(0.0) + if top { x.vector } else { DVec3::ZERO };
        m.transform_point3(local)
    };
    let face = |pts: Vec<DVec3>, holes: Vec<Vec<DVec3>>, out: &mut EditMesh| {
        if flip {
            let mut pts = pts;
            pts.reverse();
            let holes: Vec<Vec<DVec3>> = holes
                .into_iter()
                .map(|mut h| {
                    h.reverse();
                    h
                })
                .collect();
            add_face(out, &pts, &holes);
        } else {
            add_face(out, &pts, &holes);
        }
    };
    for r in &x.regions {
        let bottom: Vec<DVec3> = r.outer.iter().rev().map(|p| to3(p, false)).collect();
        let bottom_holes: Vec<Vec<DVec3>> = r
            .holes
            .iter()
            .map(|h| h.iter().rev().map(|p| to3(p, false)).collect())
            .collect();
        face(bottom, bottom_holes, out);
        let top: Vec<DVec3> = r.outer.iter().map(|p| to3(p, true)).collect();
        let top_holes: Vec<Vec<DVec3>> = r
            .holes
            .iter()
            .map(|h| h.iter().map(|p| to3(p, true)).collect())
            .collect();
        face(top, top_holes, out);
        for lp in std::iter::once(&r.outer).chain(&r.holes) {
            for i in 0..lp.len() {
                let (a, b) = (&lp[i], &lp[(i + 1) % lp.len()]);
                face(
                    vec![to3(a, false), to3(b, false), to3(b, true), to3(a, true)],
                    vec![],
                    out,
                );
            }
        }
    }
}

/// Revolve a region about the axis through `a0` along `dir` (profile frame).
fn revolve_mesh(r: &Region, a0: DVec3, dir: DVec3, angle: f64, pos: &DMat4, out: &mut EditMesh) {
    let angle = angle.clamp(-TAU, TAU);
    let full = (angle.abs() - TAU).abs() < 1e-6;
    let steps = ((angle.abs() / TAU) * 32.0).ceil().max(1.0) as usize;
    let rot = |p: DVec2, k: usize| {
        let a = angle * k as f64 / steps as f64;
        let q = DMat3::from_axis_angle(dir.normalize_or(DVec3::Y), a) * (p.extend(0.0) - a0) + a0;
        pos.transform_point3(q)
    };
    let flip = (angle < 0.0) != (pos.determinant() < 0.0);
    let face = |mut pts: Vec<DVec3>, out: &mut EditMesh| {
        if flip {
            pts.reverse();
        }
        add_face(out, &pts, &[]);
    };
    for lp in std::iter::once(&r.outer).chain(&r.holes) {
        for k in 0..steps {
            for i in 0..lp.len() {
                let (a, b) = (lp[i], lp[(i + 1) % lp.len()]);
                // quad between ring k and k+1, outward like an extrusion's sides
                face(
                    vec![rot(a, k + 1), rot(b, k + 1), rot(b, k), rot(a, k)],
                    out,
                );
            }
        }
    }
    if !full {
        let mut start: Vec<DVec3> = r.outer.iter().map(|&p| rot(p, 0)).collect();
        let mut end: Vec<DVec3> = r.outer.iter().rev().map(|&p| rot(p, steps)).collect();
        let mut sh: Vec<Vec<DVec3>> = r
            .holes
            .iter()
            .map(|h| h.iter().map(|&p| rot(p, 0)).collect())
            .collect();
        let mut eh: Vec<Vec<DVec3>> = r
            .holes
            .iter()
            .map(|h| h.iter().rev().map(|&p| rot(p, steps)).collect())
            .collect();
        // a negative angle turns the solid to the other side of each cap
        // ring, so the caps face round with the sides (which `face` turns
        // above); without this the caps point into the solid
        if flip {
            start.reverse();
            end.reverse();
            for h in sh.iter_mut().chain(eh.iter_mut()) {
                h.reverse();
            }
        }
        add_face(out, &start, &sh);
        add_face(out, &end, &eh);
    }
}

/// A round tube of radius `r` along a polyline, one capped cylinder per segment.
fn tube_mesh(path: &[DVec3], r: f64, out: &mut EditMesh) {
    if r <= 0.0 {
        return;
    }
    let n = 8usize;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let axis = b - a;
        if axis.length_squared() < 1e-12 {
            continue;
        }
        let m = frame(a, axis, None);
        let ring: Vec<DVec2> = (0..n)
            .map(|i| {
                let t = TAU * i as f64 / n as f64 + PI / n as f64;
                DVec2::new(r * t.cos(), r * t.sin())
            })
            .collect();
        if let Some(region) = Region::new(ring, vec![]) {
            extrude_mesh(
                &Extrusion {
                    regions: vec![region],
                    position: m,
                    vector: DVec3::Z * axis.length(),
                },
                out,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step_read::parse;

    fn geo(data: &str) -> StepFile {
        parse(&format!("ISO-10303-21;HEADER;ENDSEC;DATA;{data}ENDSEC;")).unwrap()
    }

    #[test]
    fn rectangle_extrusion_has_the_right_volume() {
        let f = geo(
            "#1=IFCCARTESIANPOINT((0.,0.,0.));#2=IFCAXIS2PLACEMENT3D(#1,$,$);\
            #3=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,2000.,300.);#4=IFCDIRECTION((0.,0.,1.));\
            #5=IFCEXTRUDEDAREASOLID(#3,#2,#4,3000.);",
        );
        let mut g = Geo::new(&f, 0.001, 1.0);
        let mut m = EditMesh::default();
        assert!(g.item(5, &DMat4::IDENTITY, &mut m, 0));
        crate::tess::weld(&mut m, 1e-6);
        assert_eq!(m.faces.len(), 6);
        assert!((m.volume() - 2.0 * 0.3 * 3.0).abs() < 1e-9);
    }

    #[test]
    fn profile_with_arc_and_void() {
        // a 2 x 2 square with a semicircular end and a square hole, via an indexed poly curve
        let f = geo("#1=IFCCARTESIANPOINTLIST2D(((0.,0.),(2.,0.),(3.,1.),(2.,2.),(0.,2.)));\
            #2=IFCINDEXEDPOLYCURVE(#1,(IFCLINEINDEX((1,2)),IFCARCINDEX((2,3,4)),IFCLINEINDEX((4,5,1))),$);\
            #3=IFCCARTESIANPOINT((0.5,0.5));#4=IFCCARTESIANPOINT((1.5,0.5));#5=IFCCARTESIANPOINT((1.5,1.5));\
            #6=IFCCARTESIANPOINT((0.5,1.5));#7=IFCPOLYLINE((#3,#4,#5,#6,#3));\
            #8=IFCARBITRARYPROFILEDEFWITHVOIDS(.AREA.,$,#2,(#7));\
            #9=IFCDIRECTION((0.,0.,1.));#10=IFCEXTRUDEDAREASOLID(#8,$,#9,1.);");
        let mut g = Geo::new(&f, 1.0, 1.0);
        let mut m = EditMesh::default();
        assert!(g.item(10, &DMat4::IDENTITY, &mut m, 0));
        let expected = 4.0 + PI / 2.0 - 1.0;
        assert!((m.volume() - expected).abs() < 0.02, "{}", m.volume());
        m.validate().unwrap();
    }

    #[test]
    fn mapped_items_and_placements_compose() {
        let f = geo(
            "#1=IFCCARTESIANPOINT((10.,0.,0.));#2=IFCAXIS2PLACEMENT3D(#1,$,$);\
            #3=IFCLOCALPLACEMENT($,#2);#4=IFCCARTESIANPOINT((0.,5.,0.));\
            #5=IFCAXIS2PLACEMENT3D(#4,$,$);#6=IFCLOCALPLACEMENT(#3,#5);\
            #10=IFCCARTESIANPOINT((0.,0.,0.));#11=IFCBOUNDINGBOX(#10,1.,1.,1.);\
            #12=IFCSHAPEREPRESENTATION($,'Body','BoundingBox',(#11));\
            #13=IFCAXIS2PLACEMENT3D(#10,$,$);#14=IFCREPRESENTATIONMAP(#13,#12);\
            #15=IFCCARTESIANPOINT((0.,0.,2.));\
            #16=IFCCARTESIANTRANSFORMATIONOPERATOR3D($,$,#15,2.,$);#17=IFCMAPPEDITEM(#14,#16);",
        );
        let mut g = Geo::new(&f, 1.0, 1.0);
        let world = g.placement(&Value::Ref(6));
        assert!((world.w_axis.truncate() - DVec3::new(10.0, 5.0, 0.0)).length() < 1e-12);
        let mut m = EditMesh::default();
        assert!(g.item(17, &world, &mut m, 0));
        let (lo, hi) = crate::tess::bounds(m.vertices.iter().copied()).unwrap();
        assert!((lo - DVec3::new(10.0, 5.0, 2.0)).length() < 1e-9, "{lo}");
        assert!((hi - DVec3::new(12.0, 7.0, 4.0)).length() < 1e-9, "{hi}");
    }

    /// A rectangular section bent over an arc, as a product: the shape
    /// both readings must agree on.
    fn bent_beam() -> StepFile {
        geo(
            "#1=IFCCARTESIANPOINTLIST2D(((5.,-0.15),(5.3,-0.15),(5.3,0.15),(5.,0.15)));\
            #2=IFCINDEXEDPOLYCURVE(#1,(IFCLINEINDEX((1,2,3,4,1))),$);\
            #3=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#2);\
            #4=IFCCARTESIANPOINT((0.,0.,0.));\
            #5=IFCDIRECTION((0.,1.,0.));#6=IFCAXIS1PLACEMENT(#4,#5);\
            #7=IFCCARTESIANPOINT((2.,1.,0.5));#8=IFCDIRECTION((0.,0.,1.));\
            #9=IFCAXIS2PLACEMENT3D(#7,#8,$);\
            #10=IFCREVOLVEDAREASOLID(#3,#9,#6,1.2);\
            #11=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#10));\
            #12=IFCPRODUCTDEFINITIONSHAPE($,$,(#11));\
            #15=IFCAXIS2PLACEMENT3D(#4,$,$);#13=IFCLOCALPLACEMENT($,#15);\
            #14=IFCBEAM('0hUYrQRzX4jeuyugCUcdLX',$,'bent',$,$,#13,#12,$,$);",
        )
    }

    #[test]
    fn a_revolved_solid_maps_to_the_sweep_of_the_same_solid() {
        let f = bent_beam();
        let mut g = Geo::new(&f, 1.0, 1.0);
        let world = g.placement(&Value::Ref(13));
        // the mesh path's reading of the file
        let mut m = EditMesh::default();
        assert!(g.item(10, &DMat4::IDENTITY, &mut m, 0));
        crate::tess::weld(&mut m, 1e-6);
        // the sweep path's reading
        let product = g.f.get(14).unwrap();
        let s = g
            .revolved_sweep(product, &world)
            .expect("mapped")
            .expect("sweepable");
        assert!((s.sweep - 1.2).abs() < 1e-12);
        assert!((s.axis - DVec3::Y).length() < 1e-12);
        assert!((s.center - DVec3::new(2.0, 1.0, 0.5)).length() < 1e-12);
        // the profile rides centred: a 0.3 x 0.3 section
        let (lo, hi) = s.profile.iter().fold(
            (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
            |(a, b), p| (a.min(*p), b.max(*p)),
        );
        assert!(
            (lo.x + 0.15).abs() < 1e-9 && (hi.x - 0.15).abs() < 1e-9,
            "{lo} {hi}"
        );
        assert!(
            (lo.y + 0.15).abs() < 1e-9 && (hi.y - 0.15).abs() < 1e-9,
            "{lo} {hi}"
        );
        // and its swept solid is the same revolution, rings and all: the
        // two tessellations differ only in how finely they sample the arc.
        // Ring centres sit exactly on the arc; the open end rings ride a
        // chord-direction frame (sweep_mesh mitres only interior rings),
        // tilted half a segment angle off the true radial plane, so the
        // bounds agree to profile half-width times segment angle.
        let (pts, closed) = creabim_core::parts::arc_path(s.center, s.axis, s.start, s.sweep);
        let swept = creabim_geom::parts::sweep_mesh_on(&s.profile, &pts, closed, s.axis);
        assert!((m.volume() - swept.volume()).abs() < 0.005 * swept.volume());
        let slop = 0.15 * (s.sweep.abs() / pts.len() as f64) + 1e-3;
        let (alo, ahi) = crate::tess::bounds(m.vertices.iter().copied()).unwrap();
        let (blo, bhi) = crate::tess::bounds(
            swept
                .positions
                .iter()
                .map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64)),
        )
        .unwrap();
        assert!((alo - blo).length() < slop, "{alo} {blo}");
        assert!((ahi - bhi).length() < slop, "{ahi} {bhi}");
    }

    #[test]
    fn a_clockwise_revolve_maps_to_a_negative_sweep() {
        // the same bent beam turning the other way: the sweep runs
        // right-handed negative about the axis, and the solid matches
        let f = geo(
            "#1=IFCCARTESIANPOINTLIST2D(((5.,-0.15),(5.3,-0.15),(5.3,0.15),(5.,0.15)));\
            #2=IFCINDEXEDPOLYCURVE(#1,(IFCLINEINDEX((1,2,3,4,1))),$);\
            #3=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#2);\
            #4=IFCCARTESIANPOINT((0.,0.,0.));\
            #5=IFCDIRECTION((0.,1.,0.));#6=IFCAXIS1PLACEMENT(#4,#5);\
            #7=IFCCARTESIANPOINT((2.,1.,0.5));#8=IFCDIRECTION((0.,0.,1.));\
            #9=IFCAXIS2PLACEMENT3D(#7,#8,$);\
            #10=IFCREVOLVEDAREASOLID(#3,#9,#6,-1.2);\
            #11=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#10));\
            #12=IFCPRODUCTDEFINITIONSHAPE($,$,(#11));\
            #15=IFCAXIS2PLACEMENT3D(#4,$,$);#13=IFCLOCALPLACEMENT($,#15);\
            #14=IFCBEAM('0hUYrQRzX4jeuyugCUcdLX',$,'bent',$,$,#13,#12,$,$);",
        );
        let mut g = Geo::new(&f, 1.0, 1.0);
        let world = g.placement(&Value::Ref(13));
        let product = g.f.get(14).unwrap();
        let s = g
            .revolved_sweep(product, &world)
            .expect("mapped")
            .expect("sweepable");
        assert!((s.sweep + 1.2).abs() < 1e-12);
        let mut m = EditMesh::default();
        assert!(g.item(10, &DMat4::IDENTITY, &mut m, 0));
        crate::tess::weld(&mut m, 1e-6);
        let (pts, closed) = creabim_core::parts::arc_path(s.center, s.axis, s.start, s.sweep);
        let swept = creabim_geom::parts::sweep_mesh_on(&s.profile, &pts, closed, s.axis);
        // signed volumes: the mesh path flips its winding for a negative
        // angle, so the magnitudes agree
        assert!((m.volume().abs() - swept.volume().abs()).abs() < 0.005 * swept.volume().abs());
    }

    #[test]
    fn a_revolved_solid_off_the_sweepable_config_stays_a_mesh() {
        // the turn axis off the profile plane: a0 with a z, a tilted dir.
        // Each solid wrapped in a beam product, as revolved_sweep takes one.
        for axis1 in [
            "#4=IFCCARTESIANPOINT((0.,0.,2.));#5=IFCDIRECTION((0.,1.,0.));",
            "#4=IFCCARTESIANPOINT((0.,0.,0.));#5=IFCDIRECTION((0.5,1.,0.5));",
        ] {
            let f = geo(&format!(
                "#1=IFCCARTESIANPOINTLIST2D(((5.,-0.15),(5.3,-0.15),(5.3,0.15),(5.,0.15)));\
                #2=IFCINDEXEDPOLYCURVE(#1,(IFCLINEINDEX((1,2,3,4,1))),$);\
                #3=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#2);\
                {axis1}#6=IFCAXIS1PLACEMENT(#4,#5);\
                #7=IFCCARTESIANPOINT((0.,0.,0.));#9=IFCAXIS2PLACEMENT3D(#7,$,$);\
                #10=IFCREVOLVEDAREASOLID(#3,#9,#6,1.2);\
                #11=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#10));\
                #12=IFCPRODUCTDEFINITIONSHAPE($,$,(#11));\
                #15=IFCAXIS2PLACEMENT3D(#7,$,$);#13=IFCLOCALPLACEMENT($,#15);\
                #14=IFCBEAM('0hUYrQRzX4jeuyugCUcdLX',$,'off',$,$,#13,#12,$,$);"
            ));
            let mut g = Geo::new(&f, 1.0, 1.0);
            let product = g.f.get(14).unwrap();
            let swept = g
                .revolved_sweep(product, &DMat4::IDENTITY)
                .expect("a single revolved solid");
            assert!(swept.is_err(), "expected a why, got {swept:?}");
        }
        // a body of several items, or a different solid: no reading at all
        let f = geo("#9=IFCAXIS2PLACEMENT3D($,$,$);\
            #11=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#10,#10));\
            #12=IFCPRODUCTDEFINITIONSHAPE($,$,(#11));\
            #15=IFCAXIS2PLACEMENT3D($,$,$);#13=IFCLOCALPLACEMENT($,#15);\
            #14=IFCBEAM('0hUYrQRzX4jeuyugCUcdLX',$,'two',$,$,#13,#12,$,$);");
        let mut g = Geo::new(&f, 1.0, 1.0);
        let product = g.f.get(14).unwrap();
        let world = g.placement(&Value::Ref(13));
        assert!(g.revolved_sweep(product, &world).is_none());
    }

    #[test]
    fn cyclic_references_do_not_hang() {
        let f = geo("#1=IFCLOCALPLACEMENT(#2,$);#2=IFCLOCALPLACEMENT(#1,$);\
            #3=IFCBOOLEANRESULT(.DIFFERENCE.,#3,#3);");
        let mut g = Geo::new(&f, 1.0, 1.0);
        let _ = g.placement(&Value::Ref(1));
        let mut m = EditMesh::default();
        assert!(!g.item(3, &DMat4::IDENTITY, &mut m, 0));
    }

    #[test]
    fn trimmed_circle_in_degrees() {
        let f = geo("#1=IFCCARTESIANPOINT((0.,0.));#2=IFCAXIS2PLACEMENT2D(#1,$);#3=IFCCIRCLE(#2,1.);\
            #4=IFCTRIMMEDCURVE(#3,(IFCPARAMETERVALUE(0.)),(IFCPARAMETERVALUE(90.)),.T.,.PARAMETER.);");
        let mut g = Geo::new(&f, 1.0, PI / 180.0);
        let pts = g.curve(&Value::Ref(4)).unwrap();
        assert!((pts[0] - DVec3::X).length() < 1e-9);
        assert!((pts[pts.len() - 1] - DVec3::Y).length() < 1e-9);
    }
}
