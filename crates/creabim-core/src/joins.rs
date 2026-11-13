//! Wall joins: how wall ends are trimmed or extended where walls meet, and
//! how joined walls follow when one of them moves.
//!
//! Two walls whose ends meet (an L) are mitred, so their faces meet cleanly
//! at the corner; walls built of the same assembly then meet layer by layer
//! along the mitre. A wall that ends on the middle of another (a T) is
//! trimmed back to that wall's face; when both are layered, its core runs
//! on through the other's finishes to the other's core.
//!
//! Where three or more wall ends meet at one point (two walls of a run
//! ending on a partition's end, or four ends in a cross) the two that run
//! on most nearly straight, the thicker pair first, join each other as if
//! the rest were not there, and every other wall is trimmed back to their
//! faces as at a T, its core running on to theirs.
//!
//! Two walls that cross (an X) clean up too: the thicker one (or, as thick,
//! the one drawn first) runs through and the other stops at its faces on
//! either side, its core running on to the other's core when both are
//! layered. That leaves a [`Gap`] in the second wall.
//!
//! Ends don't have to coincide exactly: an end within a wall's thickness
//! of where the centrelines cross still joins, so a wall nudged a little
//! keeps clean corners. Moving a wall also takes the walls joined to it
//! along ([`follow`]), as in Revit: their ends stretch to stay joined.
//!
//! An end whose join is disallowed ([`Wall::disallow_join`]) joins nothing:
//! it stays square, butting into what it meets, and the walls it met no
//! longer follow it (nor it them).

use crate::{assembly, Element, ElementId, ElementKind, LayerFunction, Model, Wall};
use glam::DVec2;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------- join rules

/// Which element wins where two elements' cuts overlap in a drawing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JoinWinner {
    /// Layer by layer, by each layer's priority ([`layer_priority`]):
    /// cores join cores and finishes stop at the structure. Where two
    /// layers are as strong, columns win over walls and walls over slabs,
    /// then the element drawn first.
    #[default]
    Auto,
    /// The rule's first element cuts through every layer of the second.
    First,
    /// The second cuts through every layer of the first.
    Second,
}

impl JoinWinner {
    pub const ALL: [JoinWinner; 3] = [JoinWinner::Auto, JoinWinner::First, JoinWinner::Second];
}

/// How two elements join where they meet in drawings, set by hand for one
/// junction: Revit's *Switch Join Order* and *Unjoin*, ArchiCAD's
/// priorities overridden. Without one, every junction joins
/// automatically. Deleting either element deletes the rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct JoinRule {
    pub a: ElementId,
    pub b: ElementId,
    #[serde(default)]
    pub winner: JoinWinner,
    /// Off leaves the two overlapping as they are, each drawn whole.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub cleanup: bool,
    /// How the layers meet where two wall ends make a corner, when
    /// neither runs through.
    #[serde(default, skip_serializing_if = "CornerJoin::is_auto")]
    pub corner: CornerJoin,
}

/// How two layered walls' layers meet at a corner when neither wall runs
/// through the other.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CornerJoin {
    /// As the walls join by themselves: every layer meets on the mitre.
    #[default]
    Auto,
    /// The finishes meet on the mitre, the cores lap: the stronger wall's
    /// core runs on to the far face of the other's, whose core stops at
    /// it.
    Lap,
    /// Every layer meets its partner on the mitre.
    Mitre,
}

impl CornerJoin {
    fn is_auto(&self) -> bool {
        *self == CornerJoin::Auto
    }
}

/// The corner join presets (3.7), as a wall's end offers them for the
/// one wall it meets there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CornerPreset {
    Auto,
    CoresLap,
    ThisRunsThrough,
    OtherRunsThrough,
    MitreEveryLayer,
    NotJoined,
}

impl CornerPreset {
    pub const ALL: [CornerPreset; 6] = [
        CornerPreset::Auto,
        CornerPreset::CoresLap,
        CornerPreset::ThisRunsThrough,
        CornerPreset::OtherRunsThrough,
        CornerPreset::MitreEveryLayer,
        CornerPreset::NotJoined,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CornerPreset::Auto => "Auto",
            CornerPreset::CoresLap => "Cores lap, finishes stop",
            CornerPreset::ThisRunsThrough => "This wall runs through",
            CornerPreset::OtherRunsThrough => "The other wall runs through",
            CornerPreset::MitreEveryLayer => "Mitre every layer",
            CornerPreset::NotJoined => "Not joined",
        }
    }

    /// The preset rule `r` (none: automatic) makes for wall `me`.
    pub fn of(r: Option<&JoinRule>, me: ElementId) -> CornerPreset {
        let Some(r) = r else {
            return CornerPreset::Auto;
        };
        if !r.cleanup {
            return CornerPreset::NotJoined;
        }
        match r.winner {
            JoinWinner::First if r.a == me => return CornerPreset::ThisRunsThrough,
            JoinWinner::Second if r.b == me => return CornerPreset::ThisRunsThrough,
            JoinWinner::First | JoinWinner::Second => return CornerPreset::OtherRunsThrough,
            JoinWinner::Auto => {}
        }
        match r.corner {
            CornerJoin::Auto => CornerPreset::Auto,
            CornerJoin::Lap => CornerPreset::CoresLap,
            CornerJoin::Mitre => CornerPreset::MitreEveryLayer,
        }
    }

    /// Make rule `r` this preset, seen from wall `me`.
    pub fn apply(self, r: &mut JoinRule, me: ElementId) {
        r.cleanup = self != CornerPreset::NotJoined;
        let me_first = r.a == me;
        r.winner = match self {
            CornerPreset::ThisRunsThrough if me_first => JoinWinner::First,
            CornerPreset::ThisRunsThrough => JoinWinner::Second,
            CornerPreset::OtherRunsThrough if me_first => JoinWinner::Second,
            CornerPreset::OtherRunsThrough => JoinWinner::First,
            _ => JoinWinner::Auto,
        };
        r.corner = match self {
            CornerPreset::CoresLap => CornerJoin::Lap,
            CornerPreset::MitreEveryLayer => CornerJoin::Mitre,
            _ => CornerJoin::Auto,
        };
    }
}

/// The one wall whose end meets the start (`at_end` false) or end of
/// wall `id` at a corner, if exactly one does.
pub fn corner_partner(model: &Model, id: ElementId, at_end: bool) -> Option<ElementId> {
    let walls = joinables(model);
    let me = walls.iter().find(|j| j.id == id)?;
    let w = me.wall;
    let (p, other) = if at_end {
        (w.end, w.start)
    } else {
        (w.start, w.end)
    };
    let d_in = (p - other).normalize_or_zero();
    match meeting(&walls, me, p, d_in).as_slice() {
        [(o, _)] => Some(o.id),
        _ => None,
    }
}

fn yes() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

impl JoinRule {
    pub fn new(a: ElementId, b: ElementId) -> Self {
        Self {
            a,
            b,
            winner: JoinWinner::Auto,
            cleanup: true,
            corner: CornerJoin::Auto,
        }
    }

    /// Is this the rule for the junction of `x` and `y`, either way round?
    pub fn joins(&self, x: ElementId, y: ElementId) -> bool {
        (self.a == x && self.b == y) || (self.a == y && self.b == x)
    }

    /// Does `x` cut through `y` outright under this rule? `None` when it
    /// leaves that to the layers' priorities.
    pub fn wins(&self, x: ElementId, y: ElementId) -> Option<bool> {
        match self.winner {
            JoinWinner::Auto => None,
            JoinWinner::First => Some(self.a == x && self.b == y),
            JoinWinner::Second => Some(self.b == x && self.a == y),
        }
    }

    /// The same rule as an automatic join: nothing left to keep.
    pub fn is_default(&self) -> bool {
        self.winner == JoinWinner::Auto && self.cleanup && self.corner == CornerJoin::Auto
    }
}

/// The rule set by hand for the junction of `x` and `y`, with its id.
pub fn rule_for(model: &Model, x: ElementId, y: ElementId) -> Option<(ElementId, JoinRule)> {
    model.iter().find_map(|e| match &e.kind {
        ElementKind::Join(r) if r.joins(x, y) => Some((e.id, *r)),
        _ => None,
    })
}

/// Every junction rule in the model.
pub fn rules(model: &Model) -> Vec<JoinRule> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Join(r) => Some(*r),
            _ => None,
        })
        .collect()
}

/// Which kind of element runs through which where two are as strong:
/// columns through walls, walls through slabs.
pub fn category_rank(kind: &ElementKind) -> u8 {
    match kind {
        ElementKind::Column(_) => 3,
        ElementKind::Wall(_) => 2,
        ElementKind::Slab(_) => 1,
        _ => 0,
    }
}

/// Can `kind` take part in a junction rule: is it cut and filled in
/// drawings?
pub fn joinable(kind: &ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::Wall(_)
            | ElementKind::Slab(_)
            | ElementKind::Roof(_)
            | ElementKind::Column(_)
            | ElementKind::Beam(_)
            | ElementKind::Footing(_)
            | ElementKind::Ceiling(_)
    )
}

