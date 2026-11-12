//! Moving, turning, scaling and copying elements, the same way for every
//! tool that does it (3D, plans, sections, the keyboard or a drag).

use crate::{Element, ElementId, ElementKind, Model, ViewKind};
use glam::{DQuat, DVec2, DVec3};

/// A change of position. `Rotate` turns about the vertical, as building
/// elements stand upright; `Tilt` turns about any other axis and suits only
/// free shapes. `Scale` is uniform; `Stretch` scales each axis on its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Xform {
    Move(DVec3),
    Rotate {
        center: DVec2,
        degrees: f64,
    },
    Tilt {
        center: DVec3,
        /// Unit axis to turn about, right-handed.
        axis: DVec3,
        degrees: f64,
    },
    Scale {
        center: DVec3,
        factor: f64,
    },
    Stretch {
        center: DVec3,
        factor: DVec3,
    },
}

impl Xform {
    pub fn point2(&self, p: DVec2) -> DVec2 {
        match *self {
            Xform::Move(d) => p + d.truncate(),
            Xform::Rotate { center, degrees } => {
                center + DVec2::from_angle(degrees.to_radians()).rotate(p - center)
            }
            Xform::Scale { center, factor } => center.truncate() + (p - center.truncate()) * factor,
            Xform::Stretch { center, factor } => {
                center.truncate() + (p - center.truncate()) * factor.truncate()
            }
            Xform::Tilt { .. } => self.point3(p.extend(0.0)).truncate(),
        }
    }

    pub fn point3(&self, p: DVec3) -> DVec3 {
        match *self {
            Xform::Move(d) => p + d,
            Xform::Rotate { .. } => self.point2(p.truncate()).extend(p.z),
            Xform::Scale { center, factor } => center + (p - center) * factor,
            Xform::Stretch { center, factor } => center + (p - center) * factor,
            Xform::Tilt {
                center,
                axis,
                degrees,
            } => center + DQuat::from_axis_angle(axis, degrees.to_radians()) * (p - center),
        }
    }

    /// A direction (no position) under the change: turned, never moved.
    pub fn direction(&self, v: DVec3) -> DVec3 {
        match *self {
            Xform::Rotate { degrees, .. } => DVec2::from_angle(degrees.to_radians())
                .rotate(v.truncate())
                .extend(v.z),
            Xform::Tilt { axis, degrees, .. } => {
                DQuat::from_axis_angle(axis, degrees.to_radians()) * v
            }
            _ => v,
        }
    }

    /// Does it keep shapes as they are, only moving or turning them?
    pub fn is_rigid(&self) -> bool {
        matches!(
            self,
            Xform::Move(_) | Xform::Rotate { .. } | Xform::Tilt { .. }
        )
    }

    /// Does it keep upright things upright?
    pub(crate) fn keeps_upright(&self) -> bool {
        !matches!(self, Xform::Tilt { .. })
    }

    /// The same change in the opposite direction.
    pub fn inverse(&self) -> Xform {
        match *self {
            Xform::Move(d) => Xform::Move(-d),
            Xform::Rotate { center, degrees } => Xform::Rotate {
                center,
                degrees: -degrees,
            },
            Xform::Scale { center, factor } => Xform::Scale {
                center,
                factor: 1.0 / factor,
            },
            Xform::Stretch { center, factor } => Xform::Stretch {
                center,
                factor: factor.recip(),
            },
            Xform::Tilt {
                center,
                axis,
                degrees,
            } => Xform::Tilt {
                center,
                axis,
                degrees: -degrees,
            },
        }
    }
}

