//! Sketches: lines, arcs and circles drawn on a workplane, held in shape by
//! dimension constraints (see [`crate::constraint`]). A closed sketch
//! extrudes into a mass, an open one into a surface.

use crate::{constraint::Constraint, editmesh::EditMesh, element::Workplane, ElementId};
use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};
use std::f64::consts::{PI, TAU};

/// The largest angle one straight piece of a drawn or extruded arc turns
/// through: 7.5°, so a whole circle has 48 sides.
pub const ARC_STEP: f64 = PI / 24.0;

/// How a segment runs from its corner to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum SegmentKind {
    #[default]
    Line,
    /// A circular arc. `bulge` is the tangent of a quarter of the angle it
    /// turns through: 1 is a half circle, and it is positive when the arc
    /// turns counter-clockwise (bulging to the right of the chord).
    Arc { bulge: f64 },
}

/// A whole circle, in the plane's axes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Circle {
    pub center: DVec2,
    pub radius: f64,
}

/// One segment's exact shape, in the plane's axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Curve {
    Line(DVec2, DVec2),
    /// Counter-clockwise from angle `start` (radians) when `sweep` is
    /// positive, clockwise when negative.
    Arc {
        center: DVec2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
}

impl Curve {
    /// The arc from `a` to `b` with the given bulge; a line when it is flat.
    pub fn arc(a: DVec2, b: DVec2, bulge: f64) -> Self {
        let c = a.distance(b);
        if bulge.abs() < 1e-9 || c < 1e-12 {
            return Curve::Line(a, b);
        }
        let h = c / 2.0;
        let d = (b - a) / c;
        let right = DVec2::new(d.y, -d.x);
        let s = bulge * h;
        let radius = h * (1.0 + bulge * bulge) / (2.0 * bulge.abs());
        let center = (a + b) / 2.0 + right * (s - s.signum() * radius);
        Curve::Arc {
            center,
            radius,
            start: (a - center).to_angle(),
            sweep: 4.0 * bulge.atan(),
        }
    }

    /// The point a fraction `t` of the way along.
    pub fn at(&self, t: f64) -> DVec2 {
        match *self {
            Curve::Line(a, b) => a.lerp(b, t),
            Curve::Arc {
                center,
                radius,
                start,
                sweep,
            } => center + DVec2::from_angle(start + sweep * t) * radius,
        }
    }

    /// The curve as a polyline, both ends included: arcs in pieces of at
    /// most [`ARC_STEP`].
    pub fn points(&self) -> Vec<DVec2> {
        match *self {
            Curve::Line(a, b) => vec![a, b],
            Curve::Arc { sweep, .. } => {
                let n = ((sweep.abs() / ARC_STEP - 1e-9).ceil() as usize).max(2);
                (0..=n).map(|i| self.at(i as f64 / n as f64)).collect()
            }
        }
    }
}

/// The bulge of the arc from `a` to `b` through `p`; 0 when `p` lies on the
/// line between them.
pub fn bulge_through(a: DVec2, b: DVec2, p: DVec2) -> f64 {
    let (u, v) = (a - p, b - p);
    if u.length() < 1e-12 || v.length() < 1e-12 {
        return 0.0;
    }
    // the arc through p turns through twice the angle its chord misses π by
    let angle = u.angle_to(v).abs();
    let bulge = ((PI - angle) / 2.0).tan().min(1e3);
    // counter-clockwise arcs bulge to the right of a → b
    if (b - a).perp_dot(p - a) < 0.0 {
        bulge
    } else {
        -bulge
    }
}