/// Of `a` and `b`, the one that runs through where their layers are as
/// strong and no rule says otherwise: the higher kind, then the one drawn
/// first.
pub fn default_winner(model: &Model, a: ElementId, b: ElementId) -> ElementId {
    let rank = |id: ElementId| model.get(id).map_or(0, |e| category_rank(&e.kind));
    if rank(a) != rank(b) {
        return if rank(a) > rank(b) { a } else { b };
    }
    let first = model.iter().find(|e| e.id == a || e.id == b);
    first.map_or(a, |e| e.id)
}

/// The edit that sets the junction of `a` and `b` to what `f` makes of its
/// rule (an automatic one when it has none): the rule is added, changed,
/// or taken away when it is back to automatic.
pub fn set_rule(
    model: &Model,
    a: ElementId,
    b: ElementId,
    label: &str,
    f: impl FnOnce(&mut JoinRule),
) -> crate::Transaction {
    let tx = crate::Transaction::new(label);
    let old = rule_for(model, a, b);
    let mut rule = old.map_or(JoinRule::new(a, b), |(_, r)| r);
    f(&mut rule);
    match (old, rule.is_default()) {
        (None, true) => tx,
        (None, false) => tx.insert(Element::new("Junction", ElementKind::Join(rule))),
        (Some((id, _)), true) => tx.remove_cascade(model, id),
        (Some((id, _)), false) => {
            let before = model.get(id).cloned().expect("a rule");
            let mut after = before.clone();
            after.kind = ElementKind::Join(rule);
            tx.replace(before, after)
        }
    }
}

/// Switch which of `a` and `b` cuts through the other (Revit's *Switch
/// Join Order*): from automatic, the one that would lose wins outright;
/// after that, the two swap.
pub fn switch_order(model: &Model, a: ElementId, b: ElementId) -> crate::Transaction {
    let loser = if default_winner(model, a, b) == a {
        b
    } else {
        a
    };
    set_rule(model, a, b, "Switch join order", |r| {
        let wins_now = match r.winner {
            JoinWinner::Auto => None,
            JoinWinner::First => Some(r.a),
            JoinWinner::Second => Some(r.b),
        };
        let next = match wins_now {
            None => loser,
            Some(w) if w == r.a => r.b,
            Some(_) => r.a,
        };
        r.winner = if next == r.a {
            JoinWinner::First
        } else {
            JoinWinner::Second
        };
        r.cleanup = true;
    })
}

/// How strongly a layer holds its place where elements meet: lower runs
/// through higher, as Revit's layer priorities. Structure 1, substrate 2,
/// insulation, air gaps and membranes 3, finishes 4; a layer's own
/// `priority` overrides its function's.
pub fn layer_priority(function: LayerFunction, own: Option<u8>) -> u8 {
    own.unwrap_or(match function {
        LayerFunction::Structure => 1,
        LayerFunction::Substrate => 2,
        LayerFunction::Insulation | LayerFunction::AirGap | LayerFunction::Membrane => 3,
        LayerFunction::Finish => 4,
    })
}

/// How far each face of a wall runs past its nominal end, in metres along the
/// wall. Index 0 is the face on the wall's left (start → end), index 1 the
/// face on its right. Negative values trim the wall.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WallEnds {
    pub start: [f64; 2],
    pub end: [f64; 2],
    /// How much further a layered wall's core runs than its faces, at the
    /// start and at the end: where it meets another layered wall's core
    /// through that wall's finishes.
    pub core: [f64; 2],
    /// Where the core ends instead, at the start and at the end, when its
    /// cores lap with the wall it meets: the line `u = a + b·s` along the
    /// wall (`s` across it, left positive), on the face of the other's
    /// core.
    pub core_line: [Option<(f64, f64)>; 2],
    /// Where walls crossing this one run through it, along it in order.
    pub gaps: Vec<Gap>,
}

/// Where another wall runs through a wall: the wall stops at `lo` and
/// starts again at `hi`, measured along it at its left face (index 0) and
/// its right face (index 1), so the cut is slanted when the walls cross
/// at an angle. A layered wall's core runs `core` further into the gap
/// from each side, to the core of the wall crossing it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gap {
    pub lo: [f64; 2],
    pub hi: [f64; 2],
    pub core: [f64; 2],
}

impl Gap {
    /// Where the gap starts on the line `s` across the wall.
    pub fn lo_at(&self, wall: &Wall, s: f64) -> f64 {
        let k = across(wall, s);
        self.lo[1] + (self.lo[0] - self.lo[1]) * k
    }

    /// Where the wall starts again on the line `s` across it.
    pub fn hi_at(&self, wall: &Wall, s: f64) -> f64 {
        let k = across(wall, s);
        self.hi[1] + (self.hi[0] - self.hi[1]) * k
    }

    /// Where the gap starts on the centreline: the `u` that stands for its
    /// start in drawings and meshes built along the wall.
    pub fn lo_mid(&self) -> f64 {
        (self.lo[0] + self.lo[1]) / 2.0
    }

    pub fn hi_mid(&self) -> f64 {
        (self.hi[0] + self.hi[1]) / 2.0
    }
}

/// One end of a piece of wall: the wall's own start or end, or one side
/// of a gap.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Bound {
    Start,
    End,
    /// The piece stops where gap `i` starts.
    GapLo(usize),
    /// The piece starts where gap `i` ends.
    GapHi(usize),
}

impl WallEnds {
    /// Where a mesh or drawing laid out along the wall's centreline puts
    /// the point `u` along it, on the line `s` across it: the wall's ends
    /// and the sides of its gaps (given by their centreline `u`) follow
    /// the joins; anything else stays at `u`. `core` for a layer that runs
    /// on past the faces.
    pub fn place(&self, wall: &Wall, u: f64, s: f64, core: bool) -> f64 {
        self.place_layer(wall, u, s, core, [0.0; 2])
    }

    /// As [`WallEnds::place`], for a layer whose ends were moved by hand
    /// `ext` past where the joins put them (start, end).
    pub fn place_layer(&self, wall: &Wall, u: f64, s: f64, core: bool, ext: [f64; 2]) -> f64 {
        if u <= 0.0 {
            return self.layer_start(wall, s, core) - ext[0];
        }
        if u >= wall.length() {
            return self.layer_end(wall, s, core) + ext[1];
        }
        for g in &self.gaps {
            if (u - g.lo_mid()).abs() < 1e-9 {
                return g.lo_at(wall, s) + if core { g.core[0] } else { 0.0 };
            }
            if (u - g.hi_mid()).abs() < 1e-9 {
                return g.hi_at(wall, s) - if core { g.core[1] } else { 0.0 };
            }
        }
        u
    }

    /// The stretches of the wall between its gaps, as centreline `u`
    /// spans (see [`WallEnds::place`]): the whole wall when nothing crosses it.
    pub fn solid_spans(&self, wall: &Wall) -> Vec<(f64, f64)> {
        let mut out = vec![];
        let mut u = 0.0;
        for g in &self.gaps {
            out.push((u, g.lo_mid()));
            u = g.hi_mid();
        }
        out.push((u, wall.length()));
        out
    }

    /// Is centreline `u` inside a gap?
    pub fn in_gap(&self, u: f64) -> bool {
        self.gaps.iter().any(|g| u > g.lo_mid() && u < g.hi_mid())
    }

    fn bound_u(&self, wall: &Wall, b: Bound, s: f64) -> f64 {
        match b {
            Bound::Start => self.start_at(wall, s),
            Bound::End => self.end_at(wall, s),
            Bound::GapLo(i) => self.gaps[i].lo_at(wall, s),
            Bound::GapHi(i) => self.gaps[i].hi_at(wall, s),
        }
    }

    /// How much further the core runs past a piece's bound.
    fn bound_core(&self, b: Bound) -> f64 {
        match b {
            Bound::Start => self.core[0],
            Bound::End => self.core[1],
            Bound::GapLo(i) => self.gaps[i].core[0],
            Bound::GapHi(i) => self.gaps[i].core[1],
        }
    }

    /// Wall-local `u` where the face on `side` (+1 left, -1 right) ends at the start.
    pub fn start_u(&self, side: f64) -> f64 {
        -self.start[if side > 0.0 { 0 } else { 1 }]
    }

    /// Wall-local `u` where the face on `side` ends at the far end.
    pub fn end_u(&self, wall: &Wall, side: f64) -> f64 {
        wall.length() + self.end[if side > 0.0 { 0 } else { 1 }]
    }

    /// Where the start's end line crosses the line `s` across the wall
    /// (left positive): the end is straight from one face to the other.
    pub fn start_at(&self, wall: &Wall, s: f64) -> f64 {
        let k = across(wall, s);
        self.start_u(-1.0) + (self.start_u(1.0) - self.start_u(-1.0)) * k
    }

    /// Where the far end's line crosses the line `s` across the wall.
    pub fn end_at(&self, wall: &Wall, s: f64) -> f64 {
        let k = across(wall, s);
        self.end_u(wall, -1.0) + (self.end_u(wall, 1.0) - self.end_u(wall, -1.0)) * k
    }