/// `e` after `t`, or `None` when that kind of element can't take it (a door
/// can slide along its wall but not turn on its own; a level only moves up
/// or down). Walls and slabs scale in plan; their heights stay.
pub fn transformed(model: &Model, e: &Element, t: &Xform) -> Option<Element> {
    let mut out = e.clone();
    let elevation = |level: Option<ElementId>| {
        level
            .and_then(|l| model.level(l))
            .map_or(0.0, |l| l.elevation)
    };
    // an object hung from the ceiling measures its height down from there
    let up = if crate::objects::hung(model, e) {
        -1.0
    } else {
        1.0
    };
    // origins of masses and sketches are relative to their level
    let origin = |o: DVec3, level: Option<ElementId>| {
        let z = DVec3::Z * elevation(level);
        t.point3(o + z) - z
    };
    match &mut out.kind {
        ElementKind::Level(l) => match t {
            Xform::Move(d) if d.z != 0.0 => l.elevation += d.z,
            _ => return None,
        },
        ElementKind::Grid(g) => {
            if matches!(t, Xform::Scale { .. }) || !t.keeps_upright() {
                return None;
            }
            g.start = t.point2(g.start);
            g.end = t.point2(g.end);
        }
        // a reference moves, turns and scales about its centre with them
        // a note moves across its view by the move as its view draws it;
        // on a sheet, by the move itself (sheet millimetres)
        ElementKind::Note(n) => match t {
            Xform::Move(d) => {
                let by = match crate::DrawingFrame::of(model, n.view) {
                    Some(f) => {
                        let p = f.to_model(n.at);
                        f.to_drawing(p + *d) - f.to_drawing(p)
                    }
                    None => d.truncate(),
                };
                n.at += by;
                if let Some(e) = &mut n.elbow {
                    *e += by;
                }
            }
            _ => return None,
        },
        // a paper mark only moves across its sheet
        ElementKind::PaperMark(m) => match t {
            Xform::Move(d) => m.at += d.truncate(),
            _ => return None,
        },
        // a reference lives on its views' drawing: in a section that is
        // the section's plane, so a move in the model is laid back onto it
        // (MU33: Move read a section's up as the plan's Y)
        ElementKind::Reference(r) => match ref_frame(model, r) {
            Some(frame) => {
                let on = |p: DVec3| frame.to_drawing(p);
                match t {
                    Xform::Move(_) | Xform::Scale { .. } => {
                        let at = frame.to_model(r.at);
                        r.at = on(t.point3(at));
                        if let Xform::Scale { factor, .. } = t {
                            r.scale *= factor;
                        }
                    }
                    _ => return None,
                }
            }
            None => match t {
                Xform::Move(d) => r.at += d.truncate(),
                Xform::Rotate { degrees, .. } => {
                    r.at = t.point2(r.at);
                    r.rotation = (r.rotation + degrees).rem_euclid(360.0);
                }
                Xform::Scale { factor, .. } => {
                    r.at = t.point2(r.at);
                    r.scale *= factor;
                }
                _ => return None,
            },
        },
        ElementKind::Wall(w) => {
            if !t.keeps_upright() {
                return None;
            }
            w.start = t.point2(w.start);
            w.end = t.point2(w.end);
            match t {
                Xform::Move(d) => {
                    w.base_offset += d.z;
                    // a top tied to a level moves with the wall
                    if let crate::WallTop::UpToLevel { offset, .. } = &mut w.top {
                        *offset += d.z;
                    }
                }
                Xform::Stretch { center, factor } => {
                    if factor.z <= 0.0 {
                        return None;
                    }
                    // a mirror keeps the exterior on the same side of the building
                    if factor.x * factor.y < 0.0 {
                        w.flip = !w.flip;
                    }
                    let level = elevation(Some(w.level));
                    let base = level + w.base_offset;
                    w.base_offset = center.z + (base - center.z) * factor.z - level;
                    w.height *= factor.z;
                }
                _ => {}
            }
        }
        ElementKind::Slab(s) => {
            if !t.keeps_upright() {
                return None;
            }
            for p in s.boundary.iter_mut().chain(s.holes.iter_mut().flatten()) {
                *p = t.point2(*p);
            }
            // a mirror in plan turns arcs the other way
            if let Xform::Stretch { factor, .. } = t {
                if factor.x * factor.y < 0.0 {
                    for b in &mut s.bulges {
                        *b = -*b;
                    }
                }
            }
            s.make_ccw();
            match t {
                Xform::Move(d) => s.top_offset += d.z,
                Xform::Stretch { center, factor } => {
                    if factor.z <= 0.0 {
                        return None;
                    }
                    let level = elevation(Some(s.level));
                    let top = s.top(level);
                    s.top_offset = center.z + (top - center.z) * factor.z - level;
                    s.thickness *= factor.z;
                }
                _ => {}
            }
        }
        ElementKind::Opening(o) => {
            let Xform::Move(d) = t else { return None };
            let ElementKind::Wall(w) = &model.get(o.host)?.kind else {
                return None;
            };
            let max = (w.length() - o.width).max(0.0);
            o.offset = (o.offset + d.truncate().dot(w.direction())).clamp(0.0, max);
            o.sill = (o.sill + d.z).max(0.0);
            crate::walls::clamp_opening(model, o);
        }
        ElementKind::FreeMesh(m) => {
            let z = DVec3::Z * elevation(m.level);
            let before = m.origin + z;
            m.origin = origin(m.origin, m.level);
            match t {
                Xform::Rotate { degrees, .. } => {
                    m.rotation = (m.rotation + degrees).rem_euclid(360.0)
                }
                Xform::Scale { factor, .. } => m.scale *= factor,
                Xform::Move(_) => {}
                Xform::Tilt { .. } | Xform::Stretch { .. } => {
                    // no turn or size factor holds these: build them into
                    // the corners, which the turn and scale then apply to
                    let after = m.origin + z;
                    let old = m.clone();
                    for v in &mut m.mesh.vertices {
                        let p = t.point3(before + old.to_model(*v));
                        *v = old.to_local(p - after);
                    }
                    if let Xform::Stretch { factor, .. } = t {
                        if factor.x * factor.y * factor.z < 0.0 {
                            for f in &mut m.mesh.faces {
                                f.reverse();
                            }
                        }
                        if factor.x * factor.y * factor.z == 0.0 {
                            return None;
                        }
                    }
                }
            }
        }
        ElementKind::Sketch(s) => match t {
            Xform::Stretch { .. } => {
                // curves and dimensions don't survive uneven scaling
                if s.circle.is_some()
                    || !s.constraints.is_empty()
                    || s.kinds.iter().any(|k| *k != crate::SegmentKind::Line)
                {
                    return None;
                }
                // a stretched plane is still a plane: refit its axes
                let z = DVec3::Z * elevation(s.level);
                let pts: Vec<DVec3> = s
                    .model_points()
                    .into_iter()
                    .map(|p| t.point3(p + z) - z)
                    .collect();
                let o = origin(s.plane.origin, s.level);
                let x = (t.point3(s.plane.origin + s.plane.x + z) - z - o).try_normalize()?;
                let y0 = t.point3(s.plane.origin + s.plane.y + z) - z - o;
                let y = (y0 - x * y0.dot(x)).try_normalize()?;
                s.plane.origin = o;
                s.plane.x = x;
                s.plane.y = y;
                s.points = pts.iter().map(|p| s.plane.to_plane(*p)).collect();
            }
            _ => {
                s.plane.origin = origin(s.plane.origin, s.level);
                s.plane.x = t.direction(s.plane.x);
                s.plane.y = t.direction(s.plane.y);
                if let Xform::Scale { factor, .. } = t {
                    s.scale(*factor);
                }
            }
        },
        // a detail view moves its box on its parent; what it looks at follows
        ElementKind::View(v) if v.callout.is_some() => {
            let c = v.callout.as_mut()?;
            if !matches!(t, Xform::Move(_)) {
                return None;
            }
            let frame = crate::DrawingFrame::of(model, c.parent)?;
            let by = frame.to_drawing(t.point3(frame.to_model(c.lo))) - c.lo;
            c.lo += by;
            c.hi += by;
        }
        ElementKind::View(v) => match &mut v.kind {
            ViewKind::Section { start, end, .. } if t.is_rigid() && t.keeps_upright() => {
                *start = t.point2(*start);
                *end = t.point2(*end);
            }
            _ => return None,
        },
        ElementKind::Guide(g) => {
            g.a = origin(g.a, g.level);
            g.b = origin(g.b, g.level);
        }
        ElementKind::Shaft(s) => {
            if !t.keeps_upright() {
                return None;
            }
            for p in &mut s.boundary {
                *p = t.point2(*p);
            }
            if crate::polygon::signed_area2(&s.boundary) < 0.0 {
                s.boundary.reverse();
            }
            if let Xform::Move(d) = t {
                s.base_offset += d.z;
                s.top_offset += d.z;
            }
        }
        ElementKind::Room(r) => {
            if !t.keeps_upright() {
                return None;
            }
            r.at = t.point2(r.at);
            for p in &mut r.boundary {
                *p = t.point2(*p);
            }
            if crate::polygon::signed_area2(&r.boundary) < 0.0 {
                r.boundary.reverse();
            }
        }
        ElementKind::RoomSeparator(s) => {
            if !t.keeps_upright() {
                return None;
            }
            s.a = t.point2(s.a);
            s.b = t.point2(s.b);
        }
        ElementKind::Ceiling(c) => {
            if !t.keeps_upright() {
                return None;
            }
            for p in &mut c.boundary {
                *p = t.point2(*p);
            }
            if crate::polygon::signed_area2(&c.boundary) < 0.0 {
                c.boundary.reverse();
            }
            if let Xform::Move(d) = t {
                c.height += d.z;
            }
        }
        ElementKind::Object(o) => match t {
            // its shape is its type's: it moves and turns, but keeps its size
            Xform::Move(d) => {
                o.position += d.truncate();
                o.offset += up * d.z;
            }
            Xform::Rotate { degrees, .. } if t.keeps_upright() => {
                o.position = t.point2(o.position);
                o.rotation = (o.rotation + degrees).rem_euclid(360.0);
            }
            // a mirror in plan: it goes to the mirrored place, facing the
            // mirrored way (its shape is its type's, so it is not turned over)
            Xform::Stretch { factor, .. } => {
                o.position = t.point2(o.position);
                o.rotation = mirrored_angle(o.rotation, *factor)?;
            }
            _ => return None,
        },
        ElementKind::Column(_)
        | ElementKind::Beam(_)
        | ElementKind::Roof(_)
        | ElementKind::Stair(_)
        | ElementKind::Railing(_)
        | ElementKind::Footing(_)
        | ElementKind::Part(_)
        | ElementKind::Site(_) => crate::members::transform(model, &mut out.kind, t)?,
        // drafting stays on its view's paper: moved, turned, mirrored or
        // scaled in the model, then laid back on the drawing
        ElementKind::Detail(d) => {
            let frame = crate::DrawingFrame::of(model, d.view)?;
            for p in &mut d.points {
                *p = frame.to_drawing(t.point3(frame.to_model(*p)));
            }
            if let Xform::Scale { factor, .. } = t {
                d.width *= factor.abs();
                if let crate::DetailKind::Repeat { module, joint } = &mut d.kind {
                    *module *= factor.abs();
                    *joint *= factor.abs();
                }
            }
            d.validate().ok()?;
        }
        // a crop box stays square to the axes: it only moves
        ElementKind::CropBox(c) => match t {
            Xform::Move(d) => {
                c.min += *d;
                c.max += *d;
            }
            _ => return None,
        },
        // a dimension moves on its drawing (MU32: Move skipped them): its
        // free ends go with it, and a line measuring between things slides
        // square to itself, as a dragged dimension line does
        ElementKind::Dimension(dim) => {
            let Xform::Move(d) = t else { return None };
            let frame = crate::DrawingFrame::of(model, dim.view)?;
            let o = frame.to_model(DVec2::ZERO);
            let dd = frame.to_drawing(o + *d) - frame.to_drawing(o);
            let (a, b) = (drawn_at(model, &dim.a), drawn_at(model, &dim.b));
            let mut moved = false;
            for p in [&mut dim.a, &mut dim.b] {
                if p.host.is_none() {
                    p.local += dd;
                    moved = true;
                }
            }
            if let Some(c) = &mut dim.center {
                if c.host.is_none() {
                    c.local += dd;
                    moved = true;
                }
            }
            if !moved || dim.kind == crate::DimensionKind::Linear {
                if let (Some(a), Some(b)) = (a, b) {
                    let n = (b - a).normalize_or_zero().perp();
                    if dim.a.host.is_some() && dim.b.host.is_some() {
                        dim.offset += dd.dot(n);
                    }
                }
            }
        }
        ElementKind::Sheet(_)
        | ElementKind::Viewport(_)
        | ElementKind::Material(_)
        | ElementKind::Assembly(_)
        | ElementKind::Tag(_)
        | ElementKind::TextStyle(_)
        | ElementKind::SheetTemplate(_)
        | ElementKind::Units(_)
        | ElementKind::Project(_)
        | ElementKind::MarkRules(_)
        | ElementKind::StylePack(_)
        | ElementKind::Cut(_)
        | ElementKind::Join(_)
        | ElementKind::ElementType(_)
        | ElementKind::GridGroup(_)
        | ElementKind::ScriptObject(_) => return None,
    }
    Some(out)
}