/// Lines and arcs drawn on a workplane: a profile to extrude into a mass,
/// or construction lines. The plane's origin is relative to the level's
/// elevation, like a mass's, so sketches move with their level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sketch {
    pub level: Option<ElementId>,
    pub plane: Workplane,
    /// Corners in the plane's axes. A circle has none.
    pub points: Vec<DVec2>,
    /// The last point joins the first.
    pub closed: bool,
    /// How each segment (corner `i` to corner `i + 1`) runs. Segments past
    /// the end of the list are straight, so older sketches have none.
    #[serde(default)]
    pub kinds: Vec<SegmentKind>,
    /// Set when the sketch is a whole circle rather than corners.
    #[serde(default)]
    pub circle: Option<Circle>,
    /// Dimensions and relations kept by [`Sketch::solve`].
    #[serde(default)]
    pub constraints: Vec<Constraint>,
}

impl Sketch {
    /// Straight lines through `points`.
    pub fn polyline(
        level: Option<ElementId>,
        plane: Workplane,
        points: Vec<DVec2>,
        closed: bool,
    ) -> Self {
        Self {
            level,
            plane,
            points,
            closed,
            kinds: vec![],
            circle: None,
            constraints: vec![],
        }
    }

    /// A whole circle.
    pub fn circle(level: Option<ElementId>, plane: Workplane, center: DVec2, radius: f64) -> Self {
        Self {
            circle: Some(Circle { center, radius }),
            closed: true,
            ..Self::polyline(level, plane, vec![], true)
        }
    }

    /// How many segments it has; a circle counts as one.
    pub fn segment_count(&self) -> usize {
        let n = self.points.len();
        match (self.circle, self.closed) {
            (Some(_), _) => 1,
            (None, true) => n,
            (None, false) => n.saturating_sub(1),
        }
    }

    pub fn kind(&self, segment: usize) -> SegmentKind {
        self.kinds.get(segment).copied().unwrap_or_default()
    }

    /// The corners a segment runs between. A circle's are both its centre.
    pub fn ends(&self, segment: usize) -> (DVec2, DVec2) {
        if let Some(c) = self.circle {
            return (c.center, c.center);
        }
        let n = self.points.len();
        (self.points[segment % n], self.points[(segment + 1) % n])
    }

    /// The segment's exact shape.
    pub fn curve(&self, segment: usize) -> Curve {
        if let Some(c) = self.circle {
            return Curve::Arc {
                center: c.center,
                radius: c.radius,
                start: 0.0,
                sweep: TAU,
            };
        }
        let (a, b) = self.ends(segment);
        match self.kind(segment) {
            SegmentKind::Line => Curve::Line(a, b),
            SegmentKind::Arc { bulge } => Curve::arc(a, b, bulge),
        }
    }

    pub fn curves(&self) -> Vec<Curve> {
        (0..self.segment_count()).map(|i| self.curve(i)).collect()
    }

    /// The straight distance between a segment's ends.
    pub fn chord(&self, segment: usize) -> f64 {
        let (a, b) = self.ends(segment);
        a.distance(b)
    }

    /// An arc's or the circle's radius; `None` for a straight segment.
    pub fn radius(&self, segment: usize) -> Option<f64> {
        match self.curve(segment) {
            Curve::Arc { radius, .. } => Some(radius),
            Curve::Line(..) => None,
        }
    }

    /// The whole outline as corners in the plane's axes, arcs in pieces.
    /// A closed outline doesn't repeat its first point.
    pub fn outline(&self) -> Vec<DVec2> {
        let mut out: Vec<DVec2> = Vec::new();
        for c in self.curves() {
            let pts = c.points();
            let skip = usize::from(!out.is_empty());
            out.extend(pts.into_iter().skip(skip));
        }
        if out.is_empty() {
            out = self.points.clone();
        }
        if self.closed && out.len() > 1 && out[0].distance(out[out.len() - 1]) < 1e-9 {
            out.pop();
        }
        out
    }

    /// The outline's corners in model coordinates, relative to the level's
    /// elevation.
    pub fn model_points(&self) -> Vec<DVec3> {
        self.outline()
            .iter()
            .map(|p| self.plane.to_model(*p))
            .collect()
    }