    /// The start of a layer at `s`: the end line, or past it for the core.
    pub fn layer_start(&self, wall: &Wall, s: f64, core: bool) -> f64 {
        match self.core_line[0] {
            Some((a, b)) if core => a + b * s,
            _ => self.start_at(wall, s) - if core { self.core[0] } else { 0.0 },
        }
    }

    /// The far end of a layer at `s`.
    pub fn layer_end(&self, wall: &Wall, s: f64, core: bool) -> f64 {
        match self.core_line[1] {
            Some((a, b)) if core => a + b * s,
            _ => self.end_at(wall, s) + if core { self.core[1] } else { 0.0 },
        }
    }

    /// The wall's footprint as a polygon (left start, left end, right end, right start).
    pub fn footprint(&self, wall: &Wall) -> [DVec2; 4] {
        let d = wall.direction();
        let n = d.perp();
        let t = wall.thickness / 2.0;
        let at = |u: f64, s: f64| wall.start + d * u + n * s;
        [
            at(self.start_u(1.0), t),
            at(self.end_u(wall, 1.0), t),
            at(self.end_u(wall, -1.0), -t),
            at(self.start_u(-1.0), -t),
        ]
    }

    /// The footprint in wall-local `(u, s)`, counter-clockwise, with the
    /// steps where the core (`core`, its span across the wall) runs on.
    /// Crossing walls' gaps are left out of it; see [`WallEnds::local_outlines`].
    pub fn local_outline(&self, wall: &Wall, core: (f64, f64)) -> Vec<DVec2> {
        self.piece_outline(wall, core, Bound::Start, Bound::End)
    }

    /// The footprint in wall-local `(u, s)` as one outline per piece
    /// between the gaps crossing walls leave, each like
    /// [`WallEnds::local_outline`].
    pub fn local_outlines(&self, wall: &Wall, core: (f64, f64)) -> Vec<Vec<DVec2>> {
        let mut bounds = vec![Bound::Start];
        for i in 0..self.gaps.len() {
            bounds.push(Bound::GapLo(i));
            bounds.push(Bound::GapHi(i));
        }
        bounds.push(Bound::End);
        bounds
            .chunks(2)
            .map(|b| self.piece_outline(wall, core, b[0], b[1]))
            .collect()
    }

    fn piece_outline(&self, wall: &Wall, core: (f64, f64), from: Bound, to: Bound) -> Vec<DVec2> {
        let t = wall.thickness / 2.0;
        let p = DVec2::new;
        let (c0, c1) = core;
        let (f, e) = (self.bound_core(from), self.bound_core(to));
        let at = |b: Bound, s: f64| self.bound_u(wall, b, s);
        // along the right face, up the far end, back along the left face
        // and down the start
        let mut out = vec![p(at(from, -t), -t), p(at(to, -t), -t)];
        if e > 1e-9 {
            out.extend([
                p(at(to, c0), c0),
                p(at(to, c0) + e, c0),
                p(at(to, c1) + e, c1),
                p(at(to, c1), c1),
            ]);
        }
        out.extend([p(at(to, t), t), p(at(from, t), t)]);
        if f > 1e-9 {
            out.extend([
                p(at(from, c1), c1),
                p(at(from, c1) - f, c1),
                p(at(from, c0) - f, c0),
                p(at(from, c0), c0),
            ]);
        }
        out.dedup_by(|a, b| a.distance(*b) < 1e-9);
        if out.len() > 1 && out[0].distance(out[out.len() - 1]) < 1e-9 {
            out.pop();
        }
        // On a curved wall the (u, w) outline is mapped round the arc
        // by place(); long edges need intermediate vertices or the arc
        // comes out faceted to its corners. Caps (Δu ≈ 0) are radial
        // segments and stay as they are. Straight walls skip this.
        if wall.is_arc() {
            let step = match wall.curve() {
                crate::sketch::Curve::Arc { radius, .. } => radius * crate::sketch::ARC_STEP,
                _ => f64::INFINITY,
            };
            let mut dense = Vec::with_capacity(out.len() * 2);
            for w in out.windows(2) {
                let (a, b) = (w[0], w[1]);
                dense.push(a);
                let n = ((b.x - a.x).abs() / step).floor() as usize;
                for i in 1..=n {
                    dense.push(a.lerp(b, i as f64 / (n + 1) as f64));
                }
            }
            if let Some(&last) = out.last() {
                dense.push(last);
            }
            out = dense;
        }
        out
    }
}

/// `s` across a wall as a fraction from its right face (0) to its left (1).
fn across(wall: &Wall, s: f64) -> f64 {
    if wall.thickness > 1e-12 {
        (s + wall.thickness / 2.0) / wall.thickness
    } else {
        0.5
    }
}

const TOL: f64 = 1e-3;

/// A wall with what its joins need to know: its core's span across it,
/// when it is layered.
struct Joinable<'a> {
    id: ElementId,
    wall: &'a Wall,
    core: Option<(f64, f64)>,
}

/// Where the centrelines through `p` along `a` and through `q` along `b` cross.
fn crossing(p: DVec2, a: DVec2, q: DVec2, b: DVec2) -> Option<DVec2> {
    meet(p, a, q, b).map(|s| p + a * s)
}

/// Do the ends `p` (of a wall arriving along `d_in`) and `q` (of a wall
/// leaving along `d_out`) meet in an L? They do when they coincide, or
/// when both lie within `reach` of where the centrelines cross.
fn ends_meet(p: DVec2, d_in: DVec2, q: DVec2, d_out: DVec2, reach: f64) -> bool {
    if p.distance(q) < TOL {
        return true;
    }
    if d_in.perp_dot(d_out).abs() < 0.05 {
        return false; // (nearly) parallel: no corner to meet at
    }
    crossing(p, d_in, q, d_out).is_some_and(|x| x.distance(p) <= reach && x.distance(q) <= reach)
}

/// The other wall ends that meet `p`, the end of a wall arriving along
/// `d_in`: (wall, its direction leaving the joint). Ends that coincide
/// win over ends that are only near.
fn meeting<'a>(
    walls: &'a [Joinable<'a>],
    me: &Joinable,
    p: DVec2,
    d_in: DVec2,
) -> Vec<(&'a Joinable<'a>, DVec2)> {
    let w = me.wall;
    let mut exact = vec![];
    let mut near = vec![];
    for o in walls
        .iter()
        .filter(|o| o.id != me.id && o.wall.level == w.level)
    {
        for (q, d_out, disallowed) in [
            (o.wall.start, o.wall.direction(), o.wall.disallow_join[0]),
            (o.wall.end, -o.wall.direction(), o.wall.disallow_join[1]),
        ] {
            if disallowed {
                continue;
            }
            let reach = w.thickness.max(o.wall.thickness);
            if q.distance(p) < TOL {
                exact.push((o, d_out));
            } else if ends_meet(p, d_in, q, d_out, reach) {
                near.push((o, d_out));
            }
        }
    }
    if exact.is_empty() {
        near
    } else {
        exact
    }
}

/// Whether a wall in line with `o` (parallel, on the same line) has an end
/// near `p` on the far side of `me`'s line from `o`: `o` and it are one
/// run that `me` meets at a T.
fn continues_past(walls: &[Joinable], me: &Joinable, o: &Joinable, p: DVec2, d_in: DVec2) -> bool {
    let dir = o.wall.direction();
    let n = d_in.perp();
    let reach = me.wall.thickness.max(o.wall.thickness) * 1.5;
    let o_side = {
        let q = if o.wall.start.distance(p) <= o.wall.end.distance(p) {
            o.wall.start
        } else {
            o.wall.end
        };
        (q - p).dot(n).signum()
    };
    walls.iter().any(|o2| {
        if o2.id == me.id || o2.id == o.id || o2.wall.level != me.wall.level {
            return false;
        }
        let in_line = dir.perp_dot(o2.wall.direction()).abs() < 0.02
            && (o2.wall.start - o.wall.start).dot(dir.perp()).abs() < 0.05;
        in_line
            && [o2.wall.start, o2.wall.end]
                .iter()
                .any(|q2| q2.distance(p) < reach && (*q2 - p).dot(n).signum() != o_side)
    })
}

/// The walls of the model that can join, with their cores.
fn joinables(model: &Model) -> Vec<Joinable<'_>> {
    model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Wall(w) if w.length() > TOL => Some(Joinable {
                id: e.id,
                wall: w,
                core: assembly::assembly_of(model, e).map(|(_, a)| assembly::wall_core(a, w)),
            }),
            _ => None,
        })
        .collect()
}

/// Join every wall in the model with its neighbours on the same level.
pub fn wall_joins(model: &Model) -> HashMap<ElementId, WallEnds> {
    joins_among(&joinables(model), &rules(model))
}

/// Join walls that are not in a model (all without layers), as walls of
/// the model would join: what strip footings use to meet each other the
/// way the walls over them do.
pub fn plain_joins(walls: &[(ElementId, Wall)]) -> HashMap<ElementId, WallEnds> {
    let walls: Vec<Joinable> = walls
        .iter()
        .filter(|(_, w)| w.length() > TOL)
        .map(|(id, w)| Joinable {
            id: *id,
            wall: w,
            core: None,
        })
        .collect();
    joins_among(&walls, &[])
}