/// An angle in plan (degrees) after a mirror that turns `factor`'s axes
/// over, or `None` when `factor` is not a mirror in plan.
pub fn mirrored_angle(degrees: f64, factor: DVec3) -> Option<f64> {
    if (factor.abs() - DVec3::ONE).abs().max_element() > 1e-9 || factor.z < 0.0 {
        return None;
    }
    let a = match (factor.x < 0.0, factor.y < 0.0) {
        (false, false) => degrees,
        (true, false) => 180.0 - degrees,
        (false, true) => -degrees,
        (true, true) => degrees + 180.0,
    };
    Some(a.rem_euclid(360.0))
}

/// The changes that mirror across the plane through `at` facing `normal`,
/// one after the other. Building elements stay upright, so a mirror plane
/// that stands up is turned onto the X axis, flipped there and turned
/// back; a level one flips Z; any other tilts (which only free shapes and
/// parts take). `None` for no normal.
pub fn mirror_xforms(at: DVec3, normal: DVec3) -> Option<Vec<Xform>> {
    let n = normal.try_normalize()?;
    let flip_x = Xform::Stretch {
        center: at,
        factor: DVec3::new(-1.0, 1.0, 1.0),
    };
    // (a face's normal read off its mesh is a hair off level or upright)
    if n.z.abs() < 1e-6 {
        let deg = n.truncate().to_angle().to_degrees();
        let center = at.truncate();
        let turn = |degrees: f64| Xform::Rotate { center, degrees };
        if deg.abs() < 1e-12 {
            return Some(vec![flip_x]);
        }
        return Some(vec![turn(-deg), flip_x, turn(deg)]);
    }
    if n.truncate().length() < 1e-6 {
        return Some(vec![Xform::Stretch {
            center: at,
            factor: DVec3::new(1.0, 1.0, -1.0),
        }]);
    }
    let axis = n.cross(DVec3::X).normalize();
    let degrees = n.angle_between(DVec3::X).to_degrees();
    let tilt = |degrees: f64| Xform::Tilt {
        center: at,
        axis,
        degrees,
    };
    Some(vec![tilt(degrees), flip_x, tilt(-degrees)])
}

