//! Plan drawings generated from the model: walls cut at a height, openings
//! drawn with architectural symbols, everything below shown in projection.

use crate::wall::Cut;
use creabim_core::{Curve, ElementId, ElementKind, Model, OpeningKind};
use glam::{DVec2, DVec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pen {
    /// Material cut by the plan plane: heavy line, filled.
    Cut,
    /// Visible below the cut plane: light line.
    Projection,
    /// Symbols such as door swings: light line.
    Symbol,
    /// Small things seen below the cut, furniture and fixtures: the
    /// lightest line.
    Fine,
    /// Grid lines: dashed.
    Grid,
    /// Construction guides: shown while working, never printed.
    Guide,
    /// Above the cut plane (beams, the stair past its break): dashed.
    Hidden,
    /// Another level traced under a plan: halftone, never printed, not
    /// picked, but snapped to.
    Underlay,
}

pub use creabim_core::Hatch;

/// The hatch for an element's cut faces: its material's, or its type's default.
pub fn hatch_of(model: &Model, id: ElementId) -> Hatch {
    let Some(e) = model.get(id) else {
        return Hatch::Solid;
    };
    if let Some((_, m)) = creabim_core::material_of(model, e) {
        return m.hatch;
    }
    match &e.kind {
        ElementKind::Wall(_) => Hatch::Diagonal,
        ElementKind::Slab(_) => Hatch::Cross,
        _ => Hatch::Solid,
    }
}

/// Hatch line segments clipped to `polygon`, with a base spacing of
/// `spacing` drawing units. Stipple dots come back as very short segments.
pub fn hatch_lines(hatch: Hatch, polygon: &[DVec2], spacing: f64) -> Vec<(DVec2, DVec2)> {
    let s = spacing;
    let deg = |d: f64| d.to_radians();
    match hatch {
        Hatch::Empty | Hatch::Solid => vec![],
        Hatch::Diagonal => scan_lines(polygon, deg(45.0), s, 0.0),
        Hatch::Cross => {
            let mut v = scan_lines(polygon, deg(45.0), s * 1.4, 0.0);
            v.extend(scan_lines(polygon, deg(-45.0), s * 1.4, 0.0));
            v
        }
        Hatch::Brick => {
            let mut v = scan_lines(polygon, deg(45.0), s * 2.0, 0.0);
            v.extend(scan_lines(polygon, deg(45.0), s * 2.0, s * 0.5));
            v
        }
        Hatch::Concrete => {
            let mut v = scan_lines(polygon, deg(45.0), s * 4.0, 0.0);
            v.extend(stipple(polygon, s * 1.3));
            v
        }
        Hatch::Timber => {
            // grain runs along the polygon's longest side
            let n = polygon.len();
            let angle = (0..n)
                .map(|i| polygon[(i + 1) % n] - polygon[i])
                .max_by(|a, b| a.length().total_cmp(&b.length()))
                .map_or(0.0, |d| d.to_angle());
            scan_lines(polygon, angle, s * 1.2, s * 0.3)
        }
    }
}

/// A staggered grid of dots inside a polygon.
fn stipple(polygon: &[DVec2], spacing: f64) -> Vec<(DVec2, DVec2)> {
    if polygon.len() < 3 || spacing <= 0.0 {
        return vec![];
    }
    let (lo, hi) = polygon
        .iter()
        .fold((DVec2::MAX, DVec2::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
    let dot = DVec2::new(spacing * 0.12, 0.0);
    let mut out = Vec::new();
    let mut j = (lo.y / spacing).floor() as i64;
    while (j as f64) * spacing <= hi.y {
        let y = j as f64 * spacing;
        // offset alternate rows, and jitter a little so it reads as stipple
        let shift = if j % 2 == 0 { 0.0 } else { 0.5 };
        let mut i = (lo.x / spacing).floor() as i64 - 1;
        while (i as f64) * spacing <= hi.x {
            let h = ((i * 73 + j * 151).rem_euclid(97)) as f64 / 97.0 - 0.5;
            let p = DVec2::new(
                (i as f64 + shift + h * 0.4) * spacing,
                y + h * 0.3 * spacing,
            );
            if point_in(p, polygon) {
                out.push((p, p + dot));
            }
            i += 1;
        }
        j += 1;
    }
    out
}

fn point_in(p: DVec2, poly: &[DVec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// Parallel lines at `angle`, `spacing` apart and shifted by `phase`,
/// clipped to a polygon (even-odd).
fn scan_lines(polygon: &[DVec2], angle: f64, spacing: f64, phase: f64) -> Vec<(DVec2, DVec2)> {
    if polygon.len() < 3 || spacing <= 0.0 {
        return vec![];
    }
    // rotate so the lines run along x; lines sit at y = k * spacing, so
    // neighbouring polygons share one continuous pattern
    let (dir, nrm) = (DVec2::from_angle(angle), DVec2::from_angle(angle).perp());
    let local: Vec<DVec2> = polygon
        .iter()
        .map(|p| DVec2::new(p.dot(dir), p.dot(nrm) - phase))
        .collect();
    let (y0, y1) = local
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
    let mut out = Vec::new();
    let mut k = (y0 / spacing).ceil() as i64;
    while (k as f64) * spacing < y1 {
        let y = k as f64 * spacing;
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..local.len() {
            let (a, b) = (local[i], local[(i + 1) % local.len()]);
            if (a.y > y) != (b.y > y) {
                xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.chunks(2) {
            if let [xa, xb] = pair {
                let y = y + phase;
                out.push((dir * *xa + nrm * y, dir * *xb + nrm * y));
            }
        }
        k += 1;
    }
    out
}

/// Which cut wins where two overlap: higher numbers cut through lower ones.
fn cut_priority(model: &Model, id: ElementId) -> u8 {
    model
        .get(id)
        .map_or(0, |e| creabim_core::joins::category_rank(&e.kind))
}

/// Elements of the same material (or, without materials, the same type)
/// merge into one mass where they touch; different ones keep a line between.
fn material_key(model: &Model, id: ElementId) -> (Option<ElementId>, u8) {
    match model.get(id) {
        Some(e) => match creabim_core::material_of(model, e) {
            Some((m, _)) => (Some(m.id), 0),
            None => (None, 1 + cut_priority(model, id)),
        },
        None => (None, 0),
    }
}

/// A piece of cut material: an element's cut, or one layer's of a layered
/// element. [`clean_up_pieces`] turns them into fills and outlines.
#[derive(Clone, Debug, PartialEq)]
pub struct CutPiece {
    pub id: ElementId,
    pub points: Vec<DVec2>,
    pub layer: Option<crate::scene::LayerPart>,
}

/// What decides which cut wins where two overlap: the junction rules set
/// by hand, and the order elements were drawn in.
struct Joining {
    rules: Vec<creabim_core::JoinRule>,
    order: std::collections::HashMap<ElementId, usize>,
}

impl Joining {
    fn of(model: &Model) -> Self {
        Self {
            rules: creabim_core::joins::rules(model),
            order: model.iter().enumerate().map(|(i, e)| (e.id, i)).collect(),
        }
    }

    /// Does `a` cut through `b` where they overlap? By the junction's own
    /// rule if it has one, else layer by layer: the layer with the lower
    /// join priority runs through (cores join cores, finishes stop at the
    /// structure), then columns through walls through slabs, then the
    /// element drawn first. An element without layers is all structure.
    fn cuts(&self, model: &Model, a: &CutPiece, b: &CutPiece) -> bool {
        if a.id == b.id {
            return false;
        }
        if let Some(r) = self.rules.iter().find(|r| r.joins(a.id, b.id)) {
            if !r.cleanup {
                return false;
            }
            if let Some(w) = r.wins(a.id, b.id) {
                return w;
            }
        }
        let strength = |p: &CutPiece| {
            (
                std::cmp::Reverse(p.layer.map_or(1, |l| l.priority)),
                cut_priority(model, p.id),
            )
        };
        let (sa, sb) = (strength(a), strength(b));
        if sa != sb {
            return sa > sb;
        }
        // as strong: the same material merges into one mass where they
        // overlap (see the outlines below); different ones are told
        // apart by the order they were drawn in
        if a.key(model) == b.key(model) {
            return false;
        }
        let order = |p: &CutPiece| self.order.get(&p.id).copied().unwrap_or(usize::MAX);
        order(a) < order(b)
    }
}

impl CutPiece {
    /// What it merges with: the same material, or for a layer without
    /// one, layers of the same function (air gaps run on into each other).
    fn key(&self, model: &Model) -> (Option<ElementId>, u8) {
        match self.layer {
            None => material_key(model, self.id),
            Some(l) => match l.material {
                Some(m) if model.get(m).is_some() => (Some(m), 0),
                _ if l.function == creabim_core::LayerFunction::AirGap => (None, 250),
                _ => (None, 1 + cut_priority(model, self.id)),
            },
        }
    }

    /// The colour of the material this piece cuts: its layer's, else the
    /// element's own (or its core layer's); none for an air gap.
    fn colour(&self, model: &Model) -> Option<[f32; 3]> {
        let mat = |id: ElementId| match &model.get(id)?.kind {
            ElementKind::Material(m) => Some(m.color),
            _ => None,
        };
        match self.layer {
            Some(l) => l.material.and_then(mat),
            None => {
                let e = model.get(self.id)?;
                creabim_core::material_of(model, e)
                    .map(|(_, m)| m.color)
                    .or_else(|| {
                        let (_, a) = creabim_core::assembly::assembly_of(model, e)?;
                        let i = (0..a.layers.len()).find(|i| a.is_core(*i))?;
                        mat(a.layers[i].material?)
                    })
            }
        }
    }

    fn hatch(&self, model: &Model) -> Hatch {
        match self.layer {
            None => hatch_of(model, self.id),
            Some(l) => match l.material.and_then(|m| model.get(m)).map(|e| &e.kind) {
                Some(ElementKind::Material(m)) => m.hatch,
                _ if l.function == creabim_core::LayerFunction::AirGap => Hatch::Empty,
                _ => match model.get(self.id).map(|e| &e.kind) {
                    Some(ElementKind::Wall(_)) => Hatch::Diagonal,
                    Some(ElementKind::Slab(_)) => Hatch::Cross,
                    _ => Hatch::Solid,
                },
            },
        }
    }
}

fn is_convex(poly: &[DVec2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0;
    for i in 0..n {
        let (a, b, c) = (poly[i], poly[(i + 1) % n], poly[(i + 2) % n]);
        let z = (b - a).perp_dot(c - b);
        if z.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = z.signum();
        } else if z.signum() != sign {
            return false;
        }
    }
    sign != 0.0
}

fn signed_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| poly[i].perp_dot(poly[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

/// Keep the part of `poly` on the side of a → b where `keep(side)` holds,
/// side being the signed distance (left positive).
fn clip_half(poly: &[DVec2], a: DVec2, b: DVec2, left: bool) -> Vec<DVec2> {
    let d = (b - a).normalize_or_zero();
    let side = |p: DVec2| {
        let s = d.perp_dot(p - a);
        if left {
            s
        } else {
            -s
        }
    };
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            out.push(p + (q - p) * (sp / (sp - sq)));
        }
    }
    out
}

/// `subject` minus the convex polygon `hole`, as convex-ish pieces.
pub fn subtract_convex(subject: &[DVec2], hole: &[DVec2]) -> Vec<Vec<DVec2>> {
    let ccw = signed_area(hole) > 0.0;
    let n = hole.len();
    let mut rest = subject.to_vec();
    let mut pieces = Vec::new();
    for i in 0..n {
        let (a, b) = (hole[i], hole[(i + 1) % n]);
        // outside this edge of the hole is kept; inside goes on to the next edge
        let outside = clip_half(&rest, a, b, !ccw);
        if outside.len() >= 3 && signed_area(&outside).abs() > 1e-9 {
            pieces.push(outside);
        }
        rest = clip_half(&rest, a, b, ccw);
        if rest.len() < 3 {
            break;
        }
    }
    pieces
}

/// Do two polygons' bounding boxes overlap by more than a sliver?
fn boxes_overlap(a: &[DVec2], b: &[DVec2]) -> bool {
    let bb = |p: &[DVec2]| {
        p.iter().fold((DVec2::MAX, DVec2::MIN), |(lo, hi), q| {
            (lo.min(*q), hi.max(*q))
        })
    };
    let ((alo, ahi), (blo, bhi)) = (bb(a), bb(b));
    let eps = 1e-6;
    alo.x < bhi.x - eps && blo.x < ahi.x - eps && alo.y < bhi.y - eps && blo.y < ahi.y - eps
}

/// The stretches of segment `a` → `b` running strictly inside `poly`, as
/// distances along it.
fn inside_spans(a: DVec2, b: DVec2, poly: &[DVec2], eps: f64) -> Vec<(f64, f64)> {
    let len = a.distance(b);
    if len < eps || poly.len() < 3 {
        return vec![];
    }
    let d = (b - a) / len;
    let mut ts = vec![0.0, len];
    let n = poly.len();
    for i in 0..n {
        let (p, q) = (poly[i], poly[(i + 1) % n]);
        let e = q - p;
        let den = d.perp_dot(e);
        if den.abs() < 1e-12 {
            continue;
        }
        // a + d t = p + e s
        let t = (p - a).perp_dot(e) / den;
        let s = (p - a).perp_dot(d) / den;
        if (-1e-9..=1.0 + 1e-9).contains(&s) && t > 0.0 && t < len {
            ts.push(t);
        }
    }
    ts.sort_by(|x, y| x.total_cmp(y));
    ts.dedup_by(|x, y| (*x - *y).abs() < eps);
    ts.windows(2)
        .filter(|w| w[1] > w[0] + eps)
        .filter(|w| {
            let m = a + d * ((w[0] + w[1]) / 2.0);
            point_in(m, poly) && edge_distance(m, poly) > eps
        })
        .map(|w| (w[0], w[1]))
        .collect()
}

/// Turn cut polygons into fills plus outlines, leaving out outline segments
/// shared with a neighbouring cut so joined elements read as one mass.
pub fn clean_up_cuts(model: &Model, items: &mut Vec<PlanItem>) {
    clean_up_pieces(model, items, vec![]);
}

/// [`clean_up_cuts`], with layer pieces of layered elements besides the
/// `Pen::Cut` polygons in `items`. Each layer is filled with its own
/// material's hatch; the line between two layers of one element is a thin
/// one, and layers of the same material merge across joined walls.
pub fn clean_up_pieces(model: &Model, items: &mut Vec<PlanItem>, layers: Vec<CutPiece>) {
    let mut cuts: Vec<CutPiece> = Vec::new();
    items.retain(|it| match it {
        PlanItem::Polygon {
            id,
            pen: Pen::Cut,
            points,
        } => {
            cuts.push(CutPiece {
                id: *id,
                points: points.clone(),
                layer: None,
            });
            false
        }
        _ => true,
    });
    cuts.extend(layers);
    // where cuts overlap, the stronger cuts through the other (see
    // `Joining::cuts`): a wall runs through the end of the slab that sits
    // in it, and the slab's structure through the wall's plaster
    let joining = Joining::of(model);
    let mut polys: Vec<CutPiece> = Vec::new();
    for c in &cuts {
        let mut pieces = vec![c.points.clone()];
        for other in &cuts {
            if !joining.cuts(model, other, c) || !is_convex(&other.points) {
                continue;
            }
            let hole = &other.points;
            pieces = pieces
                .into_iter()
                .flat_map(|p| {
                    if boxes_overlap(&p, hole) {
                        subtract_convex(&p, hole)
                    } else {
                        vec![p]
                    }
                })
                .collect();
        }
        polys.extend(pieces.into_iter().map(|p| CutPiece {
            points: p,
            ..c.clone()
        }));
    }
    let keys: Vec<(Option<ElementId>, u8)> = polys.iter().map(|p| p.key(model)).collect();
    for p in &polys {
        items.push(PlanItem::Fill {
            id: p.id,
            points: p.points.clone(),
            hatch: p.hatch(model),
            rgb: p.colour(model),
        });
    }
    let edges = |pts: &Vec<DVec2>| -> Vec<(DVec2, DVec2)> {
        (0..pts.len())
            .map(|i| (pts[i], pts[(i + 1) % pts.len()]))
            .collect()
    };
    let eps = 1e-4;
    for (i, piece) in polys.iter().enumerate() {
        for (a, b) in edges(&piece.points) {
            let len = a.distance(b);
            if len < eps {
                continue;
            }
            let d = (b - a) / len;
            let on_line = |p: DVec2| (p - a).perp_dot(d).abs() < eps;
            // parts of this edge covered by another cut's edge: merged away
            // where it is the same material, a thin line between two layers
            // of one element (drawn by the first of the two)
            let mut covered: Vec<(f64, f64)> = Vec::new();
            let mut thin: Vec<(f64, f64, bool)> = Vec::new();
            for (j, other) in polys.iter().enumerate() {
                let same = keys[i] == keys[j];
                if i == j || (!same && other.id != piece.id) {
                    continue;
                }
                for (p, q) in edges(&other.points) {
                    if on_line(p) && on_line(q) {
                        let (tp, tq) = ((p - a).dot(d), (q - a).dot(d));
                        let (lo, hi) = (tp.min(tq).max(0.0), tp.max(tq).min(len));
                        if hi > lo + eps {
                            if same {
                                covered.push((lo, hi));
                            } else {
                                thin.push((lo, hi, i < j));
                            }
                        }
                    }
                }
                // where two of the same overlap (footings lapped at a
                // corner), the edges running inside the other are merged
                // away too, leaving one outline round both
                if same {
                    covered.extend(inside_spans(a, b, &other.points, eps));
                }
            }
            // split the edge where any of that changes, and draw each part
            let mut ts: Vec<f64> = vec![0.0, len];
            for (lo, hi) in &covered {
                ts.extend([*lo, *hi]);
            }
            for (lo, hi, _) in &thin {
                ts.extend([*lo, *hi]);
            }
            ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
            ts.dedup_by(|x, y| (*x - *y).abs() < eps);
            let mut run: Option<(f64, f64, Pen)> = None;
            let flush = |run: &mut Option<(f64, f64, Pen)>, items: &mut Vec<PlanItem>| {
                if let Some((t0, t1, pen)) = run.take() {
                    items.push(PlanItem::Line {
                        id: piece.id,
                        pen,
                        a: a + d * t0,
                        b: a + d * t1,
                    });
                }
            };
            for w in ts.windows(2) {
                let (t0, t1) = (w[0], w[1]);
                let m = (t0 + t1) / 2.0;
                let inside = |lo: f64, hi: f64| lo <= m && m <= hi;
                let pen = if covered.iter().any(|(lo, hi)| inside(*lo, *hi)) {
                    None
                } else {
                    let mine: Vec<bool> = thin
                        .iter()
                        .filter(|(lo, hi, _)| inside(*lo, *hi))
                        .map(|t| t.2)
                        .collect();
                    if mine.is_empty() {
                        Some(Pen::Cut)
                    } else if mine.iter().any(|f| *f) {
                        Some(Pen::Projection)
                    } else {
                        None
                    }
                };
                match (pen, &mut run) {
                    (Some(p), Some((_, end, rp))) if *rp == p && (*end - t0).abs() < eps => {
                        *end = t1
                    }
                    (Some(p), _) => {
                        flush(&mut run, items);
                        run = Some((t0, t1, p));
                    }
                    (None, _) => flush(&mut run, items),
                }
            }
            flush(&mut run, items);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanItem {
    Polygon {
        id: ElementId,
        pen: Pen,
        points: Vec<DVec2>,
    },
    /// Fill of cut material, with no outline (outlines are `Pen::Cut` lines).
    Fill {
        id: ElementId,
        points: Vec<DVec2>,
        hatch: Hatch,
        /// The colour of the material cut here (its layer's, for layered
        /// elements), for the colour drawing styles (MU32).
        rgb: Option<[f32; 3]>,
    },
    Line {
        id: ElementId,
        pen: Pen,
        a: DVec2,
        b: DVec2,
    },
    /// Counter-clockwise arc from `start` to `end` angle, radians.
    Arc {
        id: ElementId,
        pen: Pen,
        center: DVec2,
        radius: f64,
        start: f64,
        end: f64,
    },
    Label {
        id: ElementId,
        at: DVec2,
        text: String,
    },
    /// Text at a fixed paper size, `size` mm tall, in a drawing of `mm`
    /// units per paper millimetre (1 for paper itself, as on a schedule).
    Text {
        id: ElementId,
        at: DVec2,
        text: String,
        size: f64,
        align: crate::paper::Anchor,
        mm: f64,
        /// sRGB 0..1; none is the view's ink.
        rgb: Option<[f32; 3]>,
        /// Semibold.
        bold: bool,
    },
    /// A dimension from `a` to `b`, its line `offset` to the left. A
    /// horizontal or vertical one measures from `a` to `b` square across,
    /// so `b` is where the true second point `witness[1]` lands on that
    /// line, and its witness lines start from `witness`.
    Dimension {
        id: ElementId,
        a: DVec2,
        b: DVec2,
        offset: f64,
        text: String,
        look: crate::annotation::TextLook,
        witness: Option<[DVec2; 2]>,
        /// Stretches of the dimension line left out where tags and notes
        /// sit across it, as fractions of the way from `a` to `b`.
        breaks: Vec<[f64; 2]>,
    },
    /// A tag: `text` in a bubble at `at`, with a leader back to `anchor`.
    Tag {
        id: ElementId,
        anchor: DVec2,
        at: DVec2,
        /// A bend in the leader.
        elbow: Option<DVec2>,
        text: String,
        round: bool,
        look: crate::annotation::TextLook,
    },
    /// An angular, radial, diameter or arc-length dimension, or a note.
    Annotation {
        id: ElementId,
        item: Box<crate::annotation::Annotation>,
    },
    /// An element hidden inside another, washed over what hides it in its
    /// material colour (sRGB, 0..1), see-through: for detailing.
    Tint {
        id: ElementId,
        points: Vec<DVec2>,
        rgb: [f32; 3],
    },
    /// A room's floor: washed in `rgb` when the view colours its rooms,
    /// otherwise not drawn; picked by a click inside it that finds nothing
    /// else.
    Room {
        id: ElementId,
        points: Vec<DVec2>,
        rgb: Option<[f32; 3]>,
    },
    /// The footprint of something below the cut, filled flat in a light
    /// tint (sRGB, 0..1) under the linework, with no outline.
    Shade {
        id: ElementId,
        points: Vec<DVec2>,
        rgb: [f32; 3],
    },
    /// Shaded surfaces as an image spanning `lo`..`hi`, drawn only inside `clip`.
    Image {
        id: ElementId,
        raster: std::sync::Arc<crate::raster::Raster>,
        lo: DVec2,
        hi: DVec2,
        clip: Option<(DVec2, DVec2)>,
    },
}

/// Is a label short enough to sit in a bubble, as grid and section marks
/// do (level datums and longer text are plain)?
pub fn is_bubble(text: &str) -> bool {
    // a roof pitch (35°, 1:60) is short too, but plain
    !text.contains([' ', '°', ':']) && (1..=5).contains(&text.chars().count())
}

/// A bubble's radius for `text`, in units of the bubble for one or two
/// characters.
pub fn bubble_scale(text: &str) -> f64 {
    (text.chars().count() as f64 * 0.38).max(1.0)
}

impl PlanItem {
    pub fn id(&self) -> ElementId {
        match self {
            PlanItem::Polygon { id, .. }
            | PlanItem::Fill { id, .. }
            | PlanItem::Line { id, .. }
            | PlanItem::Arc { id, .. }
            | PlanItem::Label { id, .. }
            | PlanItem::Text { id, .. }
            | PlanItem::Dimension { id, .. }
            | PlanItem::Tag { id, .. }
            | PlanItem::Annotation { id, .. }
            | PlanItem::Tint { id, .. }
            | PlanItem::Room { id, .. }
            | PlanItem::Shade { id, .. }
            | PlanItem::Image { id, .. } => *id,
        }
    }

    /// Corners of what the item covers, in drawing units (none for guides,
    /// which run on without end).
    pub fn extent(&self) -> Vec<DVec2> {
        let mut pts = Vec::new();
        match self {
            PlanItem::Polygon { points, .. }
            | PlanItem::Fill { points, .. }
            | PlanItem::Tint { points, .. }
            | PlanItem::Room { points, .. }
            | PlanItem::Shade { points, .. } => pts.extend(points.iter().copied()),
            PlanItem::Line {
                pen: Pen::Guide, ..
            } => {}
            PlanItem::Line { a, b, .. } => pts.extend([*a, *b]),
            PlanItem::Arc { center, radius, .. } => pts.extend([
                *center - DVec2::splat(*radius),
                *center + DVec2::splat(*radius),
            ]),
            PlanItem::Label { at, text, .. } => {
                // rough text extent at 1:100 (3 mm type)
                let half = DVec2::new(text.chars().count() as f64 * 0.09 + 0.2, 0.25);
                pts.extend([*at - half, *at + half]);
            }
            PlanItem::Text {
                at,
                text,
                size,
                align,
                mm,
                ..
            } => {
                // the text's own box, at its size on paper
                let w = crate::paper::text_width(text, *size) * mm;
                let h = size * mm;
                let x0 = match align {
                    crate::paper::Anchor::Start => at.x,
                    crate::paper::Anchor::Middle => at.x - w / 2.0,
                    crate::paper::Anchor::End => at.x - w,
                };
                pts.extend([
                    DVec2::new(x0, at.y - h / 2.0),
                    DVec2::new(x0 + w, at.y + h / 2.0),
                ]);
            }
            PlanItem::Dimension { a, b, offset, .. } => {
                let n = (*b - *a).normalize_or_zero().perp() * *offset;
                pts.extend([*a, *b, *a + n, *b + n]);
            }
            PlanItem::Tag {
                anchor, at, elbow, ..
            } => {
                pts.extend([*anchor, *at - DVec2::splat(0.3), *at + DVec2::splat(0.3)]);
                pts.extend(*elbow);
            }
            PlanItem::Annotation { item, .. } => pts.extend(item.extent()),
            PlanItem::Image { lo, hi, clip, .. } => match clip {
                Some((clo, chi)) => pts.extend([lo.max(*clo), hi.min(*chi)]),
                None => pts.extend([*lo, *hi]),
            },
        }
        pts
    }

    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let pts = self.extent();
        let first = *pts.first()?;
        Some(
            pts.iter()
                .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanView {
    pub items: Vec<PlanItem>,
}

impl PlanView {
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let pts: Vec<DVec2> = self.items.iter().flat_map(|it| it.extent()).collect();
        let first = *pts.first()?;
        Some(
            pts.iter()
                .fold((first, first), |(lo, hi), p| (lo.min(*p), hi.max(*p))),
        )
    }

    /// Render as SVG at a drawing scale, e.g. `100.0` for 1:100. Units are millimetres on paper.
    pub fn to_svg(&self, scale: f64) -> String {
        let (lo, hi) = self.bounds().unwrap_or((DVec2::ZERO, DVec2::ONE));
        let margin = DVec2::splat(1.0);
        let (lo, hi) = (lo - margin, hi + margin);
        let map = crate::paper::PaperMap {
            model: lo,
            paper: DVec2::ZERO,
            scale,
        };
        let mut svg = crate::paper::SvgCanvas::new((hi - lo) * map.k());
        crate::paper::draw(&mut svg, self, &map);
        svg.finish()
    }
}

/// Generate the plan of one level, cut `cut_height` metres above it.
/// A sketch's lines and arcs in a drawing. `project` takes a point in the
/// sketch's model coordinates (relative to its level) to drawing x and y,
/// plus depth. Arcs on a plane facing the drawing stay arcs; seen at an
/// angle they are drawn in short pieces.
pub fn sketch_items(
    id: ElementId,
    sk: &creabim_core::Sketch,
    project: &dyn Fn(DVec3) -> DVec3,
) -> Vec<PlanItem> {
    let at = |p: DVec2| project(sk.plane.to_model(p)).truncate();
    let o = at(DVec2::ZERO);
    let (x, y) = (at(DVec2::X) - o, at(DVec2::Y) - o);
    let facing =
        (x.length() - 1.0).abs() < 1e-6 && (y.length() - 1.0).abs() < 1e-6 && x.dot(y).abs() < 1e-6;
    let mut out = Vec::new();
    // a closed sketch is a face: a light wash inside it (A7-1)
    let outline = sk.outline();
    if sk.closed && outline.len() >= 3 && !creabim_core::polygon::self_intersects(&outline, true) {
        out.push(PlanItem::Tint {
            id,
            points: outline.iter().map(|p| at(*p)).collect(),
            rgb: [0.16, 0.27, 0.55],
        });
    }
    for c in sk.curves() {
        match c {
            Curve::Arc {
                center,
                radius,
                start,
                sweep,
            } if facing => {
                // seen from behind, the arc turns the other way
                let flip = x.perp_dot(y) < 0.0;
                let a0 = if flip {
                    x.to_angle() - start
                } else {
                    x.to_angle() + start
                };
                let sweep = if flip { -sweep } else { sweep };
                let (start, end) = if sweep < 0.0 {
                    (a0 + sweep, a0)
                } else {
                    (a0, a0 + sweep)
                };
                out.push(PlanItem::Arc {
                    id,
                    pen: Pen::Symbol,
                    center: at(center),
                    radius,
                    start,
                    end,
                });
            }
            _ => {
                for w in c.points().windows(2) {
                    let (a, b) = (at(w[0]), at(w[1]));
                    if a.distance(b) > 1e-6 {
                        out.push(PlanItem::Line {
                            id,
                            pen: Pen::Symbol,
                            a,
                            b,
                        });
                    }
                }
            }
        }
    }
    out
}

/// Does the plan of `level`, cut `cut_height` above it, pass through
/// footing `f`? A foundation level under the ground floor shows the
/// footings its plane cuts, whatever level their walls stand on.
fn footing_cut_at(
    model: &Model,
    f: &creabim_core::Footing,
    level: ElementId,
    cut_height: f64,
) -> bool {
    let Some(l) = model.level(level) else {
        return false;
    };
    let z = l.elevation + cut_height;
    let top = f.top(model);
    top - f.thickness < z && z < top
}

pub fn plan_view(model: &Model, level: ElementId, cut_height: f64) -> PlanView {
    plan_view_with(
        model,
        level,
        cut_height,
        &creabim_core::PlanGraphics::default(),
    )
}

/// [`plan_view`] with a view's graphics: what lies wholly below the cut
/// tinted, and the stair from the level below seen through its well, as
/// far down as the view depth.
pub fn plan_view_with(
    model: &Model,
    level: ElementId,
    cut_height: f64,
    graphics: &creabim_core::PlanGraphics,
) -> PlanView {
    let mut items = Vec::new();
    // tinted footprints, drawn under everything else
    let mut shades: Vec<PlanItem> = Vec::new();
    // dashed over the cut fills: openings above the cut
    let mut overhead: Vec<PlanItem> = Vec::new();
    let joins = creabim_core::wall_joins(model);
    let cuts = crate::scene::all_cuts(model);
    let cutters: std::collections::HashSet<ElementId> = cuts.iter().map(|c| c.1).collect();
    let targets: std::collections::HashSet<ElementId> = cuts.iter().map(|c| c.0).collect();
    // elements whose plan comes from slicing their final 3D shape
    let mut sliced: Vec<ElementId> = Vec::new();
    // the layers of layered walls, cut
    let mut layers: Vec<CutPiece> = Vec::new();
    for e in model.iter() {
        if cutters.contains(&e.id) {
            continue;
        }
        let on_level = match &e.kind {
            ElementKind::Wall(w) => w.level == level,
            ElementKind::Slab(s) => s.level == level,
            // site geography has no level: it draws on every plan.
            ElementKind::SiteFeature(_) => true,
            // an imported mesh with no level of its own is on the level its
            // base stands on (MU33: a Blender import showed in no plan)
            ElementKind::FreeMesh(f) => match f.level {
                Some(l) => l == level,
                // no level of its own (an import): in every plan whose cut
                // or range below it reaches it (MU36: it was on only one)
                None => {
                    let z0 = model.level(level).map_or(0.0, |l| l.elevation);
                    let zs = f
                        .mesh
                        .vertices
                        .iter()
                        .map(|v| f.to_model(*v).z + f.origin.z);
                    let (lo, hi) = zs.fold((f64::MAX, f64::MIN), |(a, b), z| (a.min(z), b.max(z)));
                    // under the cut and not wholly below the floor's slab
                    lo < z0 + cut_height && hi > z0 - 0.5
                }
            },
            ElementKind::Footing(f) => f.plan_level(model) == level,
            k => creabim_core::members::level_of(k) == Some(level),
        };
        if on_level && (targets.contains(&e.id) || matches!(e.kind, ElementKind::FreeMesh(_))) {
            // Cuts don't apply to map layers: they have no body to cut,
            // and the plan draws their vectors directly.
            if !matches!(e.kind, ElementKind::SiteFeature(_)) {
                sliced.push(e.id);
            }
            if !matches!(e.kind, ElementKind::Wall(_)) {
                continue;
            }
        }
        match &e.kind {
            ElementKind::Grid(g) => {
                items.push(PlanItem::Line {
                    id: e.id,
                    pen: Pen::Grid,
                    a: g.start,
                    b: g.end,
                });
                let dir = (g.end - g.start).normalize_or_zero();
                items.push(PlanItem::Label {
                    id: e.id,
                    at: g.end + dir * 0.6,
                    text: creabim_core::grids::display_label(model, g),
                });
            }
            ElementKind::Wall(w) if w.level == level => {
                let d = w.direction();
                let n = DVec2::new(-d.y, d.x);
                let t = w.thickness / 2.0;
                let ends = joins.get(&e.id).cloned().unwrap_or_default();
                // wall-local point; the wall's own ends and the sides of the
                // gaps crossing walls leave follow its joins
                // a curved wall is laid round its arc, its ends square
                let at = |u: f64, s: f64| {
                    if w.is_arc() {
                        // its ends as its joins put them (see Wall::place_end)
                        if u <= 1e-9 || u >= w.length() - 1e-9 {
                            w.place_end(u > 1e-9, ends.place(w, u, s, false), s)
                        } else {
                            w.place(u, s)
                        }
                    } else {
                        w.start + d * ends.place(w, u, s, false) + n * s
                    }
                };
                let cut_v = cut_height - w.base_offset;
                let top_v = wall_top_height(model, w);
                if cut_v <= 0.0 || cut_v >= top_v {
                    // wall entirely below or above the cut: show its outline in projection
                    if cut_v >= top_v {
                        items.push(PlanItem::Polygon {
                            id: e.id,
                            pen: Pen::Projection,
                            points: if w.is_arc() {
                                wall_band(w, &at, 0.0, w.length(), -t, t)
                            } else {
                                ends.footprint(w).to_vec()
                            },
                        });
                    }
                    continue;
                }
                if let Some(g) = &w.curtain {
                    // mullions and panels, and the doors in them
                    let mut hosted: Vec<(ElementId, &creabim_core::Opening)> = vec![];
                    for oid in model.dependents(e.id) {
                        if let Some(ElementKind::Opening(o)) = model.get(oid).map(|x| &x.kind) {
                            hosted.push((oid, o));
                        }
                    }
                    let openings: Vec<&creabim_core::Opening> =
                        hosted.iter().map(|h| h.1).collect();
                    items.extend(crate::curtain::curtain_plan(e.id, w, g, &openings, cut_v));
                    let d = w.direction();
                    let n = DVec2::new(-d.y, d.x);
                    let plain = |u: f64, s: f64| w.start + d * u + n * s;
                    for (oid, o) in hosted {
                        if let Some(c) = Cut::of(o, w) {
                            if cut_v > c.v0 && cut_v < c.v1 {
                                items.extend(opening_symbol(oid, o, w, &c, cut_v, &plain));
                            }
                        }
                    }
                    continue;
                }
                let mut gaps: Vec<(f64, f64)> = Vec::new();
                for oid in model.dependents(e.id) {
                    let Some(oe) = model.get(oid) else { continue };
                    let ElementKind::Opening(o) = &oe.kind else {
                        continue;
                    };
                    let Some(c) = Cut::of(o, w) else { continue };
                    if cut_v > c.v0 && cut_v < c.v1 {
                        gaps.push((c.u0, c.u1));
                        items.extend(opening_symbol(oid, o, w, &c, cut_v, &at));
                    } else if c.v0 >= cut_v {
                        // wholly above the cut (a duct hole, a high
                        // window): its outline in the wall, dashed
                        let corners = [at(c.u0, -t), at(c.u1, -t), at(c.u1, t), at(c.u0, t)];
                        for i in 0..4 {
                            overhead.push(PlanItem::Line {
                                id: oid,
                                pen: Pen::Hidden,
                                a: corners[i],
                                b: corners[(i + 1) % 4],
                            });
                        }
                    }
                }
                // a curtain wall set in it leaves its own gap, and draws
                // itself; a screen wall's holes leave theirs
                let screen = creabim_core::screen::screen_openings(model, e.id);
                let voids = creabim_core::curtain::voids_in(model, e.id);
                for o in voids.into_iter().map(|v| v.1).chain(screen) {
                    if let Some(c) = Cut::of(&o, w) {
                        if cut_v > c.v0 && cut_v < c.v1 {
                            gaps.push((c.u0, c.u1));
                        }
                    }
                }
                gaps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                // the stretches between crossing walls, less the openings
                let mut pieces = Vec::new();
                for (lo, hi) in ends.solid_spans(w) {
                    let mut u = lo;
                    for &(g0, g1) in &gaps {
                        if g1 <= u || g0 >= hi {
                            continue;
                        }
                        if g0 > u + 1e-9 {
                            pieces.push((u, g0));
                        }
                        u = u.max(g1);
                    }
                    if hi > u + 1e-9 {
                        pieces.push((u, hi));
                    }
                }
                if let Some((_, asm)) = creabim_core::assembly_of(model, e) {
                    // each layer between its own ends: a core runs on at a T
                    for band in creabim_core::assembly::wall_bands(asm, w) {
                        let at = |u: f64, s: f64| {
                            if w.is_arc() {
                                w.place(u, s)
                            } else {
                                w.start + d * ends.place_layer(w, u, s, band.core, band.ext) + n * s
                            }
                        };
                        for &(a, b) in &pieces {
                            layers.push(CutPiece {
                                id: e.id,
                                points: wall_band(w, &at, a, b, band.s0, band.s1),
                                layer: Some(crate::scene::LayerPart::of(&band)),
                            });
                        }
                    }
                    continue;
                }
                for (a, b) in pieces {
                    items.push(PlanItem::Polygon {
                        id: e.id,
                        pen: Pen::Cut,
                        points: wall_band(w, &at, a, b, -t, t),
                    });
                }
            }
            ElementKind::Slab(s) if s.level == level => {
                // cut when the plan plane passes through it, else seen below
                let bottom = s.top_offset - s.thickness;
                let cut = bottom < cut_height && cut_height < s.top_offset;
                let holes = creabim_core::slab::holes_of(model, e);
                // layers whose edges were moved by hand (A7-10): counted
                // from the top, each with its depth below the slab's top
                let layers: Vec<f64> = creabim_core::assembly::assembly_of(model, e)
                    .map(|(_, a)| a.layers.iter().map(|l| l.thickness).collect())
                    .unwrap_or_default();
                let edited: std::collections::BTreeSet<usize> = s
                    .layer_edges
                    .iter()
                    .map(|l| l.layer)
                    .filter(|l| *l < layers.len())
                    .collect();
                if cut {
                    // the layer the plan plane passes through
                    let mut top = s.top_offset;
                    let mut at = None;
                    for (k, t) in layers.iter().enumerate() {
                        if cut_height <= top && cut_height > top - t {
                            at = Some(k);
                        }
                        top -= t;
                    }
                    let outline = match at {
                        Some(k) if edited.contains(&k) => s.layer_outline(k),
                        _ => s.outline(),
                    };
                    // one outline with its holes bridged in, so the fill leaves them open
                    items.insert(
                        0,
                        PlanItem::Polygon {
                            id: e.id,
                            pen: Pen::Cut,
                            points: creabim_core::polygon::bridged(&outline, &holes),
                        },
                    );
                } else {
                    // seen from above: the slab's own edge where any layer
                    // still follows it, and each edited layer's edge
                    if edited.len() < layers.len().max(1) {
                        items.insert(
                            0,
                            PlanItem::Polygon {
                                id: e.id,
                                pen: Pen::Projection,
                                points: s.outline(),
                            },
                        );
                    }
                    for k in edited.iter().rev() {
                        items.insert(
                            0,
                            PlanItem::Polygon {
                                id: e.id,
                                pen: Pen::Projection,
                                points: s.layer_outline(*k),
                            },
                        );
                    }
                    let here = model.level(level).map_or(0.0, |l| l.elevation);
                    for h in &holes {
                        if stair_arrives_in(model, here, h) {
                            items.push(PlanItem::Polygon {
                                id: e.id,
                                pen: Pen::Projection,
                                points: h.clone(),
                            });
                        } else {
                            items.extend(hole_marks(e.id, h, Pen::Projection));
                        }
                    }
                }
            }
            ElementKind::Object(o) if o.level == level => {
                items.extend(crate::objects::object_plan(model, e, cut_height));
                shades.extend(crate::objects::object_shade(model, e, cut_height, graphics));
            }
            k if on_level && creabim_core::members::is_member(k) => {
                items.extend(crate::members::member_plan(model, e, cut_height));
                // a mass or sweep below the cut: its top seen tinted, in
                // its material under the colour styles (MU36: they read as
                // wireframe)
                // and a beam, in its colour as other members' (MU40: a beam
                // on the roof read as an empty outline)
                // every member: sweeps and extrusions, beams, roofs,
                // stairs, railings… (MU40: roofs and sweeps read empty)
                let solid: Option<(crate::TriMesh, [f32; 3])> = match k {
                    ElementKind::Part(p) if !p.void => {
                        crate::parts::part_mesh(model, p).map(|m| (m, [0.78, 0.76, 0.72]))
                    }
                    ElementKind::Part(_) => None,
                    _ => crate::members::member_mesh(model, e),
                };
                if let Some((mesh, base)) = solid {
                    let color = creabim_core::material_of(model, e).map_or(base, |(_, m)| m.color);
                    let item = crate::scene::SceneItem {
                        id: e.id,
                        mesh,
                        color,
                        surface: crate::scene::PLAIN_SURFACE,
                        ghost: false,
                        layer: None,
                        finish: None,
                    };
                    let z = model.level(level).map_or(0.0, |l| l.elevation) + cut_height;
                    shades.extend(mesh_shade(&item, z, graphics));
                }
            }
            // a column standing on a level below, running up through this one
            ElementKind::Column(c) if c.crosses(model, level, cut_height) => {
                items.push(PlanItem::Polygon {
                    id: e.id,
                    pen: Pen::Cut,
                    points: c.outline(),
                });
            }
            // one from a level below that rises into this level but stops
            // under the cut: seen from above (MU40: porch posts reaching
            // the roof were missing from the roof plan)
            ElementKind::Column(c)
                if !on_level && {
                    let (_, z1) = c.span(model);
                    let floor = model.level(level).map_or(0.0, |l| l.elevation);
                    z1 > floor + 0.05 && z1 <= floor + cut_height
                } =>
            {
                let pts = c.outline();
                let mut ring = pts.clone();
                if let Some(f) = pts.first() {
                    ring.push(*f);
                }
                for w in ring.windows(2) {
                    items.push(PlanItem::Line {
                        id: e.id,
                        pen: Pen::Projection,
                        a: w[0],
                        b: w[1],
                    });
                }
            }
            // a footing under a wall or column of a level above, which this
            // plan's plane cuts through: the plan of a foundation level
            ElementKind::Footing(f) if footing_cut_at(model, f, level, cut_height) => {
                for piece in creabim_core::footing::outlines(model, e, None) {
                    items.push(PlanItem::Polygon {
                        id: e.id,
                        pen: Pen::Cut,
                        points: piece,
                    });
                }
            }
            ElementKind::Shaft(sh) => {
                // on every plan it passes through, crossed, to pick it by:
                // the level its top reaches too, whose floor it cuts
                let z = model.level(level).map_or(0.0, |l| l.elevation);
                let (z0, z1) = sh.span(model);
                if (z - z0).abs() < 1e-6 && stair_in(model, level, &sh.boundary) {
                    // a stair well where its stair starts: no floor is
                    // open here, the edge of the floor above is dashed
                    let n = sh.boundary.len();
                    for i in 0..n {
                        items.push(PlanItem::Line {
                            id: e.id,
                            pen: Pen::Hidden,
                            a: sh.boundary[i],
                            b: sh.boundary[(i + 1) % n],
                        });
                    }
                } else if z0 - 1e-6 <= z && z <= z1 + 1e-6 {
                    if z > z0 + 1e-6 && stair_arrives_in(model, z, &sh.boundary) {
                        // the stair drawn through it says what it is: its
                        // outline, with no cross over the stair
                        items.push(PlanItem::Polygon {
                            id: e.id,
                            pen: Pen::Symbol,
                            points: sh.boundary.clone(),
                        });
                    } else {
                        items.extend(hole_marks(e.id, &sh.boundary, Pen::Symbol));
                    }
                }
            }
            ElementKind::Sketch(sk) if sk.level == Some(level) => {
                items.extend(sketch_items(e.id, sk, &|p| p));
            }
            // loaded map layers draw on every plan: filled polygons in
            // the category's hatch, outlines over them, polylines as
            // light projection lines. V1 has no cut/range test — site
            // geometry is flat at project zero. The layer's own style
            // colours the fill and hides it project-wide.
            ElementKind::SiteFeature(s) if s.style.visible => {
                for f in &s.features {
                    match &f.geom {
                        creabim_core::SiteGeometry::Polygon { exterior, holes } => {
                            items.push(PlanItem::Fill {
                                id: e.id,
                                points: exterior.clone(),
                                hatch: s.category.hatch(),
                                rgb: s.style.fill.map(|c| c.0),
                            });
                            items.push(PlanItem::Polygon {
                                id: e.id,
                                pen: Pen::Projection,
                                points: exterior.clone(),
                            });
                            for h in holes {
                                items.push(PlanItem::Polygon {
                                    id: e.id,
                                    pen: Pen::Projection,
                                    points: h.clone(),
                                });
                            }
                        }
                        creabim_core::SiteGeometry::Polyline { points } => {
                            for w in points.windows(2) {
                                items.push(PlanItem::Line {
                                    id: e.id,
                                    pen: Pen::Projection,
                                    a: w[0],
                                    b: w[1],
                                });
                            }
                        }
                    }
                }
            }
            // guides lying flat; one standing on a wall face shows in sections
            ElementKind::Guide(g)
                if g.level.is_none_or(|l| l == level) && (g.a.z - g.b.z).abs() < 1e-3 =>
            {
                if let Some((a, b)) = guide_span(g.a.truncate(), g.b.truncate()) {
                    items.push(PlanItem::Line {
                        id: e.id,
                        pen: Pen::Guide,
                        a,
                        b,
                    });
                }
            }
            // a callout's detail view is marked by its box, not its line
            ElementKind::View(v) if v.callout.is_none() => {
                if let creabim_core::ViewKind::Section {
                    start, end, cut, ..
                } = &v.kind
                {
                    section_marker(&mut items, e.id, &e.name, *start, *end, *cut);
                }
            }
            _ => {}
        }
    }
    // a cut wall keeps its opening symbols but takes its outline from the slice
    items.retain(|it| !sliced.contains(&it.id()) || matches!(it, PlanItem::Label { .. }));
    layers.retain(|p| !sliced.contains(&p.id));
    clean_up_pieces(model, &mut items, layers);
    items.append(&mut overhead);
    if !sliced.is_empty() {
        let z = model.level(level).map_or(0.0, |l| l.elevation) + cut_height;
        // only what is sliced, and what cuts it
        let mut needed: std::collections::HashSet<ElementId> = sliced.iter().copied().collect();
        needed.extend(cuts.iter().filter(|c| sliced.contains(&c.0)).map(|c| c.1));
        let scene = crate::scene::build_scene_of(model, Some(&needed));
        for item in scene.iter().filter(|i| sliced.contains(&i.id)) {
            sliced_plan(model, item, z, &mut items);
            // an imported mesh wholly below the cut: its footprint tinted
            if matches!(
                model.get(item.id).map(|e| &e.kind),
                Some(ElementKind::FreeMesh(_))
            ) {
                shades.extend(mesh_shade(item, z, graphics));
            }
        }
    }
    items.extend(stairs_beyond(model, level, graphics));
    category_pens(model, &mut items, graphics);
    shades.append(&mut items);
    PlanView { items: shades }
}

/// The lines a category draws in projection given the pen its view's
/// table names for it: furniture and fixtures in the fine pen.
pub(crate) fn category_pens(
    model: &Model,
    items: &mut [PlanItem],
    graphics: &creabim_core::PlanGraphics,
) {
    if graphics.category_pens.is_empty() {
        return;
    }
    let cutters = std::collections::HashSet::new();
    let mut of: std::collections::HashMap<ElementId, Option<Pen>> = Default::default();
    for it in items.iter_mut() {
        let (PlanItem::Line { id, pen, .. }
        | PlanItem::Polygon { id, pen, .. }
        | PlanItem::Arc { id, pen, .. }) = it
        else {
            continue;
        };
        if !matches!(pen, Pen::Projection | Pen::Symbol | Pen::Fine) {
            continue;
        }
        let to = *of.entry(*id).or_insert_with(|| {
            let e = model.get(*id)?;
            let c = creabim_core::visibility::Category::of(e, &cutters)?;
            graphics.pen_of(c).map(|p| match p {
                creabim_core::graphics::PenClass::Projection => Pen::Projection,
                creabim_core::graphics::PenClass::Symbol => Pen::Symbol,
                creabim_core::graphics::PenClass::Fine => Pen::Fine,
            })
        });
        if let Some(to) = to {
            *pen = to;
        }
    }
}

/// Does a stair standing on `level` start inside `outline`?
fn stair_in(model: &Model, level: ElementId, outline: &[DVec2]) -> bool {
    model.iter().any(|e| match &e.kind {
        ElementKind::Stair(s) => s.level == level && point_in(s.start, outline),
        _ => false,
    })
}

/// Does a stair from below arrive at the floor at height `z` through
/// `outline`: does its last flight end inside it?
fn stair_arrives_in(model: &Model, z: f64, outline: &[DVec2]) -> bool {
    model.iter().any(|e| {
        let ElementKind::Stair(s) = &e.kind else {
            return false;
        };
        let (base, rise) = (s.base(model), s.rise(model));
        base < z - 1e-6
            && (base + rise - z).abs() <= 0.05
            && s.layout(rise)
                .flights
                .last()
                .is_some_and(|f| point_in(f.end(s.tread) - f.dir * (s.tread / 2.0), outline))
    })
}

/// The footprint of a mesh lying wholly below `z`, as the faces it shows
/// from above, tinted. Very large meshes are tinted as their hull.
fn mesh_shade(
    item: &crate::scene::SceneItem,
    z: f64,
    graphics: &creabim_core::PlanGraphics,
) -> Vec<PlanItem> {
    let Some(rgb) = graphics.below_tint(item.color) else {
        return vec![];
    };
    let m = &item.mesh;
    let p = |i: u32| glam::Vec3::from(m.positions[i as usize]).as_dvec3();
    if m.positions.is_empty() {
        return vec![];
    }
    // what lies below the cut, seen from above: a solid the cut passes
    // through (a roof, a sloped beam) is tinted where it is below it
    // (MU40: they were left empty)
    let tris = m.indices.chunks(3).filter(|t| t.len() == 3);
    if m.indices.len() / 3 > 4000 {
        let pts: Vec<DVec2> = m
            .positions
            .iter()
            .filter(|q| (q[2] as f64) < z - 1e-6)
            .map(|q| DVec2::new(q[0] as f64, q[1] as f64))
            .collect();
        if pts.len() < 3 {
            return vec![];
        }
        let hull = creabim_core::objects::hull(&pts);
        return vec![PlanItem::Shade {
            id: item.id,
            points: hull,
            rgb,
        }];
    }
    tris.filter_map(|t| {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        // faces turned up, seen from above
        if (b - a).truncate().perp_dot((c - a).truncate()) <= 1e-9 {
            return None;
        }
        // the part of it below the cut (MU40: a face the cut crosses, as a
        // sloped beam's or a roof's top, was left empty)
        let mut poly: Vec<DVec2> = vec![];
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let (iu, iv) = (u.z < z - 1e-6, v.z < z - 1e-6);
            if iu {
                poly.push(u.truncate());
            }
            if iu != iv {
                let k = (z - u.z) / (v.z - u.z);
                poly.push((u + (v - u) * k).truncate());
            }
        }
        (poly.len() >= 3).then_some(PlanItem::Shade {
            id: item.id,
            points: poly,
            rgb,
        })
    })
    .collect()
}

/// The band of wall `w` from `a` to `b` along it and `s0` to `s1` across,
/// as an outline: four corners on a straight wall, bending round a
/// curved one (MU40).
fn wall_band(
    w: &creabim_core::Wall,
    at: &dyn Fn(f64, f64) -> DVec2,
    a: f64,
    b: f64,
    s0: f64,
    s1: f64,
) -> Vec<DVec2> {
    let mut us = vec![a];
    us.extend(
        w.bends()
            .into_iter()
            .filter(|u| *u > a + 1e-9 && *u < b - 1e-9),
    );
    us.push(b);
    let mut pts: Vec<DVec2> = us.iter().map(|u| at(*u, s0)).collect();
    pts.extend(us.iter().rev().map(|u| at(*u, s1)));
    pts
}

/// The height of a wall's top above its base where it is highest. A top
/// attached to a roof or slab follows their undersides where they are
/// over it, and keeps the wall's own height elsewhere.
fn wall_top_height(model: &Model, w: &creabim_core::Wall) -> f64 {
    let Some(top) = creabim_core::walls::top_profile(model, w) else {
        return w.height;
    };
    let base = creabim_core::walls::base_z(model, w);
    let mut ts = vec![0.0, 1.0];
    ts.extend(top.breaks(w.start, w.end));
    ts.iter()
        .flat_map(|t| [t - 1e-4, t + 1e-4])
        .filter(|t| (0.0..=1.0).contains(t))
        .map(|t| {
            top.z_at(w.start.lerp(w.end, t))
                .map_or(w.height, |z| z - base)
        })
        .fold(0.0, f64::max)
}

/// Stairs climbing from a level below up to `level`, seen through the
/// holes in its floor (a stair well, a void): their treads, sides and
/// landing in projection, clipped to the holes, with the walking line, an
/// arrow and DN where it reaches this floor. What lies deeper than the
/// view depth is left out.
fn stairs_beyond(
    model: &Model,
    level: ElementId,
    graphics: &creabim_core::PlanGraphics,
) -> Vec<PlanItem> {
    let Some(here) = model.level(level).map(|l| l.elevation) else {
        return vec![];
    };
    let below = model
        .levels()
        .map(|(_, l)| l.elevation)
        .filter(|z| *z < here - 1e-6)
        .reduce(f64::max);
    let bottom = here - graphics.depth(here, below);
    // the floor's holes: shafts reaching up to it and holes in its slabs
    let mut holes: Vec<Vec<DVec2>> = Vec::new();
    for e in model.iter() {
        match &e.kind {
            ElementKind::Shaft(sh) => {
                let (z0, z1) = sh.span(model);
                if z0 < here - 1e-6 && here <= z1 + 1e-6 {
                    holes.push(sh.boundary.clone());
                }
            }
            ElementKind::Slab(s) if s.level == level => {
                holes.extend(creabim_core::slab::holes_of(model, e));
            }
            _ => {}
        }
    }
    if holes.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();
    for e in model.iter() {
        let ElementKind::Stair(s) = &e.kind else {
            continue;
        };
        let base = s.base(model);
        let rise = s.rise(model);
        // it arrives at this floor from below
        if base >= here - 1e-6 || (base + rise - here).abs() > 0.05 {
            continue;
        }
        let mut lines: Vec<(Pen, DVec2, DVec2)> = Vec::new();
        crate::members::stair_lines(s, rise, &mut |pen, a, b, z| {
            if base + z >= bottom - 1e-6 {
                lines.push((pen, a, b));
            }
        });
        for (pen, a, b) in lines {
            for (p, q) in clip_to_holes(a, b, &holes) {
                out.push(PlanItem::Line {
                    id: e.id,
                    pen,
                    a: p,
                    b: q,
                });
            }
        }
        let lay = s.layout(rise);
        if let Some(f) = lay.flights.last() {
            let top = f.end(s.tread);
            if holes.iter().any(|h| point_in(top, h)) {
                out.push(PlanItem::Label {
                    id: e.id,
                    at: top + f.dir * 0.35,
                    text: "DN".into(),
                });
            }
        }
    }
    out
}

/// The parts of `a` → `b` inside any of `holes`.
fn clip_to_holes(a: DVec2, b: DVec2, holes: &[Vec<DVec2>]) -> Vec<(DVec2, DVec2)> {
    let len = a.distance(b);
    if len < 1e-9 {
        return vec![];
    }
    let d = (b - a) / len;
    let mut out: Vec<(f64, f64)> = Vec::new();
    for h in holes {
        let mut ts = vec![0.0, len];
        let n = h.len();
        for i in 0..n {
            let (p, q) = (h[i], h[(i + 1) % n]);
            let e = q - p;
            let den = d.perp_dot(e);
            if den.abs() < 1e-12 {
                continue;
            }
            let t = (p - a).perp_dot(e) / den;
            let s = (p - a).perp_dot(d) / den;
            if (0.0..=1.0).contains(&s) && t > 0.0 && t < len {
                ts.push(t);
            }
        }
        ts.sort_by(|x, y| x.total_cmp(y));
        for w in ts.windows(2) {
            // on or inside the hole's edge
            let m = a + d * ((w[0] + w[1]) / 2.0);
            if w[1] > w[0] + 1e-9 && (point_in(m, h) || edge_distance(m, h) < 1e-6) {
                out.push((w[0], w[1]));
            }
        }
    }
    out.sort_by(|x, y| x.0.total_cmp(&y.0));
    // join what touches, so overlapping holes give one line
    let mut joined: Vec<(f64, f64)> = Vec::new();
    for (lo, hi) in out {
        match joined.last_mut() {
            Some(last) if lo <= last.1 + 1e-9 => last.1 = last.1.max(hi),
            _ => joined.push((lo, hi)),
        }
    }
    joined
        .into_iter()
        .map(|(lo, hi)| (a + d * lo, a + d * hi))
        .collect()
}

/// The plan of `level` as an underlay: its linework (cut outlines,
/// projection, symbols) as halftone lines and arcs, without fills, text,
/// grids, guides or view marks.
pub fn underlay_items(model: &Model, level: ElementId, cut_height: f64) -> Vec<PlanItem> {
    let skip = |id: ElementId| {
        matches!(
            model.get(id).map(|e| &e.kind),
            Some(ElementKind::View(_) | ElementKind::Grid(_) | ElementKind::GridGroup(_))
        )
    };
    let mut out = Vec::new();
    for it in plan_view(model, level, cut_height).items {
        match it {
            PlanItem::Line { id, pen, a, b } if pen != Pen::Guide && pen != Pen::Grid => {
                if !skip(id) {
                    out.push(PlanItem::Line {
                        id,
                        pen: Pen::Underlay,
                        a,
                        b,
                    });
                }
            }
            PlanItem::Polygon { id, points, .. } if !skip(id) => {
                let n = points.len();
                for i in 0..n {
                    out.push(PlanItem::Line {
                        id,
                        pen: Pen::Underlay,
                        a: points[i],
                        b: points[(i + 1) % n],
                    });
                }
            }
            PlanItem::Arc {
                id,
                center,
                radius,
                start,
                end,
                ..
            } if !skip(id) => out.push(PlanItem::Arc {
                id,
                pen: Pen::Underlay,
                center,
                radius,
                start,
                end,
            }),
            _ => {}
        }
    }
    out
}

/// A hole through a slab, roof or shaft as plans draw it: its outline
/// and a cross corner to corner.
pub fn hole_marks(id: ElementId, hole: &[DVec2], pen: Pen) -> Vec<PlanItem> {
    let mut out = vec![PlanItem::Polygon {
        id,
        pen,
        points: hole.to_vec(),
    }];
    for (a, b) in hole_cross(hole) {
        out.push(PlanItem::Line {
            id,
            pen: Pen::Fine,
            a,
            b,
        });
    }
    out
}

/// The cross drawn in a hole: corner to corner of each convex piece of
/// it, so an L-shaped stair well has a cross in each leg and no line
/// leaves the opening.
pub fn hole_cross(hole: &[DVec2]) -> Vec<(DVec2, DVec2)> {
    let mut out = Vec::new();
    for piece in convex_pieces(hole) {
        if piece.len() < 4 {
            continue;
        }
        // the corners nearest the corners of its box: in a convex piece
        // the lines between them stay inside it
        let lo = piece.iter().copied().reduce(DVec2::min).unwrap_or_default();
        let hi = piece.iter().copied().reduce(DVec2::max).unwrap_or_default();
        let near = |p: DVec2| {
            piece
                .iter()
                .copied()
                .min_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
                .unwrap_or(p)
        };
        for (a, b) in [(lo, hi), (DVec2::new(lo.x, hi.y), DVec2::new(hi.x, lo.y))] {
            let (a, b) = (near(a), near(b));
            if a.distance(b) > 1e-6 {
                out.push((a, b));
            }
        }
    }
    out
}

/// A polygon split into convex pieces: itself when convex, rectangles
/// when its sides all run along x and y, otherwise its triangles merged
/// while they stay convex (Hertel and Mehlhorn).
pub fn convex_pieces(poly: &[DVec2]) -> Vec<Vec<DVec2>> {
    let mut poly = poly.to_vec();
    poly.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if poly.len() > 1 && poly[0].distance(poly[poly.len() - 1]) < 1e-9 {
        poly.pop();
    }
    if poly.len() < 3 || is_convex(&poly) {
        return vec![poly];
    }
    let n = poly.len();
    let square = (0..n).all(|i| {
        let e = poly[(i + 1) % n] - poly[i];
        e.x.abs() < 1e-9 || e.y.abs() < 1e-9
    });
    if square {
        return rectangles(&poly);
    }
    let mut pieces: Vec<Vec<DVec2>> = creabim_core::polygon::triangulate(&poly)
        .into_iter()
        .map(|t| t.iter().map(|i| poly[*i]).collect())
        .collect();
    // merge neighbours across a shared side while the result is convex
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..pieces.len() {
            for j in i + 1..pieces.len() {
                if let Some(m) = join_pieces(&pieces[i], &pieces[j]) {
                    if is_convex(&m) {
                        pieces[i] = m;
                        pieces.remove(j);
                        merged = true;
                        break 'outer;
                    }
                }
            }
        }
    }
    pieces
}

/// Two polygons sharing a side, as one (none when they share none).
fn join_pieces(a: &[DVec2], b: &[DVec2]) -> Option<Vec<DVec2>> {
    let close = |p: DVec2, q: DVec2| p.distance(q) < 1e-9;
    let (na, nb) = (a.len(), b.len());
    for i in 0..na {
        let (p, q) = (a[i], a[(i + 1) % na]);
        for j in 0..nb {
            if close(b[j], q) && close(b[(j + 1) % nb], p) {
                // a from q round to p, then b from p round to q
                let mut out: Vec<DVec2> = (0..na).map(|k| a[(i + 1 + k) % na]).collect();
                out.extend((1..nb - 1).map(|k| b[(j + 1 + k) % nb]));
                // drop corners left in a straight line
                let m = out.len();
                let keep: Vec<DVec2> = (0..m)
                    .filter(|k| {
                        let (u, v, w) = (out[(k + m - 1) % m], out[*k], out[(k + 1) % m]);
                        (v - u).perp_dot(w - v).abs() > 1e-12
                    })
                    .map(|k| out[k])
                    .collect();
                return Some(keep);
            }
        }
    }
    None
}

/// A polygon with sides along x and y as rectangles: the cells between
/// its corner lines, merged into bands across and then down.
fn rectangles(poly: &[DVec2]) -> Vec<Vec<DVec2>> {
    let sorted = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.total_cmp(b));
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        v
    };
    let xs = sorted(poly.iter().map(|p| p.x).collect());
    let ys = sorted(poly.iter().map(|p| p.y).collect());
    // each row of cells as runs [x0, x1) inside the polygon
    let mut rows: Vec<Vec<(usize, usize)>> = Vec::new();
    for j in 0..ys.len().saturating_sub(1) {
        let y = (ys[j] + ys[j + 1]) / 2.0;
        let mut runs = Vec::new();
        let mut start: Option<usize> = None;
        for i in 0..xs.len() - 1 {
            let inside = point_in(DVec2::new((xs[i] + xs[i + 1]) / 2.0, y), poly);
            match (inside, start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    runs.push((s, i));
                    start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = start {
            runs.push((s, xs.len() - 1));
        }
        rows.push(runs);
    }
    // runs repeated row on row make one rectangle
    let mut out = Vec::new();
    let mut open: Vec<((usize, usize), usize)> = Vec::new();
    for (j, runs) in rows.iter().enumerate() {
        let mut next = Vec::new();
        for (run, from) in open.drain(..) {
            if runs.contains(&run) {
                next.push((run, from));
            } else {
                out.push((run, from, j));
            }
        }
        for run in runs {
            if !next.iter().any(|(r, _)| r == run) {
                next.push((*run, j));
            }
        }
        open = next;
    }
    out.extend(open.into_iter().map(|(run, from)| (run, from, rows.len())));
    out.into_iter()
        .map(|((i0, i1), j0, j1)| {
            let (x0, x1, y0, y1) = (xs[i0], xs[i1], ys[j0], ys[j1]);
            vec![
                DVec2::new(x0, y0),
                DVec2::new(x1, y0),
                DVec2::new(x1, y1),
                DVec2::new(x0, y1),
            ]
        })
        .collect()
}

/// Is this the linework of an underlay (drawn faintly, never picked)?
/// How far inside another element's cut a line must run to count as
/// hidden by it, so lines along its edge (a slab under a wall) stay as
/// they are.
const COVER_MARGIN: f64 = 0.005;

/// Lines of elements lying under another element's cut fill, drawn again
/// dashed on top: what a view showing hidden elements adds, e.g. a beam
/// inside a wall or a stud wall in a cavity. Doors and windows, which sit
/// in their walls' holes, are left as they are.
pub fn reveal_covered(model: &Model, items: &mut Vec<PlanItem>) {
    let fills: Vec<(ElementId, &[DVec2], DVec2, DVec2)> = items
        .iter()
        .filter_map(|it| match it {
            PlanItem::Fill { id, points, .. } if points.len() >= 3 => {
                let lo = points.iter().copied().reduce(DVec2::min)?;
                let hi = points.iter().copied().reduce(DVec2::max)?;
                Some((*id, points.as_slice(), lo, hi))
            }
            _ => None,
        })
        .collect();
    if fills.is_empty() {
        return;
    }
    let mut segments: Vec<(ElementId, DVec2, DVec2)> = Vec::new();
    for it in items.iter() {
        match it {
            PlanItem::Line {
                id,
                pen: Pen::Projection | Pen::Hidden,
                a,
                b,
            } => segments.push((*id, *a, *b)),
            PlanItem::Polygon {
                id,
                pen: Pen::Projection,
                points,
            } => {
                let n = points.len();
                segments.extend((0..n).map(|i| (*id, points[i], points[(i + 1) % n])));
            }
            _ => {}
        }
    }
    let opening = |id: ElementId| {
        matches!(
            model.get(id).map(|e| &e.kind),
            Some(ElementKind::Opening(_))
        )
    };
    let covered = |own: ElementId, p: DVec2| {
        fills.iter().any(|(id, poly, lo, hi)| {
            *id != own
                && p.cmpge(*lo).all()
                && p.cmple(*hi).all()
                && point_in(p, poly)
                && edge_distance(p, poly) > COVER_MARGIN
        })
    };
    let mut out = Vec::new();
    for (id, a, b) in segments {
        if opening(id) {
            continue;
        }
        let steps = (a.distance(b) / 0.01).ceil().clamp(4.0, 400.0) as usize;
        let mut start: Option<f64> = None;
        let mut last = 0.0;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let inside = covered(id, a + (b - a) * t);
            match (inside, start) {
                (true, None) => start = Some(t),
                (false, Some(s)) => {
                    out.push((id, s, last, a, b));
                    start = None;
                }
                _ => {}
            }
            if inside {
                last = t;
            }
        }
        if let Some(s) = start {
            out.push((id, s, 1.0, a, b));
        }
    }
    items.extend(
        out.into_iter()
            .filter(|(_, s, e, ..)| e > s)
            .map(|(id, s, e, a, b)| PlanItem::Line {
                id,
                pen: Pen::Hidden,
                a: a + (b - a) * s,
                b: a + (b - a) * e,
            }),
    );
}

/// How far `p` is from the nearest edge of `poly`.
fn edge_distance(p: DVec2, poly: &[DVec2]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let ab = b - a;
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
            p.distance(a + ab * t)
        })
        .fold(f64::INFINITY, f64::min)
}

pub fn is_underlay(item: &PlanItem) -> bool {
    matches!(
        item,
        PlanItem::Line {
            pen: Pen::Underlay,
            ..
        } | PlanItem::Arc {
            pen: Pen::Underlay,
            ..
        }
    )
}

/// How far guides reach each way from their points: endless for any
/// building, short enough to stay well inside float precision.
pub const GUIDE_REACH: f64 = 200.0;

/// A guide through `a` and `b` as a long segment.
pub fn guide_span(a: DVec2, b: DVec2) -> Option<(DVec2, DVec2)> {
    let d = (b - a).try_normalize()?;
    let m = (a + b) / 2.0;
    Some((m - d * GUIDE_REACH, m + d * GUIDE_REACH))
}

/// The plan of a shape with no simple footprint: its section at `z`,
/// filled, and its edges below `z` in projection.
/// A door or window in plan, where the plan cuts the wall at `cut_v` above
/// its base: the frame members the cut passes through, and for a window
/// the sill member below, the wall faces and the glass line; for a door
/// its leaf drawn open and the swing it sweeps. `at(u, s)` is a point
/// `u` along the wall and `s` across it.
fn opening_symbol(
    id: ElementId,
    o: &creabim_core::Opening,
    w: &creabim_core::Wall,
    c: &Cut,
    cut_v: f64,
    at: &dyn Fn(f64, f64) -> DVec2,
) -> Vec<PlanItem> {
    use crate::opening::{opening_boxes, Fitted, Part};
    let mut out = Vec::new();
    let Some(f) = Fitted::of(o, w) else {
        return out;
    };
    let z = cut_v - c.v0;
    let line = |out: &mut Vec<PlanItem>, a: DVec2, b: DVec2| {
        out.push(PlanItem::Line {
            id,
            pen: Pen::Symbol,
            a,
            b,
        })
    };
    let rect = |out: &mut Vec<PlanItem>, u0: f64, u1: f64, s0: f64, s1: f64| {
        out.push(PlanItem::Polygon {
            id,
            pen: Pen::Symbol,
            points: vec![at(u0, s0), at(u1, s0), at(u1, s1), at(u0, s1)],
        })
    };
    let boxes = opening_boxes(o, w);
    for b in &boxes {
        let (u0, u1) = (c.u0 + b.lo.x, c.u0 + b.hi.x);
        let cut = b.lo.z < z && z < b.hi.z;
        match b.part {
            // the sill member shows below the cut
            Part::Frame if cut || (o.kind == OpeningKind::Window && b.lo.z <= 1e-9) => {
                rect(&mut out, u0, u1, b.lo.y, b.hi.y)
            }
            Part::Glass if cut => {
                let s = (b.lo.y + b.hi.y) / 2.0;
                line(&mut out, at(u0, s), at(u1, s));
            }
            _ => {}
        }
    }
    match o.kind {
        // a hole that starts above the floor shows the wall's faces across,
        // over its sill, as a window does; one down to the floor is just
        // the gap (MU18)
        OpeningKind::Hole => {
            if c.v0 > 1e-9 {
                let t = w.thickness / 2.0;
                for s in [-t, t] {
                    line(&mut out, at(c.u0, s), at(c.u1, s));
                }
            }
        }
        OpeningKind::Window => {
            // the wall faces carry on across, over the sill
            let t = w.thickness / 2.0;
            for s in [-t, t] {
                line(&mut out, at(c.u0, s), at(c.u1, s));
            }
        }
        OpeningKind::Door => {
            let Some(leaf) = boxes.iter().find(|b| b.part == Part::Leaf) else {
                return out;
            };
            // the leaf turned open about its hinge on the face of the frame
            // on the side it opens to, and the quarter circle it sweeps
            let left = o.faces_left(w);
            let side = if left { 1.0 } else { -1.0 };
            let s = if left { leaf.hi.y } else { leaf.lo.y };
            let r = leaf.hi.x - leaf.lo.x;
            let (u, towards) = if o.flip_hand {
                (c.u0 + leaf.hi.x, -1.0)
            } else {
                (c.u0 + leaf.lo.x, 1.0)
            };
            rect(&mut out, u, u + towards * f.leaf(), s, s + side * r);
            let hinge = at(u, s);
            let closed = at(u + towards, s) - hinge;
            let open = at(u, s + side) - hinge;
            // arcs run anticlockwise: from whichever of the two comes first
            let a0 = if closed.perp_dot(open) > 0.0 {
                closed.to_angle()
            } else {
                open.to_angle()
            };
            out.push(PlanItem::Arc {
                id,
                pen: Pen::Symbol,
                center: hinge,
                radius: r,
                start: a0,
                end: a0 + std::f64::consts::FRAC_PI_2,
            });
        }
    }
    out
}

pub(crate) fn sliced_plan(
    model: &Model,
    item: &crate::scene::SceneItem,
    z: f64,
    items: &mut Vec<PlanItem>,
) {
    sliced_plan_creased(model, item, z, items, 0.995);
}

/// [`sliced_plan`] with its own crease for the edges seen below the cut
/// (see [`crate::section::feature_edges_over`]).
pub(crate) fn sliced_plan_creased(
    model: &Model,
    item: &crate::scene::SceneItem,
    z: f64,
    items: &mut Vec<PlanItem>,
    crease: f32,
) {
    let mut cut: Vec<(DVec2, DVec2)> = Vec::new();
    for lp in crate::slice::slice(&item.mesh, z) {
        let n = lp.len();
        for i in 0..n {
            cut.push((lp[i], lp[(i + 1) % n]));
            items.push(PlanItem::Line {
                id: item.id,
                pen: Pen::Cut,
                a: lp[i],
                b: lp[(i + 1) % n],
            });
        }
        let piece = CutPiece {
            id: item.id,
            points: vec![],
            layer: item.layer,
        };
        items.push(PlanItem::Fill {
            id: item.id,
            points: lp,
            hatch: piece.hatch(model),
            rgb: piece.colour(model),
        });
    }
    for (a, b) in crate::section::feature_edges_over(item, crease) {
        let (a, b) = if a.z <= b.z { (a, b) } else { (b, a) };
        if a.z >= z - 1e-6 {
            continue;
        }
        // keep the part below the cut
        let b = if b.z > z {
            a + (b - a) * ((z - a.z) / (b.z - a.z))
        } else {
            b
        };
        // edges seen end-on, and those lying in the cut, add nothing
        let (a, b) = (a.truncate(), b.truncate());
        if a.distance(b) < 1e-6 {
            continue;
        }
        // edges hidden under the heavy cut line
        let on = |p: DVec2, (s, e): (DVec2, DVec2)| {
            let d = e - s;
            let t = ((p - s).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
            p.distance(s + d * t) < 1e-4
        };
        if cut.iter().any(|&c| on(a, c) && on(b, c)) {
            continue;
        }
        items.push(PlanItem::Line {
            id: item.id,
            pen: Pen::Projection,
            a,
            b,
        });
    }
}

/// Section line with view arrows, or an elevation marker, as drawn in plan.
fn section_marker(
    items: &mut Vec<PlanItem>,
    id: ElementId,
    name: &str,
    a: DVec2,
    b: DVec2,
    cut: bool,
) {
    let along = (b - a).normalize_or_zero();
    let look = along.perp();
    let arrow = |at: DVec2| PlanItem::Polygon {
        id,
        pen: Pen::Symbol,
        points: vec![at - along * 0.35, at + along * 0.35, at + look * 0.45],
    };
    let tag = name.rsplit(' ').next().unwrap_or(name).to_string();
    if cut {
        items.push(PlanItem::Line {
            id,
            pen: Pen::Symbol,
            a,
            b,
        });
        for end in [a, b] {
            items.push(arrow(end));
        }
        items.push(PlanItem::Label {
            id,
            at: a - along * 0.7,
            text: tag.clone(),
        });
        items.push(PlanItem::Label {
            id,
            at: b + along * 0.7,
            text: tag,
        });
    } else {
        let mid = (a + b) / 2.0;
        items.push(arrow(mid));
        items.push(PlanItem::Label {
            id,
            at: mid - look * 0.6,
            text: tag,
        });
    }
}

/// The level a height stands on: the highest at or below it, else the
/// lowest.
pub fn level_under(model: &Model, z: f64) -> Option<ElementId> {
    let mut all: Vec<(ElementId, f64)> = model.levels().map(|(e, l)| (e.id, l.elevation)).collect();
    all.sort_by(|a, b| a.1.total_cmp(&b.1));
    all.iter()
        .rev()
        .find(|l| l.1 <= z + 0.05)
        .or(all.first())
        .map(|l| l.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cut_footings(model: &Model, level: &str, cut: f64) -> usize {
        let level = model
            .iter()
            .find(|e| e.name == level && matches!(e.kind, ElementKind::Level(_)))
            .unwrap()
            .id;
        let footings: std::collections::HashSet<ElementId> = plan_view(model, level, cut)
            .items
            .iter()
            .map(|it| it.id())
            .filter(|id| {
                matches!(
                    model.get(*id).map(|e| &e.kind),
                    Some(ElementKind::Footing(_))
                )
            })
            .collect();
        footings.len()
    }

    #[test]
    fn a_wall_attached_under_the_cut_is_drawn_in_projection() {
        use creabim_core::{Document, Element, Transaction};
        let m = creabim_core::sample::sample_house();
        let (wid, w) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) if w.curtain.is_none() => Some((e.id, w.clone())),
                _ => None,
            })
            .unwrap();
        let cut = |m: &Model| {
            plan_view(m, w.level, 1.2).items.iter().any(|it| {
                it.id() == wid
                    && matches!(
                        it,
                        PlanItem::Polygon { pen: Pen::Cut, .. } | PlanItem::Fill { .. }
                    )
            })
        };
        assert!(cut(&m), "a full-height wall is cut at 1.2 m");
        // a slab over the whole wall with its underside at 1 m, and the
        // wall's top attached to it
        let (lo, hi) = (w.start.min(w.end), w.start.max(w.end));
        let pad = DVec2::splat(1.0);
        let mut slab = creabim_core::Slab::new(
            w.level,
            vec![
                lo - pad,
                DVec2::new(hi.x + 1.0, lo.y - 1.0),
                hi + pad,
                DVec2::new(lo.x - 1.0, hi.y + 1.0),
            ],
            0.2,
        );
        slab.top_offset = 1.2;
        let se = Element::new("Low slab", ElementKind::Slab(slab));
        let sid = se.id;
        let mut d = Document::from_model(m);
        d.commit(Transaction::new("slab").insert(se)).unwrap();
        d.edit(wid, "attach", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                w.attach_top = vec![sid];
            }
        })
        .unwrap();
        assert!(!cut(d.model()), "its top is under the cut");
        assert!(plan_view(d.model(), w.level, 1.2).items.iter().any(|it| {
            it.id() == wid
                && matches!(
                    it,
                    PlanItem::Polygon {
                        pen: Pen::Projection,
                        ..
                    }
                )
        }));
    }

    #[test]
    fn a_foundation_level_plan_cuts_the_footings_under_the_floor_above() {
        let m = creabim_core::starter::starter_house();
        // 0.25 above the foundation level is 0.55 below the ground floor:
        // inside every footing (top -0.35, 0.45 deep)
        assert_eq!(cut_footings(&m, "Foundation", 0.25), 16);
        // a cut above their tops passes none of them
        assert_eq!(cut_footings(&m, "Foundation", 0.6), 0);
    }

    fn named(m: &Model, name: &str) -> ElementId {
        m.iter().find(|e| e.name == name).expect(name).id
    }

    fn view_items(m: &Model, view: &str) -> Vec<PlanItem> {
        crate::section::view_drawing(m, named(m, view))
            .unwrap()
            .items
    }

    /// How many closed outlines a set of segments makes, joined end to
    /// end; every corner must be met by an even number of them.
    fn loops(segs: &[(DVec2, DVec2)]) -> usize {
        type Key = (i64, i64);
        let key = |p: DVec2| ((p.x * 1e4).round() as i64, (p.y * 1e4).round() as i64);
        fn find(p: &mut std::collections::HashMap<Key, Key>, k: Key) -> Key {
            let up = *p.entry(k).or_insert(k);
            if up == k {
                k
            } else {
                let r = find(p, up);
                p.insert(k, r);
                r
            }
        }
        let mut parent: std::collections::HashMap<Key, Key> = Default::default();
        let mut degree: std::collections::HashMap<Key, usize> = Default::default();
        for (a, b) in segs {
            let (ka, kb) = (key(*a), key(*b));
            *degree.entry(ka).or_default() += 1;
            *degree.entry(kb).or_default() += 1;
            let (ra, rb) = (find(&mut parent, ka), find(&mut parent, kb));
            parent.insert(ra, rb);
        }
        assert!(degree.values().all(|d| d % 2 == 0), "an open outline");
        let keys: Vec<Key> = parent.keys().copied().collect();
        let roots: std::collections::HashSet<Key> =
            keys.into_iter().map(|k| find(&mut parent, k)).collect();
        roots.len()
    }

    #[test]
    fn footings_meeting_at_corners_merge_into_one_outline() {
        let m = creabim_core::starter::starter_house();
        let items = view_items(&m, "Foundation plan");
        let footing =
            |id: &ElementId| matches!(m.get(*id).map(|e| &e.kind), Some(ElementKind::Footing(_)));
        let fills: Vec<&Vec<DVec2>> = items
            .iter()
            .filter_map(|it| match it {
                PlanItem::Fill { id, points, .. } if footing(id) => Some(points),
                _ => None,
            })
            .collect();
        let cut: Vec<(DVec2, DVec2)> = items
            .iter()
            .filter_map(|it| match it {
                PlanItem::Line {
                    id,
                    pen: Pen::Cut,
                    a,
                    b,
                } if footing(id) => Some((*a, *b)),
                _ => None,
            })
            .collect();
        // no outline runs inside another footing: no overlap squares
        for (a, b) in &cut {
            let mid = (*a + *b) / 2.0;
            for f in &fills {
                assert!(
                    !(point_in(mid, f) && edge_distance(mid, f) > 1e-3),
                    "a footing edge {a} {b} runs inside another"
                );
            }
        }
        // the strips make one network: an outline round its outside and
        // one round each bay inside it; each pad stands alone
        let pads = m
            .iter()
            .filter(|e| {
                matches!(&e.kind, ElementKind::Footing(f)
                    if f.kind == creabim_core::FootingKind::Pad)
            })
            .count();
        let n = loops(&cut);
        // (the starter house has seven bays between its strips)
        assert_eq!(n, 1 + 7 + pads, "{n} outlines, {pads} pads");
    }

    #[test]
    fn an_l_shaped_hole_is_crossed_in_each_leg_inside_its_outline() {
        let l = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(1.0, 0.0),
            DVec2::new(1.0, 3.0),
            DVec2::new(3.0, 3.0),
            DVec2::new(3.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let pieces = convex_pieces(&l);
        assert_eq!(pieces.len(), 2);
        let area: f64 = pieces.iter().map(|p| signed_area(p).abs()).sum();
        assert!((area - signed_area(&l).abs()).abs() < 1e-9);
        let cross = hole_cross(&l);
        assert_eq!(cross.len(), 4);
        for (a, b) in cross {
            for t in [0.25, 0.5, 0.75] {
                let p = a + (b - a) * t;
                assert!(point_in(p, &l), "{p} is outside the hole");
            }
        }
        // a hole that isn't square splits into convex pieces too
        let tilted: Vec<DVec2> = l
            .iter()
            .map(|p| DVec2::from_angle(0.3).rotate(*p))
            .collect();
        let pieces = convex_pieces(&tilted);
        assert!(pieces.len() >= 2);
        for piece in pieces {
            assert!(is_convex(&piece));
        }
    }

    #[test]
    fn the_first_floor_shows_the_stair_through_its_well() {
        let m = creabim_core::starter::starter_house();
        let stair = named(&m, "Stair");
        let shaft = named(&m, "Stair Well");
        let well = match &m.get(shaft).unwrap().kind {
            ElementKind::Shaft(s) => s.boundary.clone(),
            _ => unreachable!(),
        };
        let items = view_items(&m, "First Floor plan");
        let treads = items
            .iter()
            .filter(
                |it| matches!(it, PlanItem::Line { id, pen: Pen::Projection, .. } if *id == stair),
            )
            .count();
        assert!(treads >= 15, "{treads} lines of the stair");
        // all of it inside the well
        for it in items.iter().filter(|it| it.id() == stair) {
            if let PlanItem::Line { a, b, .. } = it {
                let mid = (*a + *b) / 2.0;
                assert!(point_in(mid, &well) || edge_distance(mid, &well) < 1e-6);
            }
        }
        assert!(items.iter().any(
            |it| matches!(it, PlanItem::Label { id, text, .. } if *id == stair && text == "DN")
        ));
        // the well keeps its outline, with no cross over the stair
        assert!(!items
            .iter()
            .any(|it| matches!(it, PlanItem::Line { id, .. } if *id == shaft)));
        assert!(items
            .iter()
            .any(|it| matches!(it, PlanItem::Polygon { id, .. } if *id == shaft)));
        // a view depth too shallow to reach the stair's foot shows less
        let f = named(&m, "First Floor");
        let shallow = creabim_core::PlanGraphics {
            range: creabim_core::graphics::PlanRange {
                view_depth: Some(1.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let count = |g: &creabim_core::PlanGraphics| {
            plan_view_with(&m, f, 1.2, g)
                .items
                .iter()
                .filter(|it| it.id() == stair)
                .count()
        };
        let (some, all) = (count(&shallow), count(&Default::default()));
        assert!(some > 0 && some < all, "{some} of {all}");
        // on the ground floor the well is only the dashed edge above
        let g = view_items(&m, "Ground Floor plan");
        assert!(g.iter().filter(|it| it.id() == shaft).all(|it| matches!(
            it,
            PlanItem::Line {
                pen: Pen::Hidden,
                ..
            }
        )));
    }

    #[test]
    fn what_lies_below_the_cut_is_tinted() {
        let m = creabim_core::starter::starter_house();
        let g = named(&m, "Ground Floor");
        let shades = |graphics: &creabim_core::PlanGraphics| {
            plan_view_with(&m, g, 1.2, graphics)
                .items
                .iter()
                .filter(|it| matches!(it, PlanItem::Shade { .. }))
                .count()
        };
        let on = shades(&Default::default());
        assert!(on >= 10, "{on}");
        let off = creabim_core::PlanGraphics {
            below_fill: false,
            ..Default::default()
        };
        assert_eq!(shades(&off), 0);
        // tints come first, under every line, and are light
        let items = plan_view(&m, g, 1.2).items;
        let last = items
            .iter()
            .rposition(|it| matches!(it, PlanItem::Shade { .. }))
            .unwrap();
        for it in &items[..=last] {
            let PlanItem::Shade { rgb, .. } = it else {
                panic!("{it:?} under a tint");
            };
            assert!(rgb.iter().all(|c| *c > 0.6), "{rgb:?}");
        }
    }

    #[test]
    fn furniture_is_drawn_lighter_than_walls_and_slabs() {
        let m = creabim_core::starter::starter_house();
        let items = view_items(&m, "Ground Floor plan");
        let objects: std::collections::HashSet<String> = items
            .iter()
            .filter(|it| {
                m.get(it.id())
                    .is_some_and(|e| matches!(e.kind, ElementKind::Object(_)))
            })
            .filter_map(|it| match it {
                PlanItem::Line { pen, .. } | PlanItem::Polygon { pen, .. } => {
                    Some(format!("{pen:?}"))
                }
                _ => None,
            })
            .collect();
        assert!(objects.contains("Fine"));
        assert!(!objects.contains("Projection") && !objects.contains("Cut"));
        let w = creabim_core::LineWeights::for_scale(50.0);
        let width = |p| crate::paper::pen_stroke_with(p, &w).width;
        assert!(width(Pen::Cut) > width(Pen::Projection));
        assert!(width(Pen::Projection) > width(Pen::Symbol));
        assert!(width(Pen::Symbol) > width(Pen::Fine));
        // a view's own weights reach its sheet
        let mut m2 = m.clone();
        let v = named(&m2, "Ground Floor plan");
        let before = m2.get(v).unwrap().clone();
        let mut after = before.clone();
        if let ElementKind::View(view) = &mut after.kind {
            let mut heavy = creabim_core::LineWeights::for_scale(50.0);
            heavy.cut = 1.4;
            view.graphics.weights = Some(heavy);
        }
        creabim_core::Transaction::new("Line weights")
            .replace(before, after)
            .apply(&mut m2)
            .unwrap();
        let sheet = named(&m2, "Ground Floor Plan");
        let svg = crate::sheet::sheet_svg(&m2, sheet, &crate::sheet::SheetInfo::default()).unwrap();
        assert!(svg.contains("stroke-width=\"1.4\""));
    }

    #[test]
    fn an_opening_above_the_cut_is_drawn_dashed_in_its_wall() {
        use creabim_core::{Element, Level, Opening, Transaction, Wall};
        let mut m = Model::new();
        let l = Element::new(
            "Ground",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let level = l.id;
        let w = Element::new(
            "W",
            ElementKind::Wall(Wall::new(
                level,
                DVec2::ZERO,
                DVec2::new(4.0, 0.0),
                0.2,
                3.0,
            )),
        );
        let wall = w.id;
        let duct = Element::new(
            "Duct",
            ElementKind::Opening(Opening {
                host: wall,
                kind: OpeningKind::Hole,
                offset: 1.0,
                sill: 2.2,
                width: 0.4,
                height: 0.3,
                material: None,
                frame: Default::default(),
                flip_hand: false,
                flip_facing: false,
            }),
        );
        let hole = duct.id;
        Transaction::new("t")
            .insert(l)
            .insert(w)
            .insert(duct)
            .apply(&mut m)
            .unwrap();
        let items = plan_view(&m, level, 1.2).items;
        let dashed: Vec<&PlanItem> = items
            .iter()
            .filter(|it| matches!(it, PlanItem::Line { id, pen: Pen::Hidden, .. } if *id == hole))
            .collect();
        assert_eq!(dashed.len(), 4);
        // drawn over the wall's fill, so it shows
        let fill = items
            .iter()
            .position(|it| matches!(it, PlanItem::Fill { id, .. } if *id == wall))
            .unwrap();
        let first = items.iter().position(|it| it.id() == hole).unwrap();
        assert!(first > fill);
        // the wall stays whole at the cut
        assert!(!items
            .iter()
            .any(|it| matches!(it, PlanItem::Line { id, pen: Pen::Symbol, .. } if *id == hole)));
    }

    /// A slab layer's edges moved by hand show in plan (A7-10): the top
    /// layer pushed out 500 mm reaches 500 mm past the slab's own edge.
    #[test]
    fn slab_layer_edits_show_in_plan() {
        let m = creabim_core::starter::starter_house();
        let (id, level, n) = m
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Slab(s) => {
                    let (_, a) = creabim_core::assembly::assembly_of(&m, e)?;
                    (a.layers.len() > 1).then_some((e.id, s.level, s.boundary.len()))
                }
                _ => None,
            })
            .expect("a layered slab");
        let far = |m: &Model| {
            plan_view(m, level, 1.2)
                .items
                .iter()
                .filter(|it| it.id() == id)
                .filter_map(|it| match it {
                    PlanItem::Polygon { points, .. } => {
                        Some(points.iter().map(|p| p.x).fold(f64::MIN, f64::max))
                    }
                    _ => None,
                })
                .fold(f64::MIN, f64::max)
        };
        let before = far(&m);
        let mut e = m.get(id).unwrap().clone();
        if let ElementKind::Slab(s) = &mut e.kind {
            for edge in 0..n {
                creabim_core::set_layer_edge(&mut s.layer_edges, 0, edge, 0.5);
            }
        }
        let mut doc = creabim_core::Document::from_model(m.clone());
        let old = m.get(id).unwrap().clone();
        doc.commit(creabim_core::Transaction::new("push").replace(old, e))
            .unwrap();
        let after = far(doc.model());
        assert!(
            (after - before - 0.5).abs() < 1e-6,
            "the plan's furthest slab edge moved {:.3} m, wanted 0.5",
            after - before
        );
    }

    /// The starter house's Ground Floor plan hides its footings; shown
    /// again (the "Show footings" button under its view depth, A7-3) they
    /// draw dashed under the floor.
    #[test]
    fn shown_footings_draw_dashed_under_the_floor() {
        let m = creabim_core::starter::starter_house();
        let footings: std::collections::HashSet<ElementId> = m
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Footing(_)))
            .map(|e| e.id)
            .collect();
        let plan = m
            .iter()
            .find(|e| e.name == "Ground Floor plan" && matches!(e.kind, ElementKind::View(_)))
            .unwrap()
            .clone();
        let dashed = |m: &Model| {
            crate::section::view_drawing_with(m, plan.id, false)
                .unwrap()
                .items
                .iter()
                .filter(|it| {
                    footings.contains(&it.id())
                        && matches!(
                            it,
                            PlanItem::Line {
                                pen: Pen::Hidden,
                                ..
                            }
                        )
                })
                .count()
        };
        assert_eq!(dashed(&m), 0, "hidden in the view");
        let mut shown = plan.clone();
        if let ElementKind::View(v) = &mut shown.kind {
            v.hidden
                .categories
                .retain(|c| *c != creabim_core::visibility::Category::Footings);
        }
        let mut doc = creabim_core::Document::from_model(m.clone());
        doc.commit(creabim_core::Transaction::new("show").replace(plan.clone(), shown))
            .unwrap();
        assert!(dashed(doc.model()) > 20, "shown, dashed under the floor");
    }

    /// A closed sketch draws as a face (a wash inside its outline); an
    /// open one, or a closed one that crosses itself, as lines only (A7-1).
    #[test]
    fn a_closed_sketch_is_washed_as_a_face() {
        let plane = creabim_core::Workplane::ground(0.0);
        let square = vec![
            DVec2::ZERO,
            DVec2::new(2.0, 0.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(0.0, 2.0),
        ];
        let bow = vec![
            DVec2::ZERO,
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 0.0),
            DVec2::new(0.0, 2.0),
        ];
        let washed = |pts: Vec<DVec2>, closed: bool| {
            let sk = creabim_core::Sketch::polyline(None, plane, pts, closed);
            sketch_items(ElementId::NIL, &sk, &|p| p)
                .iter()
                .any(|it| matches!(it, PlanItem::Tint { .. }))
        };
        assert!(washed(square.clone(), true));
        assert!(!washed(square, false));
        assert!(!washed(bow, true));
    }

    #[test]
    fn a_beam_on_a_roof_slope_is_filled_in_its_colour_in_the_plan() {
        let lv = creabim_core::Element::new(
            "L1",
            ElementKind::Level(creabim_core::Level {
                elevation: 3.0,
                height: 3.0,
            }),
        );
        let mut b = creabim_core::Beam::new(lv.id, DVec2::ZERO, DVec2::new(4.0, 0.0));
        // rising through the cut (6 m) from 5.6 to 7.6
        b.top_offset = 0.4;
        b.rise = [0.0, 2.0];
        let be = creabim_core::Element::new("Beam", ElementKind::Beam(b));
        let id = be.id;
        let m = Model::from_elements([lv.clone(), be]).unwrap();
        let g = creabim_core::PlanGraphics {
            below_fill: true,
            below_strength: 1.0,
            ..Default::default()
        };
        let v = plan_view_with(&m, lv.id, 3.0, &g);
        assert!(
            v.items
                .iter()
                .any(|it| matches!(it, PlanItem::Shade { id: i, .. } if *i == id)),
            "the beam has its fill"
        );
    }
}