/// Joins among `walls`, curved ones too: each end of a curved wall stands
/// in as a short straight wall along its tangent there, the stand-ins join
/// as straight walls do (mitres, butts, laps), and the curved wall takes
/// its start from one and its end from the other, applied along those
/// tangents (see [`Wall::place_end`]).
fn joins_among(walls: &[Joinable], rules: &[JoinRule]) -> HashMap<ElementId, WallEnds> {
    if !walls.iter().any(|j| j.wall.is_arc()) {
        return joins_straight(walls, rules);
    }
    // (arc, start stand-in, end stand-in, stand-in length)
    let mut arcs: Vec<(ElementId, ElementId, ElementId, f64)> = vec![];
    let mut owned: Vec<StandIn> = vec![];
    for j in walls.iter().filter(|j| j.wall.is_arc()) {
        let w = j.wall;
        let len = w.length();
        // long enough to read a corner by, short enough not to cross
        // walls the arc itself doesn't
        let l = (len / 3.0).min(w.thickness * 2.0 + 0.3).max(1e-3);
        let (t0, t1) = (w.tangent_at(0.0), w.tangent_at(len));
        let (s, e) = (ElementId::new(), ElementId::new());
        let mut a = w.clone();
        a.bulge = 0.0;
        a.end = w.start + t0 * l;
        a.disallow_join = [w.disallow_join[0], true];
        let mut b = w.clone();
        b.bulge = 0.0;
        b.start = w.end - t1 * l;
        b.disallow_join = [true, w.disallow_join[1]];
        owned.push((s, a, j.core));
        owned.push((e, b, j.core));
        arcs.push((j.id, s, e, l));
    }
    let mut all: Vec<Joinable> = walls
        .iter()
        .filter(|j| !j.wall.is_arc())
        .map(|j| Joinable {
            id: j.id,
            wall: j.wall,
            core: j.core,
        })
        .collect();
    all.extend(owned.iter().map(|(id, w, core)| Joinable {
        id: *id,
        wall: w,
        core: *core,
    }));
    // a rule set for an arc holds for its stand-ins
    let mut rules2: Vec<JoinRule> = rules.to_vec();
    for r in rules {
        for (arc, s, e, _) in &arcs {
            for proxy in [*s, *e] {
                let mut r2 = *r;
                if r2.a == *arc {
                    r2.a = proxy;
                } else if r2.b == *arc {
                    r2.b = proxy;
                } else {
                    continue;
                }
                rules2.push(r2);
            }
        }
    }
    let mut out = joins_straight(&all, &rules2);
    for (arc, s, e, l) in arcs {
        let a = out.remove(&s).unwrap_or_default();
        let b = out.remove(&e).unwrap_or_default();
        let len = walls
            .iter()
            .find(|j| j.id == arc)
            .map_or(0.0, |j| j.wall.length());
        out.insert(
            arc,
            WallEnds {
                start: a.start,
                end: b.end,
                core: [a.core[0], b.core[1]],
                // the end stand-in measures from l before the arc's end
                core_line: [
                    a.core_line[0],
                    b.core_line[1].map(|(u, k)| (u - l + len, k)),
                ],
                gaps: vec![],
            },
        );
    }
    out
}

/// A curved wall's end standing in as a straight wall: id, wall, core.
type StandIn = (ElementId, Wall, Option<(f64, f64)>);

fn joins_straight(walls: &[Joinable], rules: &[JoinRule]) -> HashMap<ElementId, WallEnds> {
    let mut out = HashMap::new();
    for (i, me) in walls.iter().enumerate() {
        let w = me.wall;
        let mut ends = WallEnds::default();
        // walls running through this one, the thicker (or first drawn) winning
        for (j, o) in walls.iter().enumerate() {
            if o.id == me.id || o.wall.level != w.level {
                continue;
            }
            let wins = o.wall.thickness > w.thickness + 1e-9
                || ((o.wall.thickness - w.thickness).abs() <= 1e-9 && j < i);
            if wins {
                ends.gaps.extend(crossing_gap(me, o));
            }
        }
        ends.gaps.sort_by(|a, b| a.lo_mid().total_cmp(&b.lo_mid()));
        // gaps that overlap (walls crossing side by side) become one
        ends.gaps.dedup_by(|b, a| {
            if b.lo_mid() <= a.hi_mid() {
                a.hi = [a.hi[0].max(b.hi[0]), a.hi[1].max(b.hi[1])];
                a.core[1] = b.core[1];
                true
            } else {
                false
            }
        });
        let mut lapped = [None; 2];
        for at_end in [false, true] {
            if w.disallow_join[usize::from(at_end)] {
                continue; // a square end, butting into what it meets
            }
            let (p, other) = if at_end {
                (w.end, w.start)
            } else {
                (w.start, w.end)
            };
            // direction arriving at the joint, and the wall's own left side
            // expressed relative to that direction
            let d_in = (p - other).normalize();
            let local_left = if at_end { 1.0 } else { -1.0 };
            let t = w.thickness / 2.0;

            let neighbours = meeting(walls, me, p, d_in);
            // extensions of the faces left and right of d_in, and of the core
            let ext = match neighbours.as_slice() {
                [(o, d_out)] => {
                    let q = if o.wall.start.distance(p) <= o.wall.end.distance(p) {
                        o.wall.start
                    } else {
                        o.wall.end
                    };
                    let t_o = o.wall.thickness / 2.0;
                    let rule = rules.iter().find(|r| r.joins(me.id, o.id));
                    // a wall in line with `o` carrying on past this end on
                    // the other side: the two are one run this wall meets
                    // at a T, so it stops at their face rather than mitre
                    // against one of them (MU23: a slanted end, artifacts)
                    let tee_run = rule.is_none() && continues_past(walls, me, o, p, d_in);
                    if tee_run {
                        let ext = butt(p, d_in, t, q, *d_out, t_o, false).map(|(l, r)| (l, r, 0.0));
                        if let Some((left, right, core)) = ext {
                            let (l, r) = if local_left > 0.0 {
                                (left, right)
                            } else {
                                (right, left)
                            };
                            if at_end {
                                ends.end = [l, r];
                                ends.core[1] = core;
                            } else {
                                ends.start = [l, r];
                                ends.core[0] = core;
                            }
                        }
                        continue;
                    }
                    match rule.map(|r| (r.cleanup, r.wins(me.id, o.id), r.corner)) {
                        // not joined: a square end, the two overlapping
                        Some((false, _, _)) => None,
                        // one runs through, every layer of the other stops at it
                        Some((_, Some(through), _)) => {
                            butt(p, d_in, t, q, *d_out, t_o, through).map(|(l, r)| (l, r, 0.0))
                        }
                        // the cores lap, the finishes meet on the mitre
                        Some((_, None, CornerJoin::Lap)) => {
                            let j = walls
                                .iter()
                                .position(|w| w.id == o.id)
                                .unwrap_or(usize::MAX);
                            let stronger = o.wall.thickness < w.thickness - 1e-9
                                || ((o.wall.thickness - w.thickness).abs() <= 1e-9 && i < j);
                            lapped[usize::from(at_end)] = lap(me, o, p, d_in, at_end, stronger);
                            miter(p, d_in, t, q, *d_out, t_o).map(|(l, r)| (l, r, 0.0))
                        }
                        // mitre about where the centrelines cross
                        _ => miter(p, d_in, t, q, *d_out, t_o).map(|(l, r)| (l, r, 0.0)),
                    }
                }
                [] => walls
                    .iter()
                    .filter(|o| o.id != me.id && o.wall.level == w.level)
                    .filter_map(|o| tee(p, d_in, me, o))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(e, _)| e),
                // three or more ends drawn to one point; ends only near
                // each other are left as they are
                _ if neighbours
                    .iter()
                    .all(|(o, _)| o.wall.start.distance(p).min(o.wall.end.distance(p)) < TOL) =>
                {
                    node(walls, me, p, d_in, &neighbours)
                }
                _ => None,
            };
            let Some((left, right, core)) = ext else {
                continue;
            };
            let (l, r) = if local_left > 0.0 {
                (left, right)
            } else {
                (right, left)
            };
            if at_end {
                ends.end = [l, r];
                ends.core[1] = core;
            } else {
                ends.start = [l, r];
                ends.core[0] = core;
            }
        }
        ends.core_line = lapped;
        out.insert(me.id, ends);
    }
    out
}