/// `e` mirrored across the plane through `at` facing `normal`, or `None`
/// when it can't be (a wall can't be turned upside down, a door goes with
/// its wall).
pub fn mirrored(model: &Model, e: &Element, at: DVec3, normal: DVec3) -> Option<Element> {
    let mut out = e.clone();
    for t in mirror_xforms(at, normal)? {
        out = transformed(model, &out, &t)?;
    }
    Some(out)
}

/// Where an element is, for turning and scaling it about itself.
pub fn pivot(model: &Model, id: ElementId) -> Option<DVec3> {
    let e = model.get(id)?;
    let elevation = |level: Option<ElementId>| {
        level
            .and_then(|l| model.level(l))
            .map_or(0.0, |l| l.elevation)
    };
    let mean = |pts: &[DVec2]| pts.iter().copied().sum::<DVec2>() / pts.len().max(1) as f64;
    Some(match &e.kind {
        ElementKind::Level(l) => DVec3::new(0.0, 0.0, l.elevation),
        ElementKind::Grid(g) => ((g.start + g.end) / 2.0).extend(0.0),
        ElementKind::Wall(w) => {
            ((w.start + w.end) / 2.0).extend(elevation(Some(w.level)) + w.base_offset)
        }
        ElementKind::Slab(s) => mean(&s.boundary).extend(s.top(elevation(Some(s.level)))),
        ElementKind::Opening(o) => {
            let ElementKind::Wall(w) = &model.get(o.host)?.kind else {
                return None;
            };
            (w.start + w.direction() * (o.offset + o.width / 2.0))
                .extend(elevation(Some(w.level)) + w.base_offset + o.sill)
        }
        ElementKind::FreeMesh(m) => m.origin + DVec3::Z * elevation(m.level),
        ElementKind::Sketch(s) => {
            let pts = s.model_points();
            pts.iter().copied().sum::<DVec3>() / pts.len().max(1) as f64
                + DVec3::Z * elevation(s.level)
        }
        ElementKind::View(v) if v.callout.is_some() => {
            let c = v.callout?;
            crate::DrawingFrame::of(model, c.parent)?.to_model((c.lo + c.hi) / 2.0)
        }
        ElementKind::View(v) => match &v.kind {
            ViewKind::Section { start, end, .. } => ((*start + *end) / 2.0).extend(0.0),
            _ => return None,
        },
        ElementKind::Guide(g) => (g.a + g.b) / 2.0 + DVec3::Z * elevation(g.level),
        ElementKind::CropBox(c) => c.center(),
        // (MU23) a reference turns and scales about its middle; a paper
        // mark and a note are where they are written
        ElementKind::Reference(r) => match ref_frame(model, r) {
            Some(f) => f.to_model(r.at),
            None => r.at.extend(0.0),
        },
        ElementKind::PaperMark(m) => m.at.extend(0.0),
        ElementKind::Note(n) => match crate::DrawingFrame::of(model, n.view) {
            Some(f) => f.to_model(n.at),
            None => n.at.extend(0.0),
        },
        ElementKind::Shaft(s) => mean(&s.boundary).extend(s.span(model).0),
        ElementKind::Object(o) => o.position.extend(
            crate::objects::base_z(model, e).unwrap_or(elevation(Some(o.level)) + o.offset),
        ),
        ElementKind::Detail(d) => {
            let frame = crate::DrawingFrame::of(model, d.view)?;
            frame.to_model(mean(&d.points))
        }
        ElementKind::Dimension(d) => {
            let frame = crate::DrawingFrame::of(model, d.view)?;
            let (a, b) = (drawn_at(model, &d.a)?, drawn_at(model, &d.b)?);
            frame.to_model((a + b) / 2.0)
        }
        _ => return crate::members::pivot(model, e),
    })
}

/// The section a reference is shown in, as a drawing frame; none for a
/// plan, whose coordinates are the model's own.
pub fn ref_frame(model: &Model, r: &crate::Reference) -> Option<crate::DrawingFrame> {
    match crate::DrawingFrame::of(model, *r.shown_in.first()?)? {
        f @ crate::DrawingFrame::Section { .. } => Some(f),
        _ => None,
    }
}