    /// Each straight piece of the sketch as a pair of points in model
    /// coordinates; arcs come in short pieces.
    pub fn segments(&self) -> Vec<(DVec3, DVec3)> {
        let mut out = Vec::new();
        for c in self.curves() {
            let pts: Vec<DVec3> = c.points().iter().map(|p| self.plane.to_model(*p)).collect();
            out.extend(pts.windows(2).map(|w| (w[0], w[1])));
        }
        out
    }

    /// Whether it can be closed: three corners, or two joined by an arc.
    pub fn can_close(&self) -> bool {
        let arc = |i| matches!(self.kind(i), SegmentKind::Arc { .. });
        self.circle.is_none()
            && match self.points.len() {
                0 | 1 => false,
                2 => arc(0) || arc(1),
                _ => true,
            }
    }

    /// Close or open the outline. Opening it drops the constraints on the
    /// closing segment, which no longer exists.
    pub fn set_closed(&mut self, closed: bool) {
        self.closed = closed && self.can_close();
        let count = self.segment_count();
        let points = self.points.len();
        self.constraints.retain(|c| c.fits(count, points));
    }

    /// Scale about the plane's origin, dimensions included.
    pub fn scale(&mut self, factor: f64) {
        for p in &mut self.points {
            *p *= factor;
        }
        if let Some(c) = &mut self.circle {
            c.center *= factor;
            c.radius *= factor.abs();
        }
        for c in &mut self.constraints {
            if let Some(v) = c.value_mut() {
                *v *= factor.abs();
            }
        }
    }

    /// Why the sketch is unusable, if it is.
    pub fn check(&self) -> Result<(), String> {
        let p = &self.plane;
        let square = (p.x.length() - 1.0).abs() < 1e-6
            && (p.y.length() - 1.0).abs() < 1e-6
            && p.x.dot(p.y).abs() < 1e-6;
        if !(square && p.origin.is_finite()) {
            return Err("sketch plane needs perpendicular unit axes".into());
        }
        if self.points.iter().any(|q| !q.is_finite()) {
            return Err("sketch points must be finite".into());
        }
        let bent = self.kinds.iter().any(|k| match k {
            SegmentKind::Arc { bulge } => !bulge.is_finite(),
            SegmentKind::Line => false,
        });
        if bent {
            return Err("sketch arcs must be finite".into());
        }
        if let Some(c) = self.circle {
            if !(c.center.is_finite() && c.radius.is_finite() && c.radius > 1e-9) {
                return Err("a circle needs a positive radius".into());
            }
            if !self.points.is_empty() {
                return Err("a circle sketch has no corners".into());
            }
        } else {
            match (self.points.len(), self.closed) {
                (0 | 1, _) => return Err("a sketch needs at least 2 points".into()),
                (2, true) if !self.can_close() => {
                    return Err("a closed sketch needs at least 3 points, or an arc".into())
                }
                _ => {}
            }
        }
        for c in &self.constraints {
            c.check(self)?;
        }
        Ok(())
    }