/// The end of a wall at a joint where three or more wall ends meet (see
/// the module notes): the face extensions and core run of the end of `me`
/// at `p`, arriving along `d_in`, given the other ends there.
fn node(
    walls: &[Joinable],
    me: &Joinable,
    p: DVec2,
    d_in: DVec2,
    neighbours: &[(&Joinable, DVec2)],
) -> Option<(f64, f64, f64)> {
    // every end at the joint, with its direction leaving it
    let mut ends: Vec<(&Joinable, DVec2)> = vec![(me, -d_in)];
    ends.extend(neighbours.iter().copied());
    let index = |j: &Joinable| {
        walls
            .iter()
            .position(|w| w.id == j.id)
            .unwrap_or(usize::MAX)
    };
    // the pair running on: most nearly straight, then thickest, then first
    // drawn, so every wall at the joint picks the same two
    let key = |i: usize, k: usize| {
        let (a, b) = (ends[i], ends[k]);
        let straight = -a.1.dot(b.1);
        let thick = a.0.wall.thickness + b.0.wall.thickness;
        let (x, y) = (index(a.0), index(b.0));
        (straight, thick, x.min(y), x.max(y))
    };
    let better = |a: (f64, f64, usize, usize), b: (f64, f64, usize, usize)| {
        if (a.0 - b.0).abs() > 1e-6 {
            return a.0 > b.0;
        }
        if (a.1 - b.1).abs() > 1e-9 {
            return a.1 > b.1;
        }
        (a.2, a.3) < (b.2, b.3)
    };
    let mut pair = (0, 1);
    for i in 0..ends.len() {
        for k in i + 1..ends.len() {
            if better(key(i, k), key(pair.0, pair.1)) {
                pair = (i, k);
            }
        }
    }
    // where a wall's end at the joint is: the one of its ends nearest `p`
    let end_at = |j: &Joinable| {
        if j.wall.start.distance(p) <= j.wall.end.distance(p) {
            j.wall.start
        } else {
            j.wall.end
        }
    };
    let t = me.wall.thickness / 2.0;
    if pair.0 == 0 {
        // one of the two running on: join the other as at an L
        let (o, d_out) = ends[pair.1];
        let q = end_at(o);
        return Some(
            miter(p, d_in, t, q, d_out, o.wall.thickness / 2.0)
                .map_or((0.0, 0.0, 0.0), |(l, r)| (l, r, 0.0)),
        );
    }
    let run = [ends[pair.0], ends[pair.1]];
    let n_in = d_in.perp();
    // the face of a wall running on that faces this one, across it from
    // its end at the joint, and that end
    let face_of = |k: &Joinable, dk: DVec2| {
        let c = end_at(k);
        let nk = dk.perp();
        let side = if (-d_in).dot(nk) > 0.0 { 1.0 } else { -1.0 };
        (c, nk, side, c + nk * (k.wall.thickness / 2.0 * side))
    };
    // how far the line `s` across this wall runs before it meets the
    // faces of the walls running on: the face it meets on that wall's own
    // side of the joint, else the nearer
    let reach = |s: f64| -> Option<(f64, usize)> {
        let mut hits = vec![];
        for (i, (k, dk)) in run.iter().enumerate() {
            let (c, _, _, face) = face_of(k, *dk);
            let Some(x) = meet(p + n_in * s, d_in, face, *dk) else {
                continue;
            };
            let at = p + n_in * s + d_in * x;
            hits.push((x, i, (at - c).dot(*dk) >= -TOL));
        }
        let on: Vec<_> = hits.iter().filter(|h| h.2).collect();
        let pick = if on.is_empty() {
            hits.iter().min_by(|a, b| a.0.total_cmp(&b.0))
        } else {
            on.into_iter().min_by(|a, b| a.0.total_cmp(&b.0))
        };
        pick.map(|h| (h.0, h.1))
    };
    let (left, _) = reach(t)?;
    let (right, _) = reach(-t)?;
    let limit = 4.0
        * run
            .iter()
            .map(|r| r.0.wall.thickness)
            .fold(t * 2.0, f64::max);
    if left.abs() > limit || right.abs() > limit {
        return None;
    }
    // layered into layered: the core runs on to the core of the wall its
    // centreline meets
    let core = match (me.core, reach(0.0)) {
        (Some(_), Some((f, i))) => {
            let (k, dk) = run[i];
            k.core.map_or(0.0, |(c0, c1)| {
                let (c, nk, side, _) = face_of(k, dk);
                // the core's span in the frame leaving the joint
                let (lo, hi) = if dk.dot(k.wall.direction()) > 0.0 {
                    (c0, c1)
                } else {
                    (-c1, -c0)
                };
                let s = if side > 0.0 { hi } else { lo };
                meet(p, d_in, c + nk * s, dk).map_or(0.0, |x| (x - f).max(0.0))
            })
        }
        _ => 0.0,
    };
    Some((left, right, core))
}

/// Distance along `a` from `p` to where the line `p + a s` meets `q + b r`.
fn meet(p: DVec2, a: DVec2, q: DVec2, b: DVec2) -> Option<f64> {
    let den = a.perp_dot(b);
    (den.abs() > 1e-9).then(|| (q - p).perp_dot(b) / den)
}

/// Mitre a wall arriving at `p` along `d_in` with one leaving `q` along
/// `d_out`: how far each face of the arriving wall runs past `p`.
fn miter(
    p: DVec2,
    d_in: DVec2,
    t: f64,
    q: DVec2,
    d_out: DVec2,
    t_other: f64,
) -> Option<(f64, f64)> {
    if d_in.perp_dot(d_out).abs() < 1e-6 {
        return None; // straight run: square ends already meet
    }
    let (n_in, n_out) = (d_in.perp(), d_out.perp());
    let left = meet(p + n_in * t, d_in, q + n_out * t_other, d_out)?;
    let right = meet(p - n_in * t, d_in, q - n_out * t_other, d_out)?;
    let limit = 4.0 * t.max(t_other) + p.distance(q);
    (left.abs() < limit && right.abs() < limit).then_some((left, right))
}

/// Butt a wall arriving at `p` along `d_in` against one leaving `q` along
/// `d_out`: how far each face runs past `p`, to the other's far face when
/// it runs `through`, else to its near face.
fn butt(
    p: DVec2,
    d_in: DVec2,
    t: f64,
    q: DVec2,
    d_out: DVec2,
    t_other: f64,
    through: bool,
) -> Option<(f64, f64)> {
    if d_in.perp_dot(d_out).abs() < 1e-6 {
        return None;
    }
    let (n_in, n_out) = (d_in.perp(), d_out.perp());
    // the side of the other wall this one comes from
    let side = if (-d_in).dot(n_out) > 0.0 { 1.0 } else { -1.0 };
    let face = q + n_out * (t_other * if through { -side } else { side });
    let left = meet(p + n_in * t, d_in, face, d_out)?;
    let right = meet(p - n_in * t, d_in, face, d_out)?;
    let limit = 4.0 * t.max(t_other) + p.distance(q);
    (left.abs() < limit && right.abs() < limit).then_some((left, right))
}

/// Where the core of `me` ends at its start or end (`at_end`) where its
/// core laps with `o`'s: the stronger on the far face of the other's core,
/// the other on its near face, as a line `u = a + b·s` along `me`. `None`
/// unless both are layered.
fn lap(
    me: &Joinable,
    o: &Joinable,
    p: DVec2,
    d_in: DVec2,
    at_end: bool,
    stronger: bool,
) -> Option<(f64, f64)> {
    let (Some((m0, m1)), Some((c0, c1))) = (me.core, o.core) else {
        return None;
    };
    let w = me.wall;
    let n = w.direction().perp();
    let (od, on) = (o.wall.direction(), o.wall.direction().perp());
    // how far along d_in from p the line `s` across me meets the face of
    // o's core at offset `c`
    let reach = |s: f64, c: f64| meet(p + n * s, d_in, o.wall.start + on * c, od);
    let mid = (m0 + m1) / 2.0;
    let (x0, x1) = (reach(mid, c0)?, reach(mid, c1)?);
    let face = if (x0 > x1) == stronger { c0 } else { c1 };
    let u = |s: f64| reach(s, face).map(|x| if at_end { w.length() + x } else { -x });
    let (u0, u1) = (u(m0)?, u(m1)?);
    if (m1 - m0).abs() < 1e-9 {
        return None;
    }
    let b = (u1 - u0) / (m1 - m0);
    Some((u0 - b * m0, b))
}

/// Trim a wall that ends at `p` on the body of wall `o` back to `o`'s
/// near face: the face extensions and the core's further run, and how far
/// `p` is from `o`'s centreline (the nearest host wins).
fn tee(p: DVec2, d_in: DVec2, me: &Joinable, o: &Joinable) -> Option<((f64, f64, f64), f64)> {
    let (w, ow) = (me.wall, o.wall);
    let t = w.thickness / 2.0;
    let d = ow.direction();
    let u = (p - ow.start).dot(d);
    let off = (p - ow.start).perp_dot(d).abs();
    // an end short of the face, or through it, by up to its own thickness still joins
    if u <= TOL || u >= ow.length() - TOL || off > ow.thickness / 2.0 + w.thickness + TOL {
        return None;
    }
    if d_in.perp_dot(d).abs() < 1e-6 {
        return None;
    }
    let n = d.perp();
    // the face on the side the joining wall comes from
    let side = if (-d_in).dot(n) > 0.0 { 1.0 } else { -1.0 };
    let face = ow.start + n * (ow.thickness / 2.0 * side);
    let n_in = d_in.perp();
    let left = meet(p + n_in * t, d_in, face, d)?;
    let right = meet(p - n_in * t, d_in, face, d)?;
    // layered into layered: the core runs on to the other's core
    let core = match (me.core, o.core) {
        (Some(_), Some((c0, c1))) => {
            let s = if side > 0.0 { c1 } else { c0 };
            let core_face = ow.start + n * s;
            meet(p, d_in, core_face, d)
                .zip(meet(p, d_in, face, d))
                .map_or(0.0, |(c, f)| (c - f).max(0.0))
        }
        _ => 0.0,
    };
    Some(((left, right, core), off))
}