/// Where a dimension's point sits on its drawing, as near as the model
/// says without drawing it: free, on a level line, on a wall, or where it
/// was put on a body.
fn drawn_at(model: &Model, p: &crate::HostPoint) -> Option<DVec2> {
    let Some(h) = p.host else {
        return Some(p.local);
    };
    match &model.get(h)?.kind {
        ElementKind::Level(l) => Some(DVec2::new(p.local.x, l.elevation + p.local.y)),
        ElementKind::Wall(w) => {
            let d = w.direction();
            let u = if p.from_end {
                w.length() + p.local.x
            } else {
                p.local.x
            };
            Some(w.start + d * u + d.perp() * p.local.y)
        }
        ElementKind::Opening(o) => match &model.get(o.host)?.kind {
            ElementKind::Wall(w) => {
                let d = w.direction();
                Some(w.start + d * (o.offset + p.local.x) + d.perp() * p.local.y)
            }
            _ => None,
        },
        _ => Some(p.local),
    }
}

/// Copies of an element with fresh ids, ready to insert: a wall comes with
/// its doors and windows. The copy is named "<name> copy".
pub fn duplicate(model: &Model, id: ElementId) -> Vec<Element> {
    let Some(e) = model.get(id) else {
        return vec![];
    };
    if !matches!(
        e.kind,
        ElementKind::Wall(_)
            | ElementKind::Slab(_)
            | ElementKind::Opening(_)
            | ElementKind::FreeMesh(_)
            | ElementKind::Sketch(_)
            | ElementKind::Grid(_)
            | ElementKind::Guide(_)
            | ElementKind::Shaft(_)
            | ElementKind::Object(_)
            | ElementKind::Room(_)
            | ElementKind::RoomSeparator(_)
            | ElementKind::Ceiling(_)
            | ElementKind::Detail(_)
    ) && !crate::members::is_member(&e.kind)
    {
        return vec![];
    }
    let mut copy = Element::new(format!("{} copy", e.name), e.kind.clone());
    // a copy is of the same type
    copy.of_type = e.of_type.clone();
    if let ElementKind::Grid(g) = &mut copy.kind {
        // a copied grid line follows on in its group
        g.label = crate::grids::next_label(model, g.group, g.start, g.end);
        copy.name = format!("Grid {}", crate::grids::display_label(model, g));
    }
    if let ElementKind::Opening(o) = &mut copy.kind {
        // a lone door copy goes beside the original, on the same wall
        if let Some(ElementKind::Wall(w)) = model.get(o.host).map(|h| &h.kind) {
            let next = o.offset + o.width + 0.3;
            o.offset = if next + o.width <= w.length() {
                next
            } else {
                (o.offset - o.width - 0.3).max(0.0)
            };
        }
    }
    let mut out = vec![copy];
    if matches!(e.kind, ElementKind::Wall(_)) {
        for d in model.dependents(id) {
            if let Some(h) = model.get(d) {
                if let ElementKind::Opening(o) = &h.kind {
                    let mut o = o.clone();
                    o.host = out[0].id;
                    let mut c = Element::new(h.name.clone(), ElementKind::Opening(o));
                    c.of_type = h.of_type.clone();
                    out.push(c);
                }
            }
        }
    }
    out
}

/// What copying `ids` takes: the elements themselves, the doors and windows
/// in copied walls, and cuts whose target is copied (with their cutters).
pub fn copy_set(model: &Model, ids: &[ElementId]) -> Vec<ElementId> {
    let mut out: Vec<ElementId> = Vec::new();
    let push = |id: ElementId, out: &mut Vec<ElementId>| {
        if !out.contains(&id) && model.get(id).is_some() {
            out.push(id);
        }
    };
    for id in ids {
        push(*id, &mut out);
    }
    let mut i = 0;
    while i < out.len() {
        let id = out[i];
        for d in model.dependents(id) {
            match model.get(d).map(|e| &e.kind) {
                Some(ElementKind::Opening(_)) => push(d, &mut out),
                Some(ElementKind::Cut(c)) if c.target == id => {
                    push(c.cutter, &mut out);
                    push(d, &mut out);
                }
                _ => {}
            }
        }
        i += 1;
    }
    out
}

/// Fresh copies of `elements`, ready to insert into `model`: new ids, with
/// references among them pointed at the copies. A reference to something
/// neither copied nor in `model` (pasting into another project) goes to the
/// model's first level when it was a level, else the copy is dropped.
/// Returns the copies and how many were dropped.
pub fn fresh_copies(model: &Model, elements: &[Element]) -> (Vec<Element>, usize) {
    let map: std::collections::HashMap<String, String> = elements
        .iter()
        .map(|e| (e.id.0.to_string(), ElementId::new().0.to_string()))
        .collect();
    fn remap(v: &mut serde_json::Value, map: &std::collections::HashMap<String, String>) {
        match v {
            serde_json::Value::String(s) => {
                if let Some(n) = map.get(s.as_str()) {
                    *s = n.clone();
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|x| remap(x, map)),
            serde_json::Value::Object(o) => o.values_mut().for_each(|x| remap(x, map)),
            _ => {}
        }
    }
    let first_level = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Level(l) => Some((e.id, l.elevation)),
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|l| l.0);
    let mut copies: Vec<Element> = Vec::new();
    for e in elements {
        let Ok(mut v) = serde_json::to_value(e) else {
            continue;
        };
        remap(&mut v, &map);
        let Ok(mut c) = serde_json::from_value::<Element>(v) else {
            continue;
        };
        // a copy is a new object, not a mirror of the linked file's
        c.source = None;
        copies.push(c);
    }
    // point missing levels at this model's own
    let known = |id: ElementId, copies: &[Element]| {
        model.get(id).is_some() || copies.iter().any(|c| c.id == id)
    };
    for i in 0..copies.len() {
        let missing: Vec<ElementId> = copies[i]
            .references()
            .into_iter()
            .filter(|r| !known(*r, &copies))
            .collect();
        if missing.is_empty() {
            continue;
        }
        if let Some(level) = first_level {
            let c = &mut copies[i];
            let fix = |l: &mut ElementId| {
                if missing.contains(l) {
                    *l = level;
                }
            };
            match &mut c.kind {
                ElementKind::Wall(w) => fix(&mut w.level),
                ElementKind::Slab(s) => fix(&mut s.level),
                ElementKind::FreeMesh(m) => m.level.iter_mut().for_each(fix),
                ElementKind::Sketch(s) => s.level.iter_mut().for_each(fix),
                ElementKind::Guide(g) => g.level.iter_mut().for_each(fix),
                ElementKind::Object(o) => fix(&mut o.level),
                ElementKind::Room(r) => fix(&mut r.level),
                ElementKind::RoomSeparator(r) => fix(&mut r.level),
                ElementKind::Ceiling(c) => fix(&mut c.level),
                k => {
                    if let Some(l) = crate::members::level_mut(k) {
                        fix(l);
                    }
                }
            }
        }
    }
    // drop what still points nowhere, repeatedly (a door whose wall went)
    let before = copies.len();
    loop {
        let ids: Vec<ElementId> = copies.iter().map(|c| c.id).collect();
        let n = copies.len();
        copies.retain(|c| {
            c.references()
                .into_iter()
                .all(|r| model.get(r).is_some() || ids.contains(&r))
        });
        if copies.len() == n {
            break;
        }
    }
    let dropped = before - copies.len();
    (copies, dropped)
}