    /// The profile pushed `distance` along the plane's normal: a closed solid
    /// for a closed sketch, a surface (to solidify) for an open one. Arcs
    /// become flat faces of at most [`ARC_STEP`] each. Vertices are relative
    /// to the plane's origin, ready for a mass placed there.
    pub fn extrude(&self, distance: f64) -> EditMesh {
        let mut pts = self.outline();
        if self.closed && crate::polygon::signed_area2(&pts) < 0.0 {
            pts.reverse();
        }
        let n = self.plane.normal() * distance;
        let flat = |p: DVec2| self.plane.x * p.x + self.plane.y * p.y;
        let m = pts.len() as u32;
        let mut mesh = EditMesh::default();
        mesh.vertices.extend(pts.iter().map(|p| flat(*p)));
        mesh.vertices.extend(pts.iter().map(|p| flat(*p) + n));
        let edges = if self.closed { m } else { m - 1 };
        for i in 0..edges {
            let j = (i + 1) % m;
            mesh.faces.push(vec![i, j, j + m, i + m]);
        }
        if self.closed {
            mesh.faces.insert(0, (0..m).rev().collect());
            mesh.faces.insert(1, (m..2 * m).collect());
        }
        if distance < 0.0 {
            for f in &mut mesh.faces {
                f.reverse();
            }
        }
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l_shape(plane: Workplane, closed: bool) -> Sketch {
        Sketch::polyline(
            None,
            plane,
            vec![
                DVec2::new(0.0, 0.0),
                DVec2::new(4.0, 0.0),
                DVec2::new(4.0, 2.0),
                DVec2::new(2.0, 2.0),
                DVec2::new(2.0, 4.0),
                DVec2::new(0.0, 4.0),
            ],
            closed,
        )
    }

    #[test]
    fn workplanes_on_walls_keep_their_x_axis_level() {
        let p = Workplane::facing(DVec3::new(1.0, 2.0, 3.0), DVec3::new(0.0, -1.0, 0.0));
        assert!(p.x.z.abs() < 1e-12 && p.normal().distance(-DVec3::Y) < 1e-12);
        let q = DVec3::new(4.0, 2.0, 7.0);
        assert!(p.to_model(p.to_plane(q)).distance(q) < 1e-12);
        let hit = p.intersect(DVec3::new(0.0, -5.0, 0.0), DVec3::Y).unwrap();
        assert!((hit.y - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_closed_sketch_extrudes_into_a_solid_on_any_plane() {
        for plane in [
            Workplane::ground(0.0),
            Workplane::facing(DVec3::ZERO, DVec3::new(1.0, 1.0, 0.5)),
        ] {
            for d in [2.0, -2.0] {
                let m = l_shape(plane, true).extrude(d);
                m.validate().unwrap();
                assert!((m.volume() - 24.0).abs() < 1e-9, "{d}");
                // the concave caps triangulate with n - 2 triangles each
                let caps: Vec<_> = (0..2).map(|f| m.face_triangles(f)).collect();
                assert!(caps.iter().all(|t| t.len() == 4));
            }
        }
    }

    #[test]
    fn an_open_sketch_extrudes_into_a_surface() {
        let m = l_shape(Workplane::ground(0.0), false).extrude(3.0);
        assert_eq!(m.faces.len(), 5);
        assert_eq!(m.vertices.len(), 12);
    }

    #[test]
    fn a_bulge_of_one_is_a_half_circle_through_its_side() {
        let (a, b) = (DVec2::new(1.0, 0.0), DVec2::new(-1.0, 0.0));
        let through = DVec2::new(0.0, 1.0);
        let bulge = bulge_through(a, b, through);
        assert!((bulge - 1.0).abs() < 1e-12);
        let Curve::Arc { center, radius, .. } = Curve::arc(a, b, bulge) else {
            panic!("expected an arc");
        };
        assert!(center.length() < 1e-12 && (radius - 1.0).abs() < 1e-12);
        let arc = Curve::arc(a, b, bulge);
        assert!(arc.at(0.5).distance(through) < 1e-12);
        assert!(arc.at(1.0).distance(b) < 1e-12);
        // the other way round bulges the other side
        let below = Curve::arc(a, b, bulge_through(a, b, -through));
        assert!(below.at(0.5).distance(-through) < 1e-12);
        // a shallow arc keeps its centre beyond the chord
        let shallow = Curve::arc(a, b, bulge_through(a, b, DVec2::new(0.0, 0.25)));
        assert!(shallow.at(0.5).distance(DVec2::new(0.0, 0.25)) < 1e-12);
        assert!(bulge_through(a, b, DVec2::ZERO).abs() < 1e-12);
    }

    #[test]
    fn bulge_is_the_tangent_of_a_quarter_sweep() {
        use std::f64::consts::FRAC_PI_2;
        // The arc_wall conversion rests on this: sweep s ⟺ bulge tan(s/4),
        // counter-clockwise positive, bulging right of start → end.
        let (a, b) = (DVec2::new(2.0, 0.0), DVec2::new(0.0, 2.0));
        let through = DVec2::new(2.0_f64.sqrt(), 2.0_f64.sqrt());
        let bulge = bulge_through(a, b, through);
        assert!((bulge - (FRAC_PI_2 / 4.0).tan()).abs() < 1e-12);
        let arc = Curve::arc(a, b, bulge);
        assert!(arc.at(0.5).distance(through) < 1e-9);
        assert!(arc.at(1.0).distance(b) < 1e-12);
        // Clockwise mirrors it: same chord, negative bulge, other side.
        let (c, d) = (DVec2::new(2.0, 0.0), DVec2::new(0.0, -2.0));
        let under = DVec2::new(2.0_f64.sqrt(), -2.0_f64.sqrt());
        let bulge = bulge_through(c, d, under);
        assert!((bulge + (FRAC_PI_2 / 4.0).tan()).abs() < 1e-12);
        let arc = Curve::arc(c, d, bulge);
        assert!(arc.at(0.5).distance(under) < 1e-9);
        // A quarter of the sweep back: the builder's formula inverts.
        assert!((4.0 * bulge.atan() + FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn circles_and_arcs_extrude_in_pieces() {
        let plane = Workplane::ground(0.0);
        let c = Sketch::circle(None, plane, DVec2::new(1.0, 1.0), 2.0);
        c.check().unwrap();
        let m = c.extrude(3.0);
        m.validate().unwrap();
        assert_eq!(c.outline().len(), 48);
        let exact = PI * 4.0 * 3.0;
        assert!((m.volume() - exact).abs() / exact < 0.005);

        // a D: a straight side closed by a half circle
        let mut d = Sketch::polyline(
            None,
            plane,
            vec![DVec2::new(0.0, 2.0), DVec2::new(0.0, -2.0)],
            true,
        );
        d.kinds = vec![SegmentKind::Line, SegmentKind::Arc { bulge: 1.0 }];
        d.check().unwrap();
        let m = d.extrude(1.0);
        m.validate().unwrap();
        assert!((m.volume() - PI * 2.0).abs() < 0.05);
        assert_eq!(d.segments().len(), 1 + 24);
    }

    #[test]
    fn opening_a_sketch_drops_constraints_on_its_closing_side() {
        let mut s = l_shape(Workplane::ground(0.0), true);
        s.constraints = vec![
            Constraint::Vertical { segment: 5 },
            Constraint::Horizontal { segment: 0 },
        ];
        s.set_closed(false);
        assert_eq!(s.constraints, vec![Constraint::Horizontal { segment: 0 }]);
        s.check().unwrap();
    }

    #[test]
    fn sketches_saved_before_arcs_load_unchanged() {
        // the shape sketches were saved in before arcs and constraints
        #[derive(Serialize)]
        struct OldSketch {
            level: Option<ElementId>,
            plane: Workplane,
            points: Vec<DVec2>,
            closed: bool,
        }
        let old = OldSketch {
            level: Some(ElementId::new()),
            plane: Workplane::facing(DVec3::new(1.0, 2.0, 0.5), DVec3::new(0.0, -1.0, 0.0)),
            points: vec![DVec2::ZERO, DVec2::new(3.0, 0.0), DVec2::new(3.0, 2.5)],
            closed: true,
        };
        let json = serde_json::to_string(&old).unwrap();
        let read: Sketch = serde_json::from_str(&json).unwrap();
        let expected = Sketch::polyline(old.level, old.plane, old.points.clone(), true);
        assert_eq!(read, expected);
        assert_eq!(read.outline(), old.points);
        // and saved again, it reads back the same
        let again: Sketch = serde_json::from_str(&serde_json::to_string(&read).unwrap()).unwrap();
        assert_eq!(again, read);
    }
}