/// The gap wall `o` leaves running through `me`, when the two cross: each
/// passes right through the other, both its ends well clear of the other
/// (an end closer than that makes a T or an L instead).
fn crossing_gap(me: &Joinable, o: &Joinable) -> Option<Gap> {
    let (w, ow) = (me.wall, o.wall);
    let (d, od) = (w.direction(), ow.direction());
    if d.perp_dot(od).abs() < 0.05 {
        return None;
    }
    // both ends of `a` on either side of `b`, further off than a T reaches
    let clear = |a: &Wall, b: &Wall| {
        let off = |p: DVec2| (p - b.start).perp_dot(b.direction());
        let (s0, s1) = (off(a.start), off(a.end));
        let reach = b.thickness / 2.0 + a.thickness + TOL;
        s0 * s1 < 0.0 && s0.abs() > reach && s1.abs() > reach
    };
    if !clear(w, ow) || !clear(ow, w) {
        return None;
    }
    let (n, on) = (d.perp(), od.perp());
    let (t, ot) = (w.thickness / 2.0, ow.thickness / 2.0);
    // where a line `s` across this wall meets one `so` across the other
    let u = |s: f64, so: f64| meet(w.start + n * s, d, ow.start + on * so, od);
    let mut lo = [0.0; 2];
    let mut hi = [0.0; 2];
    for (k, s) in [t, -t].into_iter().enumerate() {
        let (a, b) = (u(s, ot)?, u(s, -ot)?);
        lo[k] = a.min(b);
        hi[k] = a.max(b);
    }
    let len = w.length();
    if lo.iter().chain(&hi).any(|v| *v <= TOL || *v >= len - TOL) {
        return None;
    }
    // layered through layered: the core runs on to the other's core
    let core = match (me.core, o.core) {
        (Some(_), Some((c0, c1))) => {
            let (f0, f1) = (u(0.0, ot)?, u(0.0, -ot)?);
            let (k0, k1) = (u(0.0, c0)?, u(0.0, c1)?);
            [
                (k0.min(k1) - f0.min(f1)).max(0.0),
                (f0.max(f1) - k0.max(k1)).max(0.0),
            ]
        }
        _ => [0.0; 2],
    };
    Some(Gap { lo, hi, core })
}

// ---------------------------------------------------------------- following

/// The other walls that follow when `changed` replace their originals in
/// `before` (moved, turned, stretched): ends that met a moved wall's end
/// go with it, ends on a moved wall's body slide along their own wall to
/// stay on it, and a wall moved bodily keeps its ends on the walls they
/// sat on. Doors and windows in a wall whose start moves stay where they
/// were. Changes `changed` in place and returns the other elements that
/// change, as (before, after).
pub fn follow(before: &Model, changed: &mut [Element]) -> Vec<(Element, Element)> {
    let moving: HashSet<ElementId> = changed.iter().map(|e| e.id).collect();
    let walls = joinables(before);
    let mut out: HashMap<ElementId, Element> = HashMap::new();
    let mut order: Vec<ElementId> = vec![];
    for c in changed.iter_mut() {
        let (Some(old), ElementKind::Wall(nw)) = (before.get(c.id), &mut c.kind) else {
            continue;
        };
        let ElementKind::Wall(ow) = &old.kind else {
            continue;
        };
        let Some(me) = walls.iter().find(|j| j.id == old.id) else {
            continue;
        };
        let rigid = (nw.start - ow.start).distance(nw.end - ow.end) < 1e-9;
        for at_end in [false, true] {
            let (p_old, other_old) = if at_end {
                (ow.end, ow.start)
            } else {
                (ow.start, ow.end)
            };
            let p_new = if at_end { nw.end } else { nw.start };
            let d_in = (p_old - other_old).normalize_or_zero();
            // an end whose join is disallowed takes nothing along
            if ow.disallow_join[usize::from(at_end)] {
                continue;
            }
            // what this end was joined to, before
            let corner = meeting(&walls, me, p_old, d_in);
            if p_old.distance(p_new) > 1e-9 {
                for (o, d_out) in &corner {
                    // which of its ends met this one
                    let (wall, oe) = (o.id, *d_out == -o.wall.direction());
                    if moving.contains(&wall) {
                        continue;
                    }
                    // a wall lying exactly on this one (its copy, or a
                    // copy left behind) is not joined to it (MU25)
                    let on_me = (o.wall.start.distance(ow.start) < 1e-6
                        && o.wall.end.distance(ow.end) < 1e-6)
                        || (o.wall.start.distance(ow.end) < 1e-6
                            && o.wall.end.distance(ow.start) < 1e-6);
                    if on_me {
                        continue;
                    }
                    let delta = p_new - p_old;
                    let e = out
                        .entry(wall)
                        .or_insert_with(|| before.get(wall).cloned().expect("a joined wall"));
                    if !order.contains(&wall) {
                        order.push(wall);
                    }
                    if let ElementKind::Wall(o) = &mut e.kind {
                        if oe {
                            o.end += delta;
                        } else {
                            o.start += delta;
                        }
                    }
                }
            }
            // a wall moved bodily keeps a T end on the wall it sat on
            if rigid && corner.is_empty() && p_old.distance(p_new) > 1e-9 {
                let host = walls
                    .iter()
                    .filter(|o| o.id != me.id && o.wall.level == ow.level)
                    .filter_map(|o| tee(p_old, d_in, me, o).map(|(_, off)| (o, off)))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(o, _)| o);
                if let Some(host) = host.filter(|h| !moving.contains(&h.id)) {
                    let d_new =
                        (p_new - if at_end { nw.start } else { nw.end }).normalize_or_zero();
                    if let Some(q) = slide_onto(p_old, p_new, d_new, host.wall, host.wall) {
                        if at_end {
                            nw.end = q;
                        } else {
                            nw.start = q;
                        }
                    }
                }
            }
        }
        // ends of other walls on this one's body slide along to stay on it
        for o in walls
            .iter()
            .filter(|o| o.id != me.id && o.wall.level == ow.level)
        {
            if moving.contains(&o.id) {
                continue;
            }
            for oe in [false, true] {
                if o.wall.disallow_join[usize::from(oe)] {
                    continue;
                }
                let (q, q_other) = if oe {
                    (o.wall.end, o.wall.start)
                } else {
                    (o.wall.start, o.wall.end)
                };
                let d_q = (q - q_other).normalize_or_zero();
                if !meeting(&walls, o, q, d_q).is_empty() || tee(q, d_q, o, me).is_none() {
                    continue;
                }
                // the nearest host must be this wall
                let nearest = walls
                    .iter()
                    .filter(|h| h.id != o.id && h.wall.level == o.wall.level)
                    .filter_map(|h| tee(q, d_q, o, h).map(|(_, off)| (h.id, off)))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(h, _)| h);
                if nearest != Some(me.id) {
                    continue;
                }
                let current = out
                    .get(&o.id)
                    .and_then(|e| match &e.kind {
                        ElementKind::Wall(w) => Some(if oe { w.end } else { w.start }),
                        _ => None,
                    })
                    .unwrap_or(q);
                let Some(nq) = slide_onto(q, current, d_q, ow, nw) else {
                    continue;
                };
                let e = out
                    .entry(o.id)
                    .or_insert_with(|| before.get(o.id).cloned().expect("a joined wall"));
                if !order.contains(&o.id) {
                    order.push(o.id);
                }
                if let ElementKind::Wall(w) = &mut e.kind {
                    if oe {
                        w.end = nq;
                    } else {
                        w.start = nq;
                    }
                }
            }
        }
    }
    let mut result = vec![];
    for id in order {
        let (Some(b), Some(mut a)) = (before.get(id), out.remove(&id)) else {
            continue;
        };
        if let ElementKind::Wall(w) = &a.kind {
            if w.length() < 0.05 || a == *b {
                continue;
            }
        }
        // its doors and windows stay where they were along it
        if let (ElementKind::Wall(ob), ElementKind::Wall(oa)) = (&b.kind, &mut a.kind) {
            let shift = (oa.start - ob.start).dot(oa.direction());
            if shift.abs() > 1e-9 {
                for d in before.dependents(id) {
                    let Some(de) = before.get(d) else { continue };
                    let ElementKind::Opening(op) = &de.kind else {
                        continue;
                    };
                    let mut na = de.clone();
                    if let ElementKind::Opening(nop) = &mut na.kind {
                        let max = (oa.length() - op.width).max(0.0);
                        nop.offset = (op.offset - shift).clamp(0.0, max);
                    }
                    if na != *de {
                        result.push((de.clone(), na));
                    }
                }
            }
        }
        result.insert(0, (b.clone(), a));
    }
    result
}