/// A section or elevation turned to look the other way along the same
/// line, as one undo step: its ends swap (it looks to the left of start →
/// end), and the dimensions, tags and notes drawn in it are mirrored so
/// they stay on what they marked. `None` for any other element.
pub fn flip_view(model: &Model, view: ElementId) -> Option<crate::Transaction> {
    let before = model.get(view)?;
    let mut after = before.clone();
    let ElementKind::View(v) = &mut after.kind else {
        return None;
    };
    let ViewKind::Section { start, end, .. } = &mut v.kind else {
        return None;
    };
    let length = start.distance(*end);
    std::mem::swap(start, end);
    let label = if matches!(v.kind, ViewKind::Section { cut: false, .. }) {
        "Flip elevation"
    } else {
        "Flip section"
    };
    // the drawing runs along the line from its start: across is mirrored
    let mirror = |p: DVec2| DVec2::new(length - p.x, p.y);
    let free = |h: &mut crate::HostPoint| {
        if h.host.is_none() {
            h.local = mirror(h.local);
        }
    };
    let mut tx = crate::Transaction::new(label).replace(before.clone(), after);
    for e in model.iter() {
        let mut changed = e.clone();
        let mine = match &mut changed.kind {
            ElementKind::Dimension(d) if d.view == view => {
                free(&mut d.a);
                free(&mut d.b);
                if let Some(c) = &mut d.center {
                    free(c);
                }
                // left of a → b is now its right
                d.offset = -d.offset;
                true
            }
            ElementKind::Tag(t) if t.view == view => {
                t.offset.x = -t.offset.x;
                true
            }
            ElementKind::Note(n) if n.view == view => {
                n.at = mirror(n.at);
                if let Some(l) = &mut n.leader {
                    free(l);
                }
                true
            }
            _ => false,
        };
        if mine && changed != *e {
            tx = tx.replace(e.clone(), changed);
        }
    }
    Some(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sample::sample_house, Wall};

    #[test]
    fn a_flipped_section_looks_the_other_way_and_keeps_its_notes_in_place() {
        let mut doc = crate::Document::from_model(sample_house());
        let sec = Element::new(
            "Section X",
            ElementKind::View(crate::View::section(
                DVec2::new(0.0, 3.0),
                DVec2::new(10.0, 3.0),
                8.0,
                true,
            )),
        );
        let id = sec.id;
        let note = Element::new(
            "Note",
            ElementKind::Note(crate::Note::new(id, DVec2::new(2.0, 1.0), "here")),
        );
        let dim = Element::new(
            "Dim",
            ElementKind::Dimension(crate::Dimension::linear(
                id,
                crate::HostPoint::free(DVec2::new(1.0, 0.0)),
                crate::HostPoint::free(DVec2::new(4.0, 0.0)),
                0.5,
            )),
        );
        let (nid, did) = (note.id, dim.id);
        doc.commit(
            crate::Transaction::new("Add")
                .insert(sec)
                .insert(note)
                .insert(dim),
        )
        .unwrap();
        let tx = flip_view(doc.model(), id).unwrap();
        doc.commit(tx).unwrap();
        let ElementKind::View(v) = &doc.model().get(id).unwrap().kind else {
            panic!()
        };
        let ViewKind::Section { start, end, .. } = v.kind else {
            panic!()
        };
        // the same line, run the other way: it now looks south
        assert_eq!((start, end), (DVec2::new(10.0, 3.0), DVec2::new(0.0, 3.0)));
        let ElementKind::Note(n) = &doc.model().get(nid).unwrap().kind else {
            panic!()
        };
        assert_eq!(n.at, DVec2::new(8.0, 1.0));
        let ElementKind::Dimension(d) = &doc.model().get(did).unwrap().kind else {
            panic!()
        };
        assert_eq!(
            (d.a.local, d.b.local),
            (DVec2::new(9.0, 0.0), DVec2::new(6.0, 0.0))
        );
        assert_eq!(d.offset, -0.5);
        assert_eq!(doc.undo_label(), Some("Flip section"));
        doc.undo();
        let ElementKind::View(v) = &doc.model().get(id).unwrap().kind else {
            panic!()
        };
        assert!(matches!(v.kind, ViewKind::Section { start, .. } if start == DVec2::new(0.0, 3.0)));
        // only sections and elevations flip
        let wall = doc
            .model()
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)))
            .unwrap()
            .id;
        assert!(flip_view(doc.model(), wall).is_none());
    }

    #[test]
    fn turning_a_wall_about_its_middle_keeps_its_length() {
        let m = sample_house();
        let (id, w) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        let c = pivot(&m, id).unwrap();
        let t = Xform::Rotate {
            center: c.truncate(),
            degrees: 90.0,
        };
        let ElementKind::Wall(r) = transformed(&m, m.get(id).unwrap(), &t).unwrap().kind else {
            panic!()
        };
        assert!((r.length() - w.length()).abs() < 1e-9);
        assert!(r.direction().dot(w.direction()).abs() < 1e-9);
        let back = transformed(&m, &Element::new("w", ElementKind::Wall(r)), &t.inverse());
        let ElementKind::Wall(Wall { start, .. }) = back.unwrap().kind else {
            panic!()
        };
        assert!(start.distance(w.start) < 1e-9);
    }

    #[test]
    fn doors_slide_along_their_wall_and_stay_inside_it() {
        let m = sample_house();
        let (id, wall_len) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Opening(o) => match &m.get(o.host)?.kind {
                    ElementKind::Wall(w) => Some((e.id, w.length())),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let t = Xform::Move(DVec3::new(500.0, 500.0, 0.3));
        let ElementKind::Opening(o) = transformed(&m, m.get(id).unwrap(), &t).unwrap().kind else {
            panic!()
        };
        // it stops at the end of the wall's clear stretch: at the wall
        // joined at its corner, not inside it (MU23)
        let (_, hi) = crate::walls::clear_span(&m, o.host);
        assert!(hi <= wall_len + 1e-9);
        assert!(
            (o.offset + o.width - hi).abs() < 1e-9,
            "{} {} {hi}",
            o.offset,
            o.width
        );
        let turn = Xform::Rotate {
            center: DVec2::ZERO,
            degrees: 10.0,
        };
        assert!(transformed(&m, m.get(id).unwrap(), &turn).is_none());
    }

    #[test]
    fn duplicating_a_wall_brings_its_openings() {
        let m = sample_house();
        let wall = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)) && !m.dependents(e.id).is_empty())
            .unwrap();
        let copies = duplicate(&m, wall.id);
        let hosted = m
            .dependents(wall.id)
            .into_iter()
            .filter(|d| matches!(m.get(*d).unwrap().kind, ElementKind::Opening(_)))
            .count();
        assert_eq!(copies.len(), 1 + hosted);
        let mut tx = crate::Transaction::new("Duplicate");
        for c in copies {
            tx = tx.insert(c);
        }
        let mut doc = crate::Document::from_model(m);
        doc.commit(tx).unwrap();
    }

    #[test]
    fn a_tilted_mass_keeps_its_volume_and_a_stretched_wall_grows() {
        let m = sample_house();
        let wall = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)))
            .unwrap();
        let ElementKind::Wall(w0) = &wall.kind else {
            panic!()
        };
        let t = Xform::Stretch {
            center: DVec3::ZERO,
            factor: DVec3::new(2.0, 2.0, 1.5),
        };
        let ElementKind::Wall(w) = transformed(&m, wall, &t).unwrap().kind else {
            panic!()
        };
        assert!((w.length() - 2.0 * w0.length()).abs() < 1e-9);
        assert!((w.height - 1.5 * w0.height).abs() < 1e-9);
        let tilt = Xform::Tilt {
            center: DVec3::ZERO,
            axis: DVec3::X,
            degrees: 30.0,
        };
        assert!(transformed(&m, wall, &tilt).is_none());

        let mut mass = crate::FreeMesh::new(
            None,
            DVec3::new(1.0, 2.0, 0.0),
            crate::EditMesh::cuboid(DVec3::ONE),
        );
        mass.rotation = 30.0;
        mass.scale = 2.0;
        let e = Element::new("m", ElementKind::FreeMesh(mass.clone()));
        let world = |f: &crate::FreeMesh| -> Vec<DVec3> {
            f.mesh
                .vertices
                .iter()
                .map(|v| f.origin + f.to_model(*v))
                .collect()
        };
        let tilt = Xform::Tilt {
            center: DVec3::new(1.0, 2.0, 0.0),
            axis: DVec3::Y,
            degrees: 90.0,
        };
        let ElementKind::FreeMesh(r) = transformed(&m, &e, &tilt).unwrap().kind else {
            panic!()
        };
        for (a, b) in world(&mass).iter().zip(world(&r)) {
            assert!(tilt.point3(*a).distance(b) < 1e-9);
        }
    }

    #[test]
    fn pasted_copies_get_new_ids_and_keep_their_doors_in_their_walls() {
        let m = sample_house();
        let wall = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)) && !m.dependents(e.id).is_empty())
            .unwrap();
        let set = copy_set(&m, &[wall.id]);
        assert!(set.len() > 1);
        let elements: Vec<Element> = set.iter().map(|id| m.get(*id).unwrap().clone()).collect();
        let (copies, dropped) = fresh_copies(&m, &elements);
        assert_eq!(dropped, 0);
        assert!(copies.iter().all(|c| m.get(c.id).is_none()));
        let new_wall = copies[0].id;
        for c in &copies[1..] {
            if let ElementKind::Opening(o) = &c.kind {
                assert_eq!(o.host, new_wall);
            }
        }
        // a door pasted alone into an empty project has nowhere to go
        let door = elements[1].clone();
        let empty = Model::new();
        let (c, dropped) = fresh_copies(&empty, &[door]);
        assert!(c.is_empty());
        assert_eq!(dropped, 1);
    }

    #[test]
    fn a_wall_mirrors_across_a_slanted_line_and_keeps_its_outside_out() {
        let m = sample_house();
        let wall = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)))
            .unwrap();
        let ElementKind::Wall(w0) = &wall.kind else {
            panic!()
        };
        // across the line y = x: the plane through the origin facing (1, -1)
        let n = DVec3::new(1.0, -1.0, 0.0);
        let ElementKind::Wall(w) = mirrored(&m, wall, DVec3::ZERO, n).unwrap().kind else {
            panic!()
        };
        let swap = |p: DVec2| DVec2::new(p.y, p.x);
        assert!(w.start.distance(swap(w0.start)) < 1e-9);
        assert!(w.end.distance(swap(w0.end)) < 1e-9);
        assert_eq!(w.flip, !w0.flip);
        // a wall can't be turned upside down
        assert!(mirrored(&m, wall, DVec3::ZERO, DVec3::Z).is_none());
        // a normal a hair off level (read off a face's mesh) is level
        let hair = DVec3::new(1.0, -1.0, 1e-8);
        assert!(mirrored(&m, wall, DVec3::ZERO, hair).is_some());
    }

    #[test]
    fn a_part_on_a_face_mirrors_square_to_it_and_still_stands_out_of_it() {
        use crate::parts::{on_plane, Form, Part};
        let mut m = sample_house();
        let level = m
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Level(_)))
            .unwrap()
            .id;
        let sq = crate::polygon::rectangle(DVec2::ZERO, DVec2::new(2.0, 1.0));
        let mut p = Part::extrusion(level, sq, 0.0, 0.3);
        // on a sloped face, rising along +y
        let up = DVec3::new(0.0, -0.6, 0.8);
        let face = crate::Workplane::facing(DVec3::new(0.0, 0.0, 2.0), up);
        if let Form::Extrusion { plane, .. } = &mut p.form {
            *plane = Some(face);
        }
        let e = Element::new("Box", ElementKind::Part(p));
        m.insert(e.clone()).unwrap();
        let corners = |e: &Element| -> Vec<DVec3> {
            let ElementKind::Part(p) = &e.kind else {
                panic!()
            };
            let Form::Extrusion {
                outline,
                plane: Some(pl),
                height,
                ..
            } = &p.form
            else {
                panic!()
            };
            let z = DVec3::Z * crate::members::elevation(&m, p.level);
            outline
                .iter()
                .flat_map(|q| [on_plane(pl, *q, 0.0) + z, on_plane(pl, *q, *height) + z])
                .collect()
        };
        // mirrored across planes square to the face, one standing up (in
        // turns about the vertical) and one sloped (by tilts): the corners
        // land on the mirror images of the old ones
        for n in [face.x, face.y] {
            let at = face.origin + n + DVec3::Z * crate::members::elevation(&m, level);
            let r = mirrored(&m, &e, at, n).unwrap();
            let reflect = |q: DVec3| q - 2.0 * (q - at).dot(n) * n;
            let before: Vec<DVec3> = corners(&e).into_iter().map(reflect).collect();
            for q in corners(&r) {
                assert!(before.iter().any(|b| b.distance(q) < 1e-9), "{q}");
            }
            // and it still rises out of the face, not into it
            let ElementKind::Part(rp) = &r.kind else {
                panic!()
            };
            let Form::Extrusion { plane: Some(q), .. } = &rp.form else {
                panic!()
            };
            assert!((q.normal() - up.normalize()).length() < 1e-9);
            assert!(rp.validate().is_ok());
        }
    }

    #[test]
    fn a_detail_moves_turns_and_mirrors_on_its_own_paper() {
        use crate::{DetailItem, DetailKind, View};
        let mut m = Model::new();
        let section = Element::new(
            "Section",
            ElementKind::View(View::section(DVec2::ZERO, DVec2::new(0.0, 5.0), 3.0, true)),
        );
        let sid = section.id;
        let p = DVec2::new;
        let d = Element::new(
            "Batt",
            ElementKind::Detail(DetailItem::new(
                sid,
                DetailKind::Batt,
                vec![p(1.0, 0.5), p(2.0, 0.5)],
            )),
        );
        let did = d.id;
        m.insert(section).unwrap();
        m.insert(d).unwrap();
        let points = |e: Element| match e.kind {
            ElementKind::Detail(d) => d.points,
            _ => panic!(),
        };
        // the section runs up +Y: along it is the drawing's x, up is y
        let c = pivot(&m, did).unwrap();
        assert!(c.distance(DVec3::new(0.0, 1.5, 0.5)) < 1e-9);
        let moved = transformed(
            &m,
            m.get(did).unwrap(),
            &Xform::Move(DVec3::new(0.0, 0.5, 0.25)),
        );
        let pts = points(moved.unwrap());
        assert!(pts[0].distance(p(1.5, 0.75)) < 1e-9);
        assert!(pts[1].distance(p(2.5, 0.75)) < 1e-9);
        // a move across the paper changes nothing on it
        let across = transformed(&m, m.get(did).unwrap(), &Xform::Move(DVec3::X));
        assert!(points(across.unwrap())[0].distance(p(1.0, 0.5)) < 1e-9);
        // mirrored about its middle, end for end
        let mirror = Xform::Stretch {
            center: c,
            factor: DVec3::new(1.0, -1.0, 1.0),
        };
        let pts = points(transformed(&m, m.get(did).unwrap(), &mirror).unwrap());
        assert!(pts[0].distance(p(2.0, 0.5)) < 1e-9);
        // and a copy is a detail too
        assert_eq!(duplicate(&m, did).len(), 1);
    }
}

#[cfg(test)]
mod reference_moves {
    use super::*;
    use crate::{Element, Model, RefSource, Reference, View};

    #[test]
    fn a_reference_in_a_section_moves_on_the_section_plane() {
        // a section running along +Y
        let v = Element::new(
            "Section",
            ElementKind::View(View::section(DVec2::ZERO, DVec2::new(0.0, 10.0), 5.0, true)),
        );
        let vid = v.id;
        let model = Model::from_elements([v]).unwrap();
        let mut r = Reference::new(RefSource::View(vid), vec![vid]);
        r.at = DVec2::new(2.0, 1.0);
        let e = Element::new("Ref", ElementKind::Reference(r));
        // 1 m up and 3 m along the section line
        let moved = transformed(&model, &e, &Xform::Move(DVec3::new(0.0, 3.0, 1.0))).unwrap();
        let ElementKind::Reference(r) = moved.kind else {
            panic!()
        };
        assert!((r.at - DVec2::new(5.0, 2.0)).length() < 1e-9, "{:?}", r.at);
    }
}