/// An end that sat at `p_old`, `off` across `host_old`, slid along its own
/// line (through `p_now` along `dir`) to sit as far across `host_new`.
/// `None` when the line runs along the host or leaves its body.
fn slide_onto(
    p_old: DVec2,
    p_now: DVec2,
    dir: DVec2,
    host_old: &Wall,
    host_new: &Wall,
) -> Option<DVec2> {
    let off = (p_old - host_old.start).perp_dot(host_old.direction());
    let d = host_new.direction();
    // the line along the new host, `off` across it
    let line = host_new.start + d.perp() * -off;
    let s = meet(p_now, dir, line, d)?;
    let q = p_now + dir * s;
    let u = (q - host_new.start).dot(d);
    (u > TOL && u < host_new.length() - TOL && s.abs() < 50.0).then_some(q)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, Level, Transaction};

    fn model(walls: &[(DVec2, DVec2)]) -> (Model, Vec<ElementId>) {
        let mut doc = crate::Document::new();
        let level = Element::new(
            "L0",
            ElementKind::Level(Level {
                elevation: 0.0,
                height: 3.0,
            }),
        );
        let lid = level.id;
        let mut tx = Transaction::new("setup").insert(level);
        let mut ids = vec![];
        for (a, b) in walls {
            let e = Element::new("W", ElementKind::Wall(Wall::new(lid, *a, *b, 0.2, 3.0)));
            ids.push(e.id);
            tx = tx.insert(e);
        }
        doc.commit(tx).unwrap();
        (doc.model().clone(), ids)
    }

    #[test]
    fn a_curved_wall_mitres_with_the_straight_wall_at_its_corner() {
        // straight (0,0)-(5,0), then a quarter arc on to (8,3)
        let (mut m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(5.0, 0.0)),
            (DVec2::new(5.0, 0.0), DVec2::new(8.0, 3.0)),
        ]);
        let mut doc = crate::Document::from_model(m.clone());
        doc.edit(ids[1], "bend", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                // tangent at its start turned off the straight wall's line
                w.bulge = 0.25;
            }
        })
        .unwrap();
        m = doc.model().clone();
        let joins = wall_joins(&m);
        let (s, a) = (wall(&m, ids[0]), wall(&m, ids[1]));
        assert!(a.is_arc());
        let (js, ja) = (joins[&ids[0]].clone(), joins[&ids[1]].clone());
        // each face of the two meets at one corner point
        for side in [1.0, -1.0] {
            let t = 0.1 * side;
            let straight_end = s.place(js.end_u(&s, side), t);
            let arc_start = a.place_end(false, ja.start_u(side), t);
            assert!(
                straight_end.distance(arc_start) < 1e-6,
                "side {side}: {straight_end} vs {arc_start}"
            );
        }
    }

    fn wall(m: &Model, id: ElementId) -> Wall {
        match &m.get(id).unwrap().kind {
            ElementKind::Wall(w) => w.clone(),
            _ => unreachable!(),
        }
    }

    /// Add a rule for the junction of `a` and `b`, preset `p` from `a`.
    fn with_preset(m: &Model, a: ElementId, b: ElementId, p: CornerPreset) -> Model {
        let mut doc = crate::Document::from_model(m.clone());
        doc.commit(set_rule(doc.model(), a, b, "preset", |r| p.apply(r, a)))
            .unwrap();
        doc.model().clone()
    }

    #[test]
    fn corner_presets_butt_or_free_the_corner() {
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0)),
        ]);
        assert_eq!(corner_partner(&m, ids[0], true), Some(ids[1]));
        assert_eq!(corner_partner(&m, ids[0], false), None);
        // the first runs through: both its faces reach the other's far face
        let through = with_preset(&m, ids[0], ids[1], CornerPreset::ThisRunsThrough);
        let j = wall_joins(&through);
        assert!((j[&ids[0]].end[0] - 0.1).abs() < 1e-9 && (j[&ids[0]].end[1] - 0.1).abs() < 1e-9);
        // and the other stops at its near face
        assert!(
            (j[&ids[1]].start[0] + 0.1).abs() < 1e-9 && (j[&ids[1]].start[1] + 0.1).abs() < 1e-9
        );
        let rule = rule_for(&through, ids[0], ids[1]).unwrap().1;
        assert_eq!(
            CornerPreset::of(Some(&rule), ids[0]),
            CornerPreset::ThisRunsThrough
        );
        assert_eq!(
            CornerPreset::of(Some(&rule), ids[1]),
            CornerPreset::OtherRunsThrough
        );
        // seen from the other, the other way round
        let other = with_preset(&m, ids[1], ids[0], CornerPreset::OtherRunsThrough);
        assert_eq!(wall_joins(&other), j);
        // not joined: square ends at the corner point
        let free = with_preset(&m, ids[0], ids[1], CornerPreset::NotJoined);
        let j = wall_joins(&free);
        assert_eq!(j[&ids[0]].end, [0.0; 2]);
        assert_eq!(j[&ids[1]].start, [0.0; 2]);
        // and back to Auto leaves no rule
        let back = with_preset(&free, ids[0], ids[1], CornerPreset::Auto);
        assert!(rule_for(&back, ids[0], ids[1]).is_none());
        assert_eq!(wall_joins(&back), wall_joins(&m));
    }

    #[test]
    fn lapped_cores_run_one_on_and_stop_the_other() {
        let (mut m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0)),
        ]);
        // 50 finish, 100 core, 50 finish: the core from -0.05 to 0.05
        let a = Element::new(
            "A",
            ElementKind::Assembly(crate::Assembly {
                usage: Default::default(),
                layers: vec![
                    crate::Layer::new(LayerFunction::Finish, None, 0.05),
                    crate::Layer::new(LayerFunction::Structure, None, 0.1),
                    crate::Layer::new(LayerFunction::Finish, None, 0.05),
                ],
                description: String::new(),
            }),
        );
        let aid = a.id;
        let mut doc = crate::Document::from_model(m.clone());
        let mut tx = Transaction::new("layers").insert(a);
        for id in &ids {
            let before = doc.model().get(*id).unwrap().clone();
            let mut after = before.clone();
            if let ElementKind::Wall(w) = &mut after.kind {
                w.assembly = Some(aid);
            }
            tx = tx.replace(before, after);
        }
        doc.commit(tx).unwrap();
        m = doc.model().clone();
        let lap = with_preset(&m, ids[0], ids[1], CornerPreset::CoresLap);
        let (plain, j) = (wall_joins(&m), wall_joins(&lap));
        // the faces stay on the mitre
        assert_eq!(plain[&ids[0]].end, j[&ids[0]].end);
        // the first drawn runs its core to the far face of the other's
        // core (0.05 past the corner), the other stops at its near face
        let w0 = wall(&lap, ids[0]);
        let core0 = j[&ids[0]].layer_end(&w0, 0.0, true) - w0.length();
        assert!((core0 - 0.05).abs() < 1e-9, "{core0}");
        let w1 = wall(&lap, ids[1]);
        let core1 = j[&ids[1]].layer_start(&w1, 0.0, true);
        assert!((core1 - 0.05).abs() < 1e-9, "{core1}");
    }

    #[test]
    fn l_corner_is_mitred() {
        // counter-clockwise corner at (4, 0): left faces are inside
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0)),
        ]);
        let j = wall_joins(&m);
        let a = j[&ids[0]].clone();
        assert!((a.end[0] + 0.1).abs() < 1e-9, "inner face shortened");
        assert!((a.end[1] - 0.1).abs() < 1e-9, "outer face extended");
        // both walls' outer faces end at the same corner point
        let fa = a.footprint(&wall(&m, ids[0]));
        let fb = j[&ids[1]].footprint(&wall(&m, ids[1]));
        assert!(fa[2].distance(DVec2::new(4.1, -0.1)) < 1e-9);
        assert!(fb[3].distance(DVec2::new(4.1, -0.1)) < 1e-9);
    }

    #[test]
    fn tee_is_trimmed_to_the_face() {
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, 4.0), DVec2::new(3.0, 0.0)),
        ]);
        let j = wall_joins(&m);
        let b = j[&ids[1]].clone();
        assert!((b.end[0] + 0.1).abs() < 1e-9 && (b.end[1] + 0.1).abs() < 1e-9);
        assert_eq!(j[&ids[0]], WallEnds::default());
    }

    #[test]
    fn straight_runs_and_free_ends_are_untouched() {
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(3.0, 0.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(6.0, 0.0)),
        ]);
        let j = wall_joins(&m);
        assert_eq!(j[&ids[0]], WallEnds::default());
        assert_eq!(j[&ids[1]], WallEnds::default());
    }

    /// The outer corner both walls' footprints reach.
    fn corners_close(m: &Model, a: ElementId, b: ElementId) -> bool {
        let j = wall_joins(m);
        let fa = j[&a].footprint(&wall(m, a));
        let fb = j[&b].footprint(&wall(m, b));
        fa.iter()
            .filter(|p| fb.iter().any(|q| p.distance(*q) < 1e-6))
            .count()
            == 2
    }

    #[test]
    fn a_corner_a_notch_apart_still_mitres() {
        // the second wall moved 60 mm in: its end no longer meets the first's
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(3.94, 0.0), DVec2::new(3.94, 3.0)),
        ]);
        assert!(corners_close(&m, ids[0], ids[1]));
        // far apart, they don't
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(3.0, 0.5), DVec2::new(3.0, 3.0)),
        ]);
        assert!(!corners_close(&m, ids[0], ids[1]));
    }

    #[test]
    fn moving_a_wall_takes_its_corners_along() {
        // a U: the right wall moves 0.3 m left, the top and bottom follow
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(5.0, 0.0)),
            (DVec2::new(5.0, 0.0), DVec2::new(5.0, 4.0)),
            (DVec2::new(5.0, 4.0), DVec2::new(0.0, 4.0)),
        ]);
        let right = m.get(ids[1]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            right,
            &crate::transform::Xform::Move(glam::DVec3::new(-0.3, 0.0, 0.0)),
        )
        .unwrap()];
        let others = follow(&m, &mut moved);
        assert_eq!(others.len(), 2);
        let mut doc = crate::Document::from_model(m.clone());
        let mut tx = Transaction::new("Move").replace(right.clone(), moved[0].clone());
        for (b, a) in others {
            tx = tx.replace(b, a);
        }
        doc.commit(tx).unwrap();
        let m2 = doc.model();
        assert!(wall(m2, ids[0]).end.distance(DVec2::new(4.7, 0.0)) < 1e-9);
        assert!(wall(m2, ids[2]).start.distance(DVec2::new(4.7, 4.0)) < 1e-9);
        assert!(corners_close(m2, ids[0], ids[1]) && corners_close(m2, ids[1], ids[2]));
    }

    #[test]
    fn walls_on_a_moved_wall_slide_to_stay_on_it() {
        // a partition tees into a wall that moves up 0.5 m
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, -4.0), DVec2::new(3.0, 0.0)),
        ]);
        let host = m.get(ids[0]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            host,
            &crate::transform::Xform::Move(glam::DVec3::new(0.0, 0.5, 0.0)),
        )
        .unwrap()];
        let others = follow(&m, &mut moved);
        assert_eq!(others.len(), 1);
        let ElementKind::Wall(p) = &others[0].1.kind else {
            unreachable!()
        };
        assert!(p.end.distance(DVec2::new(3.0, 0.5)) < 1e-9);
        // and a partition moved along its length keeps both ends on its walls
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(0.0, 4.0), DVec2::new(6.0, 4.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(3.0, 4.0)),
        ]);
        let part = m.get(ids[2]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            part,
            &crate::transform::Xform::Move(glam::DVec3::new(1.0, 0.3, 0.0)),
        )
        .unwrap()];
        assert!(follow(&m, &mut moved).is_empty());
        let ElementKind::Wall(p) = &moved[0].kind else {
            unreachable!()
        };
        assert!(
            p.start.distance(DVec2::new(4.0, 0.0)) < 1e-9,
            "{:?}",
            p.start
        );
        assert!(p.end.distance(DVec2::new(4.0, 4.0)) < 1e-9, "{:?}", p.end);
    }

    fn set_wall(m: &Model, id: ElementId, f: impl FnOnce(&mut Wall)) -> Model {
        let mut doc = crate::Document::from_model(m.clone());
        doc.edit(id, "edit", |e| {
            if let ElementKind::Wall(w) = &mut e.kind {
                f(w)
            }
        })
        .unwrap();
        doc.model().clone()
    }

    #[test]
    fn crossing_walls_make_a_cross() {
        // the first wall runs through; the second stops at its faces
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, -2.0), DVec2::new(3.0, 2.0)),
        ]);
        let j = wall_joins(&m);
        assert!(j[&ids[0]].gaps.is_empty());
        let g = j[&ids[1]].gaps.clone();
        assert_eq!(g.len(), 1);
        // the second wall runs up from y = -2: the first's faces are at ±0.1
        assert!((g[0].lo_mid() - 1.9).abs() < 1e-9 && (g[0].hi_mid() - 2.1).abs() < 1e-9);
        let pieces = j[&ids[1]].local_outlines(&wall(&m, ids[1]), (-0.1, 0.1));
        assert_eq!(pieces.len(), 2);
        // the thicker wall wins whatever the order
        let m = set_wall(&m, ids[1], |w| w.thickness = 0.3);
        let j = wall_joins(&m);
        assert_eq!((j[&ids[0]].gaps.len(), j[&ids[1]].gaps.len()), (1, 0));
        // at an angle the gap is slanted, from face to face
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(1.0, -2.0), DVec2::new(5.0, 2.0)),
        ]);
        let j = wall_joins(&m);
        let g = j[&ids[1]].gaps[0];
        let w = wall(&m, ids[1]);
        let face = |u: f64, s: f64| w.start + w.direction() * u + w.direction().perp() * s;
        for (k, s) in [(0, 0.1), (1, -0.1)] {
            assert!((face(g.lo[k], s).y + 0.1).abs() < 1e-9);
            assert!((face(g.hi[k], s).y - 0.1).abs() < 1e-9);
        }
    }

    #[test]
    fn a_wall_ending_on_another_is_a_tee_not_a_cross() {
        // it stops just past the far face: still trimmed to the near face
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, -3.0), DVec2::new(3.0, 0.15)),
        ]);
        let j = wall_joins(&m);
        assert!(j[&ids[1]].gaps.is_empty() && j[&ids[0]].gaps.is_empty());
        assert!((j[&ids[1]].end[0] + 0.25).abs() < 1e-9);
    }

    #[test]
    fn three_walls_at_a_point_join_as_a_tee() {
        // a run of two walls ending on a partition's end: the run butts
        // square, the partition stops at its face
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(3.0, 0.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, 4.0), DVec2::new(3.0, 0.0)),
        ]);
        let j = wall_joins(&m);
        assert_eq!(j[&ids[0]], WallEnds::default());
        assert_eq!(j[&ids[1]], WallEnds::default());
        let b = &j[&ids[2]];
        assert!((b.end[0] + 0.1).abs() < 1e-9 && (b.end[1] + 0.1).abs() < 1e-9);
        // the same whichever was drawn first, and with the partition first
        let (m, ids) = model(&[
            (DVec2::new(3.0, 0.0), DVec2::new(3.0, 4.0)),
            (DVec2::new(6.0, 0.0), DVec2::new(3.0, 0.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(0.0, 0.0)),
        ]);
        let j = wall_joins(&m);
        let b = &j[&ids[0]];
        assert!((b.start[0] + 0.1).abs() < 1e-9 && (b.start[1] + 0.1).abs() < 1e-9);
        assert_eq!(j[&ids[1]], WallEnds::default());
        // no two footprints overlap
        let fp: Vec<[DVec2; 4]> = ids
            .iter()
            .map(|id| j[id].footprint(&wall(&m, *id)))
            .collect();
        assert!(fp[0].iter().all(|p| p.y >= 0.1 - 1e-9));
    }

    #[test]
    fn four_walls_at_a_point_make_a_cross() {
        let (m, ids) = model(&[
            (DVec2::new(0.0, 0.0), DVec2::new(3.0, 0.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(6.0, 0.0)),
            (DVec2::new(3.0, 3.0), DVec2::new(3.0, 0.0)),
            (DVec2::new(3.0, 0.0), DVec2::new(3.0, -3.0)),
        ]);
        let m = set_wall(&m, ids[0], |w| w.thickness = 0.3);
        let m = set_wall(&m, ids[1], |w| w.thickness = 0.3);
        let j = wall_joins(&m);
        // the thicker run goes through; the thinner ones stop at its faces
        assert_eq!(j[&ids[0]], WallEnds::default());
        assert!((j[&ids[2]].end[0] + 0.15).abs() < 1e-9);
        assert!((j[&ids[3]].start[1] + 0.15).abs() < 1e-9);
    }

    #[test]
    fn a_disallowed_join_leaves_the_end_square_and_free() {
        let (m, ids) = model(&[
            (DVec2::ZERO, DVec2::new(4.0, 0.0)),
            (DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0)),
        ]);
        let m = set_wall(&m, ids[0], |w| w.disallow_join = [false, true]);
        let j = wall_joins(&m);
        assert_eq!(j[&ids[0]], WallEnds::default());
        // the other wall no longer mitres with it either
        assert_eq!(j[&ids[1]].start, [0.0, 0.0]);
        // moving the first wall leaves the second where it is
        let first = m.get(ids[0]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            first,
            &crate::transform::Xform::Move(glam::DVec3::new(-0.5, 0.0, 0.0)),
        )
        .unwrap()];
        assert!(follow(&m, &mut moved).is_empty());
        // nor does moving the second take the first along
        let second = m.get(ids[1]).unwrap();
        let mut moved = vec![crate::transform::transformed(
            &m,
            second,
            &crate::transform::Xform::Move(glam::DVec3::new(0.5, 0.0, 0.0)),
        )
        .unwrap()];
        assert!(follow(&m, &mut moved).is_empty());
    }
}
