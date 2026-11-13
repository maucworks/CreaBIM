use crate::params::{
    ceiling_props, curtain_props, footing_props, room_props, type_props, Prop, CEILING_PSET,
    CURTAIN_PSET, FOOTING_PSET, ROOM_PSET, TYPE_PSET,
};
use crate::step::{list, point2, point3, real, string, Ref, StepWriter};
use creabim_core::{Element, ElementId, ElementKind, Model, OpeningKind};
use glam::{DVec2, DVec3};
use std::collections::HashMap;

pub struct ExportOptions {
    pub project_name: String,
    pub site_name: String,
    pub building_name: String,
    /// ISO 8601 timestamp for the file header.
    pub timestamp: String,
    /// How walls, slabs and roofs go out (MU20).
    pub body: IfcBody,
}

/// How walls, slabs and roofs are written (MU20). Everything else is the
/// same in all three.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IfcBody {
    /// Each one IfcWall / IfcSlab / IfcRoof with its layer set and its
    /// plain body: what every program reads and re-imports as such.
    #[default]
    Modelled,
    /// Their bodies cut where walls run through slabs, as CreaBIM draws
    /// and shows them.
    Joined,
    /// The structure stays the element; each other layer is an
    /// IfcCovering covering it (finishes, insulation, membranes).
    Layers,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            project_name: "CreaBIM Project".into(),
            site_name: "Site".into(),
            building_name: "Building".into(),
            timestamp: "2026-01-01T00:00:00".into(),
            body: IfcBody::Modelled,
        }
    }
}

impl ExportOptions {
    /// The names from the model's project information (A7-5), `fallback`
    /// (the file's name) for a project without one.
    pub fn for_model(model: &creabim_core::Model, fallback: &str) -> Self {
        let p = model
            .project_info()
            .map(|(_, p)| p.clone())
            .unwrap_or_default();
        let or = |v: &str, d: &str| {
            if v.trim().is_empty() {
                d.to_string()
            } else {
                v.trim().to_string()
            }
        };
        Self {
            project_name: or(&p.name, fallback),
            site_name: or(&p.site_name, "Site"),
            building_name: or(&p.building_name, "Building"),
            ..Default::default()
        }
    }
}

/// Marks the hatch in an exported material's description, so CreaBIM
/// gets it back on import.
pub(crate) const HATCH_PREFIX: &str = "CreaBIM hatch: ";

struct Ctx {
    w: StepWriter,
    body: Ref,
    /// Plan symbols of furniture and fixtures.
    footprint: Ref,
    axis: Ref,
    z_axis: Ref,
    x_axis: Ref,
    /// Each element's product, for voiding it later.
    products: HashMap<ElementId, Ref>,
    /// Each element's mark, written as its IFC Tag.
    marks: HashMap<ElementId, String>,
}

impl Ctx {
    /// An element's Tag attribute: its mark, or unset.
    fn tag(&self, id: ElementId) -> String {
        self.marks
            .get(&id)
            .map_or_else(|| "$".into(), |m| string(m))
    }

    fn guid(&self) -> String {
        string(&ElementId::new().ifc_guid())
    }

    fn dir(&mut self, d: DVec3) -> Ref {
        if d == DVec3::Z {
            return self.z_axis;
        }
        if d == DVec3::X {
            return self.x_axis;
        }
        self.w.add(
            "IFCDIRECTION",
            format!("({},{},{})", real(d.x), real(d.y), real(d.z)),
        )
    }

    fn placement3d(&mut self, at: DVec3, axis: Option<DVec3>, ref_dir: Option<DVec3>) -> Ref {
        let p = self.w.add("IFCCARTESIANPOINT", point3(at.x, at.y, at.z));
        let a = axis.map(|d| self.dir(d).to_string()).unwrap_or("$".into());
        let r = ref_dir
            .map(|d| self.dir(d).to_string())
            .unwrap_or("$".into());
        self.w.add("IFCAXIS2PLACEMENT3D", format!("{p},{a},{r}"))
    }

    fn local_placement(&mut self, rel: Option<Ref>, at: DVec3, ref_dir: Option<DVec3>) -> Ref {
        let axis = ref_dir.map(|_| DVec3::Z);
        let p = self.placement3d(at, axis, ref_dir);
        let rel = rel.map(|r| r.to_string()).unwrap_or("$".into());
        self.w.add("IFCLOCALPLACEMENT", format!("{rel},{p}"))
    }

    fn shape(&mut self, kind: &str, items: &[Ref]) -> Ref {
        let rep = self.body_rep(kind, items);
        self.w
            .add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,({rep})"))
    }

    fn body_rep(&mut self, kind: &str, items: &[Ref]) -> Ref {
        self.w.add(
            "IFCSHAPEREPRESENTATION",
            format!("{},'Body',{},{}", self.body, string(kind), list(items)),
        )
    }

    /// A property set of single values.
    fn pset(&mut self, name: &str, props: &[(&'static str, Prop)]) -> Ref {
        let refs: Vec<Ref> = props
            .iter()
            .map(|(k, v)| {
                self.w.add(
                    "IFCPROPERTYSINGLEVALUE",
                    format!("{},$,{},$", string(k), v.step()),
                )
            })
            .collect();
        let g = self.guid();
        self.w.add(
            "IFCPROPERTYSET",
            format!("{g},$,{},$,{}", string(name), list(&refs)),
        )
    }

    /// A property set, given to `objects`.
    fn pset_on(&mut self, objects: &[Ref], name: &str, props: &[(&'static str, Prop)]) {
        let set = self.pset(name, props);
        let g = self.guid();
        self.w.add(
            "IFCRELDEFINESBYPROPERTIES",
            format!("{g},$,$,$,{},{set}", list(objects)),
        );
    }

    /// A polygon in plan extruded `depth` up from height `z`.
    fn plan_extrusion(&mut self, pts: &[DVec2], z: f64, depth: f64) -> Ref {
        let mut refs: Vec<Ref> = pts
            .iter()
            .map(|p| self.w.add("IFCCARTESIANPOINT", point2(p.x, p.y)))
            .collect();
        refs.push(refs[0]);
        let poly = self.w.add("IFCPOLYLINE", list(&refs));
        let prof = self
            .w
            .add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{poly}"));
        let pos = self.placement3d(DVec3::new(0.0, 0.0, z), None, None);
        self.w.add(
            "IFCEXTRUDEDAREASOLID",
            format!("{prof},{pos},{},{}", self.z_axis, real(depth)),
        )
    }

    /// An IfcOpeningElement of shape `solid` (in `host_pl`'s frame) voiding `host`.
    fn void(&mut self, host: Ref, host_pl: Ref, solid: Ref, name: &str, guid: Option<String>) {
        let pl = self.local_placement(Some(host_pl), DVec3::ZERO, None);
        let shape = self.shape("SweptSolid", &[solid]);
        let guid = guid.unwrap_or_else(|| self.guid());
        let opening = self.w.add(
            "IFCOPENINGELEMENT",
            format!("{guid},$,{},$,$,{pl},{shape},$,.OPENING.", string(name)),
        );
        let g = self.guid();
        self.w
            .add("IFCRELVOIDSELEMENT", format!("{g},$,$,$,{host},{opening}"));
    }

    /// A rectangle `xdim` × `ydim` centred at `center` (profile plane),
    /// extruded `depth` along the solid's local Z.
    fn rect_extrusion(
        &mut self,
        center: DVec2,
        xdim: f64,
        ydim: f64,
        position: Ref,
        depth: f64,
    ) -> Ref {
        let c = self.w.add("IFCCARTESIANPOINT", point2(center.x, center.y));
        let p2 = self.w.add("IFCAXIS2PLACEMENT2D", format!("{c},$"));
        let prof = self.w.add(
            "IFCRECTANGLEPROFILEDEF",
            format!(".AREA.,$,{p2},{},{}", real(xdim), real(ydim)),
        );
        self.w.add(
            "IFCEXTRUDEDAREASOLID",
            format!("{prof},{position},{},{}", self.z_axis, real(depth)),
        )
    }
}

/// Export the model as an IFC4 file.
pub fn export_ifc4(model: &Model, opts: &ExportOptions) -> String {
    let mut w = StepWriter::default();
    let z_axis = w.add("IFCDIRECTION", "(0.,0.,1.)");
    let x_axis = w.add("IFCDIRECTION", "(1.,0.,0.)");
    let origin = w.add("IFCCARTESIANPOINT", "(0.,0.,0.)");
    let wcs = w.add("IFCAXIS2PLACEMENT3D", format!("{origin},$,$"));
    let model_ctx = w.add(
        "IFCGEOMETRICREPRESENTATIONCONTEXT",
        format!("$,'Model',3,1.E-05,{wcs},$"),
    );
    let body = w.add(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
        format!("'Body','Model',*,*,*,*,{model_ctx},$,.MODEL_VIEW.,$"),
    );
    let axis = w.add(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
        format!("'Axis','Model',*,*,*,*,{model_ctx},$,.GRAPH_VIEW.,$"),
    );
    let footprint = w.add(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
        format!("'FootPrint','Model',*,*,*,*,{model_ctx},$,.PLAN_VIEW.,$"),
    );
    let mut c = Ctx {
        w,
        body,
        footprint,
        axis,
        z_axis,
        x_axis,
        products: HashMap::new(),
        marks: element_marks(model),
    };

    let units = [
        c.w.add("IFCSIUNIT", "*,.LENGTHUNIT.,$,.METRE."),
        c.w.add("IFCSIUNIT", "*,.AREAUNIT.,$,.SQUARE_METRE."),
        c.w.add("IFCSIUNIT", "*,.VOLUMEUNIT.,$,.CUBIC_METRE."),
        c.w.add("IFCSIUNIT", "*,.PLANEANGLEUNIT.,$,.RADIAN."),
    ];
    let unit_assignment = c.w.add("IFCUNITASSIGNMENT", list(units));
    let project = c.w.add(
        "IFCPROJECT",
        format!(
            "{},$,{},$,$,$,$,({model_ctx}),{unit_assignment}",
            c.guid(),
            string(&opts.project_name)
        ),
    );

    let site_pl = c.local_placement(None, DVec3::ZERO, None);
    let site = c.w.add(
        "IFCSITE",
        format!(
            "{},$,{},$,$,{site_pl},$,$,.ELEMENT.,$,$,$,$,$",
            c.guid(),
            string(&opts.site_name)
        ),
    );
    let bldg_pl = c.local_placement(Some(site_pl), DVec3::ZERO, None);
    let building = c.w.add(
        "IFCBUILDING",
        format!(
            "{},$,{},$,$,{bldg_pl},$,$,.ELEMENT.,$,$,$",
            c.guid(),
            string(&opts.building_name)
        ),
    );

    // Storeys, one per level.
    let mut storeys: HashMap<ElementId, (Ref, Ref)> = HashMap::new();
    let mut storey_order = Vec::new();
    for (e, l) in model.levels() {
        let pl = c.local_placement(Some(bldg_pl), DVec3::new(0.0, 0.0, l.elevation), None);
        let s = c.w.add(
            "IFCBUILDINGSTOREY",
            format!(
                "{},$,{},$,$,{pl},$,$,.ELEMENT.,{}",
                string(&e.id.ifc_guid()),
                string(&e.name),
                real(l.elevation)
            ),
        );
        storeys.insert(e.id, (s, pl));
        storey_order.push(s);
    }

    let mut contained: HashMap<Option<ElementId>, Vec<Ref>> = HashMap::new();
    let mut wall_refs: HashMap<ElementId, (Ref, Ref)> = HashMap::new(); // entity, placement
                                                                        // how far each wall's axis (its location line) is from its centreline
    let mut wall_axes: HashMap<ElementId, f64> = HashMap::new();
    let joins = creabim_core::wall_joins(model);
    let mut by_material: HashMap<ElementId, Vec<Ref>> = HashMap::new();
    // layered elements, associated with their assembly's layer set instead
    let mut layered: Vec<(Ref, ElementId, LayerUse)> = Vec::new();
    // boolean cuts: cutters are construction aids and stay out; what they
    // cut is written as its final shape, tessellated
    let cuts = creabim_geom::scene::cuts(model);
    let cutters: std::collections::HashSet<ElementId> = cuts.iter().map(|c| c.1).collect();
    // screen walls too: their holes are written in their shape
    let screens: Vec<ElementId> = model
        .iter()
        .filter(|e| creabim_core::screen::screen_of(model, e).is_some())
        .map(|e| e.id)
        .collect();
    let mut cut_meshes: HashMap<ElementId, creabim_geom::mesh::TriMesh> =
        if cuts.is_empty() && screens.is_empty() {
            HashMap::new()
        } else {
            let targets: std::collections::HashSet<ElementId> =
                cuts.iter().map(|c| c.0).chain(screens).collect();
            let mut all = creabim_geom::scene::element_meshes(
                &creabim_geom::scene::build_scene_for_exchange(model),
            );
            all.retain(|id, _| targets.contains(id));
            all
        };
    // MU20: joined bodies, or only the structural core of a layered one,
    // written as their final shapes
    let joins_now = creabim_core::wall_joins(model);
    let joinable = |e: &Element| match &e.kind {
        ElementKind::Wall(w) => w.curtain.is_none(),
        ElementKind::Slab(_) | ElementKind::Roof(_) => true,
        _ => false,
    };
    match opts.body {
        IfcBody::Modelled => {}
        IfcBody::Joined => {
            let joined =
                creabim_geom::scene::element_meshes(&creabim_geom::scene::build_scene(model));
            for e in model.iter().filter(|e| joinable(e)) {
                if let Some(m) = joined.get(&e.id) {
                    cut_meshes.insert(e.id, m.clone());
                }
            }
        }
        IfcBody::Layers => {
            for e in model.iter().filter(|e| joinable(e)) {
                let parts = creabim_geom::scene::layer_parts(model, e, &joins_now);
                if parts.len() < 2 {
                    continue;
                }
                let mut core = creabim_geom::mesh::TriMesh::default();
                for (l, m) in &parts {
                    if l.core {
                        core.append(m);
                    }
                }
                if !core.indices.is_empty() {
                    cut_meshes.insert(e.id, core);
                }
            }
        }
    }
    let layers_mode = opts.body == IfcBody::Layers;
    let elevation =
        |l: Option<ElementId>| l.and_then(|l| model.level(l)).map_or(0.0, |l| l.elevation);
    // the products of each element type
    let mut typed: HashMap<ElementId, Vec<Ref>> = HashMap::new();
    // ceilings over rooms, to tie to their spaces
    let mut ceilings: Vec<(ElementId, Ref)> = vec![];
    let mut of_type = |e: &Element, r: Ref| {
        if let Some(t) = &e.of_type {
            typed.entry(t.id).or_default().push(r);
        }
    };

    for e in model.iter() {
        if cutters.contains(&e.id) {
            continue;
        }
        let cut = cut_meshes.get(&e.id);
        match &e.kind {
            ElementKind::Wall(
                wall @ creabim_core::Wall {
                    curtain: Some(grid),
                    ..
                },
            ) => {
                // a curtain wall: no body of its own, aggregating its
                // mullions (IfcMember) and panels (IfcPlate)
                let Some(&(_, spl)) = storeys.get(&wall.level) else {
                    continue;
                };
                let d = wall.direction();
                let pl = c.local_placement(
                    Some(spl),
                    DVec3::new(wall.start.x, wall.start.y, wall.base_offset),
                    Some(DVec3::new(d.x, d.y, 0.0)),
                );
                let r = c.w.add(
                    "IFCCURTAINWALL",
                    format!(
                        "{},$,{},$,$,{pl},$,$,.NOTDEFINED.",
                        string(&e.id.ifc_guid()),
                        string(&e.name)
                    ),
                );
                wall_refs.insert(e.id, (r, pl));
                wall_axes.insert(e.id, 0.0);
                contained.entry(Some(wall.level)).or_default().push(r);
                of_type(e, r);
                let openings: Vec<&creabim_core::Opening> = model
                    .dependents(e.id)
                    .into_iter()
                    .filter_map(|o| match &model.get(o)?.kind {
                        ElementKind::Opening(o) => Some(o),
                        _ => None,
                    })
                    .collect();
                let mut parts = vec![];
                let mut solid_parts = vec![];
                for b in creabim_geom::curtain::curtain_boxes(wall, grid, &openings) {
                    use creabim_geom::curtain::CurtainPart;
                    let size = b.hi - b.lo;
                    let ppl = c.local_placement(Some(pl), b.lo, None);
                    let pos = c.placement3d(DVec3::ZERO, None, None);
                    let solid =
                        c.rect_extrusion(size.truncate() / 2.0, size.x, size.y, pos, size.z);
                    let shape = c.shape("SweptSolid", &[solid]);
                    let (class, tail, what) = match b.part {
                        CurtainPart::Mullion => ("IFCMEMBER", ",$,.MULLION.", "mullion"),
                        CurtainPart::Glass => ("IFCPLATE", ",$,.CURTAIN_PANEL.", "glass"),
                        CurtainPart::Panel => ("IFCPLATE", ",$,.CURTAIN_PANEL.", "panel"),
                    };
                    let g = c.guid();
                    let p = c.w.add(
                        class,
                        format!(
                            "{g},$,{},$,$,{ppl},{shape}{tail}",
                            string(&format!("{} {what}", e.name))
                        ),
                    );
                    parts.push(p);
                    if b.part != CurtainPart::Glass {
                        solid_parts.push(p);
                    }
                }
                if !parts.is_empty() {
                    let g = c.guid();
                    c.w.add(
                        "IFCRELAGGREGATES",
                        format!("{g},$,$,$,{r},{}", list(&parts)),
                    );
                }
                if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().extend(solid_parts);
                }
                let mut props = curtain_props(grid);
                props.push(("Length", Prop::Length(wall.length())));
                props.push(("Height", Prop::Length(wall.height)));
                c.pset_on(&[r], CURTAIN_PSET, &props);
            }
            ElementKind::Wall(wall) => {
                let Some(&(_, spl)) = storeys.get(&wall.level) else {
                    continue;
                };
                let d = wall.direction();
                let n = d.perp();
                // the wall's axis is its location line: its placement sits
                // on it and the footprint is measured from it
                let loc = creabim_core::walls::location_offset(model, wall);
                let origin = wall.start + n * loc;
                let pl = c.local_placement(
                    Some(spl),
                    DVec3::new(origin.x, origin.y, wall.base_offset),
                    Some(DVec3::new(d.x, d.y, 0.0)),
                );
                wall_axes.insert(e.id, loc);
                // footprint in wall-local coordinates, following its joins,
                // one piece between each pair of walls crossing it
                let ends = joins.get(&e.id).cloned().unwrap_or_default();
                let t = wall.thickness / 2.0;
                let asm = creabim_core::assembly_of(model, e);
                // a layered wall's core may run on past its faces at a T
                let core = asm.map_or((-t, t), |(_, a)| creabim_core::assembly::wall_core(a, wall));
                // an attached top is cut by the undersides over it
                let top = creabim_core::walls::top_profile(model, wall);
                let z0 = elevation(Some(wall.level)) + wall.base_offset;
                let depth = top.as_ref().map_or(wall.height, |tp| {
                    let steps = 16;
                    (0..=steps)
                        .flat_map(|i| {
                            let p = wall.start.lerp(wall.end, i as f64 / steps as f64);
                            [p + n * t, p - n * t]
                        })
                        .filter_map(|p| tp.z_at(p))
                        .map(|z| z - z0)
                        .fold(wall.height, f64::max)
                });
                let mut solids = vec![];
                for local in ends.local_outlines(wall, core) {
                    // Through the wall's own mapping, not raw (u, w): on
                    // a curved wall u runs round the arc and the
                    // footprint must bend with it. For straight walls
                    // this is exactly (u, w - loc) as before.
                    let mut pts: Vec<Ref> = local
                        .iter()
                        .map(|p| {
                            let q = wall.place(p.x, p.y) - origin;
                            c.w.add("IFCCARTESIANPOINT", point2(q.dot(d), q.dot(n)))
                        })
                        .collect();
                    pts.push(pts[0]);
                    let poly = c.w.add("IFCPOLYLINE", list(&pts));
                    let prof =
                        c.w.add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{poly}"));
                    let pos = c.placement3d(DVec3::ZERO, None, None);
                    let mut solid = c.w.add(
                        "IFCEXTRUDEDAREASOLID",
                        format!("{prof},{pos},{},{}", c.z_axis, real(depth)),
                    );
                    if let Some(tp) = &top {
                        // wall-local: x along the wall from its axis start
                        let to_local = |p: DVec2| {
                            let q = p - origin;
                            DVec2::new(q.dot(d), q.dot(n))
                        };
                        for f in &tp.faces {
                            solid = clip_under(&mut c, solid, f, to_local, d, n, origin, z0);
                        }
                    }
                    solids.push(solid);
                }
                let clipped = top.is_some();
                // the axis lets other tools (and our importer) recover the
                // location line whatever the joins did to the ends
                let a0 = c.w.add("IFCCARTESIANPOINT", point2(0.0, 0.0));
                let a1 = c.w.add("IFCCARTESIANPOINT", point2(wall.length(), 0.0));
                let axis_line = c.w.add("IFCPOLYLINE", format!("({a0},{a1})"));
                let axis_rep = c.w.add(
                    "IFCSHAPEREPRESENTATION",
                    format!("{},'Axis','Curve2D',({axis_line})", c.axis),
                );
                let body_rep = match cut {
                    Some(m) => {
                        let fs = tessellate(&mut c, m, |p| {
                            let q = p.truncate() - origin;
                            DVec3::new(q.dot(d), q.dot(n), p.z - z0)
                        });
                        c.body_rep("Tessellation", &[fs])
                    }
                    None if clipped => c.body_rep("Clipping", &solids),
                    None => c.body_rep("SweptSolid", &solids),
                };
                let shape = c.w.add(
                    "IFCPRODUCTDEFINITIONSHAPE",
                    format!("$,$,({body_rep},{axis_rep})"),
                );
                let r = product(&mut c, "IFCWALL", e, pl, shape, ",$,.STANDARD.");
                of_type(e, r);
                wall_refs.insert(e.id, (r, pl));
                contained.entry(Some(wall.level)).or_default().push(r);
                if cut.is_none() || layers_mode {
                    let z = elevation(Some(wall.level));
                    write_layer_parts(
                        &mut c,
                        model,
                        e,
                        r,
                        spl,
                        z,
                        &joins,
                        &mut by_material,
                        layers_mode,
                    );
                }
                if let Some((a, _)) = asm {
                    // layers run across the wall from its exterior face,
                    // measured from the axis
                    let (sense, offset) = if wall.flip {
                        (false, t - loc)
                    } else {
                        (true, -t - loc)
                    };
                    layered.push((r, a.id, LayerUse::Usage("AXIS2", sense, offset)));
                } else if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
            }
            ElementKind::Slab(slab) => {
                let Some(&(_, spl)) = storeys.get(&slab.level) else {
                    continue;
                };
                let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
                // arcs go out in short straight pieces
                let mut pts: Vec<Ref> = slab
                    .outline()
                    .iter()
                    .map(|p| c.w.add("IFCCARTESIANPOINT", point2(p.x, p.y)))
                    .collect();
                pts.push(pts[0]);
                let poly = c.w.add("IFCPOLYLINE", list(&pts));
                let prof =
                    c.w.add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{poly}"));
                let pos = c.placement3d(
                    DVec3::new(0.0, 0.0, slab.top_offset - slab.thickness),
                    None,
                    None,
                );
                let solid = c.w.add(
                    "IFCEXTRUDEDAREASOLID",
                    format!("{prof},{pos},{},{}", c.z_axis, real(slab.thickness)),
                );
                let shape = match cut {
                    Some(m) => {
                        let z0 = elevation(Some(slab.level));
                        let fs = tessellate(&mut c, m, |p| p - DVec3::Z * z0);
                        c.shape("Tessellation", &[fs])
                    }
                    None => c.shape("SweptSolid", &[solid]),
                };
                let r = product(&mut c, "IFCSLAB", e, pl, shape, ",$,.FLOOR.");
                of_type(e, r);
                contained.entry(Some(slab.level)).or_default().push(r);
                if cut.is_none() || layers_mode {
                    let z = elevation(Some(slab.level));
                    write_layer_parts(
                        &mut c,
                        model,
                        e,
                        r,
                        spl,
                        z,
                        &joins,
                        &mut by_material,
                        layers_mode,
                    );
                }
                // holes, and shafts passing through, as openings voiding it
                for (i, h) in creabim_core::slab::holes_of(model, e).iter().enumerate() {
                    let z = slab.top_offset - slab.thickness - 0.05;
                    let solid = c.plan_extrusion(h, z, slab.thickness + 0.1);
                    c.void(r, pl, solid, &format!("{} hole {}", e.name, i + 1), None);
                }
                if let Some((a, _)) = creabim_core::assembly_of(model, e) {
                    // layers run down from the top
                    layered.push((r, a.id, LayerUse::Usage("AXIS3", false, slab.top_offset)));
                } else if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
            }
            ElementKind::Ceiling(ceiling) => {
                let (Some(&(_, spl)), Some(outline)) =
                    (storeys.get(&ceiling.level), ceiling.outline(model))
                else {
                    continue;
                };
                let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
                let solid = c.plan_extrusion(&outline, ceiling.height, ceiling.thickness);
                let shape = c.shape("SweptSolid", &[solid]);
                let r = product(&mut c, "IFCCOVERING", e, pl, shape, ",$,.CEILING.");
                contained.entry(Some(ceiling.level)).or_default().push(r);
                let room = ceiling.room.filter(|r| model.get(*r).is_some());
                let props = ceiling_props(ceiling, room.map(|r| r.ifc_guid()));
                c.pset_on(&[r], CEILING_PSET, &props);
                if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
                if let Some(room) = room {
                    ceilings.push((room, r));
                }
            }
            ElementKind::FreeMesh(f) => {
                let spl = f
                    .level
                    .and_then(|l| storeys.get(&l))
                    .map(|s| s.1)
                    .unwrap_or(bldg_pl);
                let pl = c.local_placement(Some(spl), f.origin, None);
                // the shape as shown, with its modifiers applied
                let mesh = f.evaluated().mesh;
                let coords: Vec<String> = mesh
                    .vertices
                    .iter()
                    .map(|v| point3(v.x, v.y, v.z))
                    .collect();
                let pts =
                    c.w.add("IFCCARTESIANPOINTLIST3D", format!("({})", coords.join(",")));
                let mut tris = Vec::new();
                for face in 0..mesh.faces.len() {
                    for [a, b, d] in mesh.face_triangles(face) {
                        tris.push(format!("({},{},{})", a + 1, b + 1, d + 1));
                    }
                }
                let fs = match cut {
                    Some(m) => {
                        let at = f.origin + DVec3::Z * elevation(f.level);
                        tessellate(&mut c, m, |p| p - at)
                    }
                    None => c.w.add(
                        "IFCTRIANGULATEDFACESET",
                        format!("{pts},$,.T.,({}),$", tris.join(",")),
                    ),
                };
                let shape = c.shape("Tessellation", &[fs]);
                let (class, tail) = free_mesh_entity(&f.ifc_class);
                let r = product(&mut c, &class, e, pl, shape, &tail);
                if crate::import::could_be_native(&class) {
                    // so CreaBIM reads it back as the mesh it is
                    let props = [("Kind", Prop::Label("FreeMesh".into()))];
                    c.pset_on(&[r], crate::import::MEMBER_PSET, &props);
                }
                contained.entry(f.level).or_default().push(r);
                if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
            }
            // a void is an opening in what it cuts, written below
            ElementKind::Part(p) if p.void => {}
            k if creabim_core::members::is_member(k) => {
                let level = creabim_core::members::level_of(k);
                let Some(&(_, spl)) = level.and_then(|l| storeys.get(&l)) else {
                    continue;
                };
                // the shape as built, tessellated in the storey's frame
                let mesh = match cut {
                    Some(m) => Some(m.clone()),
                    None => creabim_geom::members::member_mesh(model, e).map(|m| m.0),
                };
                let Some(mesh) = mesh else { continue };
                let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
                let z0 = elevation(level);
                let fs = tessellate(&mut c, &mesh, |p| p - DVec3::Z * z0);
                let shape = c.shape("Tessellation", &[fs]);
                let (class, tail) = match k {
                    ElementKind::Part(_) | ElementKind::Site(_) => part_entity(e),
                    _ => {
                        let (class, tail) = member_entity(k);
                        (class.to_string(), tail.to_string())
                    }
                };
                let r = product(&mut c, &class, e, pl, shape, &tail);
                of_type(e, r);
                contained.entry(level).or_default().push(r);
                if let ElementKind::Footing(f) = k {
                    // its own values, so CreaBIM reads it back as a footing
                    let host = f.host.filter(|h| model.get(*h).is_some());
                    let props = footing_props(f, host.map(|h| h.ifc_guid()));
                    c.pset_on(&[r], FOOTING_PSET, &props);
                } else if let Ok(json) = serde_json::to_string(k) {
                    // its own definition, so CreaBIM reads it back as the
                    // same column, beam, roof, stair or railing
                    let props = [("Definition", Prop::Text(json))];
                    c.pset_on(&[r], crate::import::MEMBER_PSET, &props);
                }
                if cut.is_none() || layers_mode {
                    write_layer_parts(
                        &mut c,
                        model,
                        e,
                        r,
                        spl,
                        z0,
                        &joins,
                        &mut by_material,
                        layers_mode,
                    );
                }
                if let ElementKind::Roof(roof) = k {
                    // the mesh has its holes already; say so for other tools
                    let vt = roof.vertical_thickness();
                    let z = roof.base_offset - vt - 0.1;
                    for (i, h) in creabim_core::slab::holes_of(model, e).iter().enumerate() {
                        let solid = c.plan_extrusion(h, z, roof.peak() + vt + 0.2);
                        c.void(r, pl, solid, &format!("{} hole {}", e.name, i + 1), None);
                    }
                }
                if let Some((a, _)) = creabim_core::assembly_of(model, e) {
                    // a roof's shape isn't a straight extrusion: the set itself
                    layered.push((r, a.id, LayerUse::Set));
                } else if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
            }
            _ => {}
        }
    }

    // A void is an opening in each element it cuts, in its own shape.
    for (target, void) in creabim_core::parts::void_cuts(model) {
        let (Some(host), Some(v)) = (c.products.get(&target).copied(), model.get(void)) else {
            continue;
        };
        let ElementKind::Part(p) = &v.kind else {
            continue;
        };
        let (Some(&(_, spl)), Some(mesh)) = (
            storeys.get(&p.level),
            creabim_geom::parts::part_mesh(model, p),
        ) else {
            continue;
        };
        let z0 = elevation(Some(p.level));
        let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
        let fs = tessellate(&mut c, &mesh, |q| q - DVec3::Z * z0);
        let shape = c.shape("Tessellation", &[fs]);
        let name = model.get(target).map_or("", |t| t.name.as_str());
        let opening = c.w.add(
            "IFCOPENINGELEMENT",
            format!(
                "{},$,{},$,$,{pl},{shape},$,.OPENING.",
                c.guid(),
                string(&format!("{} in {name}", v.name))
            ),
        );
        let g = c.guid();
        c.w.add("IFCRELVOIDSELEMENT", format!("{g},$,$,$,{host},{opening}"));
    }

    // Openings need their host wall exported first, so they get a second pass.
    for e in model.iter() {
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        let Some(&(wall_r, wall_pl)) = wall_refs.get(&o.host) else {
            continue;
        };
        let Some(ElementKind::Wall(wall)) = model.get(o.host).map(|h| &h.kind) else {
            continue;
        };
        // on the wall's centreline, wherever its axis is
        let loc = wall_axes.get(&o.host).copied().unwrap_or(0.0);
        let (pl, shape) = opening_body(&mut c, wall_pl, loc, wall, o);
        // a bare hole is the opening itself, under its own id
        let hole = o.kind == OpeningKind::Hole;
        let (guid, name) = if hole {
            (string(&e.id.ifc_guid()), e.name.clone())
        } else {
            (c.guid(), format!("{} opening", e.name))
        };
        let opening = c.w.add(
            "IFCOPENINGELEMENT",
            format!("{guid},$,{},$,$,{pl},{shape},$,.OPENING.", string(&name)),
        );
        let g = c.guid();
        c.w.add(
            "IFCRELVOIDSELEMENT",
            format!("{g},$,$,$,{wall_r},{opening}"),
        );
        if hole {
            continue;
        }

        // the door or window itself: its frame, glass, leaf and handle,
        // each part a box, placed where the opening starts in the wall. A
        // door's own y axis points the way it swings, as IFC has it, so a
        // door opening to the wall's right is placed turned round.
        let cut = creabim_geom::wall::Cut::of(o, wall);
        let start = cut.map_or(DVec3::ZERO, |cut| {
            DVec3::new(cut.u0 - o.offset, 0.0, cut.v0 - o.sill)
        });
        let width = cut.map_or(o.width, |cut| cut.u1 - cut.u0);
        let turned = o.kind == OpeningKind::Door && !o.faces_left(wall);
        let fill_pl = if turned {
            c.local_placement(Some(pl), start + DVec3::X * width, Some(-DVec3::X))
        } else {
            c.local_placement(Some(pl), start, None)
        };
        let mut parts = Vec::new();
        for b in creabim_geom::opening::opening_boxes(o, wall) {
            let size = b.hi - b.lo;
            let lo = if turned {
                DVec3::new(width - b.hi.x, -b.hi.y, b.lo.z)
            } else {
                b.lo
            };
            let pos = c.placement3d(lo, None, None);
            parts.push(c.rect_extrusion(size.truncate() / 2.0, size.x, size.y, pos, size.z));
        }
        if parts.is_empty() {
            continue;
        }
        let fill_shape = c.shape("SweptSolid", &parts);
        // hinged on the left seen along its y axis (the way it swings)
        let hinge_left = !o.flip_hand != turned;
        let (class, tail) = match o.kind {
            OpeningKind::Door => (
                "IFCDOOR",
                format!(
                    ",$,{},{},.DOOR.,{},$",
                    real(o.height),
                    real(o.width),
                    if hinge_left {
                        ".SINGLE_SWING_LEFT."
                    } else {
                        ".SINGLE_SWING_RIGHT."
                    }
                ),
            ),
            OpeningKind::Window => (
                "IFCWINDOW",
                format!(
                    ",$,{},{},.WINDOW.,.SINGLE_PANEL.,$",
                    real(o.height),
                    real(o.width)
                ),
            ),
            OpeningKind::Hole => continue,
        };
        let fill = product(&mut c, class, e, fill_pl, fill_shape, &tail);
        of_type(e, fill);
        let g = c.guid();
        c.w.add("IFCRELFILLSELEMENT", format!("{g},$,$,$,{opening},{fill}"));
        if let Some((m, _)) = creabim_core::material_of(model, e) {
            by_material.entry(m.id).or_default().push(fill);
        }
        let level = match model.get(o.host).map(|h| &h.kind) {
            Some(ElementKind::Wall(w)) => w.level,
            _ => continue,
        };
        contained.entry(Some(level)).or_default().push(fill);
    }

    // A curtain wall set in a wall cuts its own opening there and fills it,
    // as a door does.
    for e in model.iter() {
        let ElementKind::Wall(wall) = &e.kind else {
            continue;
        };
        for (cw, o) in creabim_core::curtain::voids_in(model, e.id) {
            let (Some(&(wall_r, wall_pl)), Some(&(cw_r, _))) =
                (wall_refs.get(&e.id), wall_refs.get(&cw))
            else {
                continue;
            };
            let loc = wall_axes.get(&e.id).copied().unwrap_or(0.0);
            let (pl, shape) = opening_body(&mut c, wall_pl, loc, wall, &o);
            let name = model.get(cw).map_or("Curtain wall", |x| x.name.as_str());
            let opening = c.w.add(
                "IFCOPENINGELEMENT",
                format!(
                    "{},$,{},$,$,{pl},{shape},$,.OPENING.",
                    c.guid(),
                    string(&format!("{name} opening"))
                ),
            );
            let g = c.guid();
            c.w.add(
                "IFCRELVOIDSELEMENT",
                format!("{g},$,$,$,{wall_r},{opening}"),
            );
            let g = c.guid();
            c.w.add("IFCRELFILLSELEMENT", format!("{g},$,$,$,{opening},{cw_r}"));
        }
    }

    spaces(&mut c, model, &storeys, &wall_refs, &ceilings);

    // Materials, each associated with the elements made of it. Unused ones
    // are written too, so the project's material library goes along.
    let mut material_refs: HashMap<ElementId, Ref> = HashMap::new();
    // each material's surface style, for styling the parts of objects
    let mut material_styles: HashMap<ElementId, Ref> = HashMap::new();
    for e in model.iter() {
        let users = by_material.get(&e.id);
        let ElementKind::Material(mat) = &e.kind else {
            continue;
        };
        let mut description = format!("{HATCH_PREFIX}{}", mat.hatch.label());
        if !mat.description.trim().is_empty() {
            description = format!("{description}. {}", mat.description.trim());
        }
        let category = match mat.category.trim() {
            "" => "$".to_string(),
            c => string(c),
        };
        let m = c.w.add(
            "IFCMATERIAL",
            format!("{},{},{category}", string(&e.name), string(&description)),
        );
        material_refs.insert(e.id, m);
        // the look, as a surface style on the material: colour,
        // transparency, roughness and whether it is a metal
        let [r, g, b] = mat.color.map(|v| real(v as f64));
        let rgb = c.w.add("IFCCOLOURRGB", format!("$,{r},{g},{b}"));
        let method = if mat.metallic >= 0.5 {
            ".METAL."
        } else if mat.category == "glass" {
            ".GLASS."
        } else {
            ".NOTDEFINED."
        };
        let rendering = c.w.add(
            "IFCSURFACESTYLERENDERING",
            format!(
                "{rgb},{},$,$,$,$,$,IFCSPECULARROUGHNESS({}),{method}",
                real((1.0 - mat.opacity as f64).clamp(0.0, 1.0)),
                real(mat.roughness as f64)
            ),
        );
        let style = c.w.add(
            "IFCSURFACESTYLE",
            format!("{},.BOTH.,({rendering})", string(&e.name)),
        );
        material_styles.insert(e.id, style);
        let styled = c.w.add("IFCSTYLEDITEM", format!("$,({style}),$"));
        let styled_rep = c.w.add(
            "IFCSTYLEDREPRESENTATION",
            format!("{},'Style','Material',({styled})", c.body),
        );
        c.w.add(
            "IFCMATERIALDEFINITIONREPRESENTATION",
            format!("$,$,({styled_rep}),{m}"),
        );
        if let Some(users) = users.filter(|u| !u.is_empty()) {
            let g = c.guid();
            c.w.add(
                "IFCRELASSOCIATESMATERIAL",
                format!("{g},$,$,$,{},{m}", list(users)),
            );
        }
    }

    // Assemblies as layer sets, unused ones too; each layered element gets
    // a usage saying how the set lies in it (walls across, slabs down).
    let mut layer_sets: HashMap<ElementId, Ref> = HashMap::new();
    for e in model.iter() {
        let ElementKind::Assembly(a) = &e.kind else {
            continue;
        };
        if a.layers.is_empty() {
            continue;
        }
        let layers: Vec<Ref> = a
            .layers
            .iter()
            .map(|l| {
                let m = l
                    .material
                    .and_then(|m| material_refs.get(&m))
                    .map_or("$".to_string(), |r| r.to_string());
                let air = l.function == creabim_core::LayerFunction::AirGap;
                c.w.add(
                    "IFCMATERIALLAYER",
                    format!(
                        "{m},{},{},{},$,{},$",
                        real(l.thickness),
                        if air { ".T." } else { ".F." },
                        string(l.function.label()),
                        string(l.function.ifc_category())
                    ),
                )
            })
            .collect();
        let description = match a.description.trim() {
            "" => "$".to_string(),
            d => string(d),
        };
        let set = c.w.add(
            "IFCMATERIALLAYERSET",
            format!("{},{},{description}", list(&layers), string(&e.name)),
        );
        layer_sets.insert(e.id, set);
    }
    for (product, assembly, how) in &layered {
        let Some(&set) = layer_sets.get(assembly) else {
            continue;
        };
        let relating = match how {
            LayerUse::Set => set,
            LayerUse::Usage(axis, positive, offset) => c.w.add(
                "IFCMATERIALLAYERSETUSAGE",
                format!(
                    "{set},.{axis}.,{},{},$",
                    if *positive {
                        ".POSITIVE."
                    } else {
                        ".NEGATIVE."
                    },
                    real(*offset)
                ),
            ),
        };
        let g = c.guid();
        c.w.add(
            "IFCRELASSOCIATESMATERIAL",
            format!("{g},$,$,$,({product}),{relating}"),
        );
    }

    // Element types, each with its shared parameters, its material or
    // layer set, and the elements of the type.
    for e in model.iter() {
        let ElementKind::ElementType(t) = &e.kind else {
            continue;
        };
        if let creabim_core::TypeParams::Object(o) = &t.params {
            let looks = Looks {
                materials: &material_refs,
                styles: &material_styles,
            };
            for (level, r) in object_type(&mut c, model, e, t, o, &looks, &storeys) {
                contained.entry(Some(level)).or_default().push(r);
            }
            continue;
        }
        let set = c.pset(TYPE_PSET, &type_props(t));
        let description = match t.description.trim() {
            "" => "$".to_string(),
            d => string(d),
        };
        let head = format!(
            "{},$,{},{description},$,({set}),$,$,$",
            string(&e.id.ifc_guid()),
            string(&e.name)
        );
        use creabim_core::TypeParams as P;
        let (class, tail) = match &t.params {
            P::Wall(w) if w.curtain.is_some() => ("IFCCURTAINWALLTYPE", ",.NOTDEFINED."),
            P::Wall(_) => ("IFCWALLTYPE", ",.STANDARD."),
            P::Door(_) => ("IFCDOORTYPE", ",.DOOR.,.SINGLE_SWING_LEFT.,.F.,$"),
            P::Window(_) => ("IFCWINDOWTYPE", ",.WINDOW.,.SINGLE_PANEL.,.F.,$"),
            P::Slab(_) => ("IFCSLABTYPE", ",.FLOOR."),
            P::Roof(_) => ("IFCROOFTYPE", ",.NOTDEFINED."),
            P::Column(_) => ("IFCCOLUMNTYPE", ",.COLUMN."),
            P::Beam(_) => ("IFCBEAMTYPE", ",.BEAM."),
            P::Footing(f) => match f.kind {
                creabim_core::FootingKind::Strip => ("IFCFOOTINGTYPE", ",.STRIP_FOOTING."),
                creabim_core::FootingKind::Pad => ("IFCFOOTINGTYPE", ",.PAD_FOOTING."),
            },
            P::Object(_) => unreachable!("object types are written above"),
        };
        let tr = c.w.add(class, format!("{head}{tail}"));
        let relating = t
            .assembly()
            .and_then(|a| layer_sets.get(&a))
            .or_else(|| t.material().and_then(|m| material_refs.get(&m)))
            .copied();
        if let Some(m) = relating {
            let g = c.guid();
            c.w.add("IFCRELASSOCIATESMATERIAL", format!("{g},$,$,$,({tr}),{m}"));
        }
        if let Some(users) = typed.get(&e.id).filter(|u| !u.is_empty()) {
            let g = c.guid();
            c.w.add(
                "IFCRELDEFINESBYTYPE",
                format!("{g},$,$,$,{},{tr}", list(users)),
            );
        }
    }

    // Spatial structure.
    let g = c.guid();
    c.w.add("IFCRELAGGREGATES", format!("{g},$,$,$,{project},({site})"));
    let g = c.guid();
    c.w.add("IFCRELAGGREGATES", format!("{g},$,$,$,{site},({building})"));
    if !storey_order.is_empty() {
        let g = c.guid();
        c.w.add(
            "IFCRELAGGREGATES",
            format!("{g},$,$,$,{building},{}", list(&storey_order)),
        );
    }
    let mut keys: Vec<_> = contained.keys().copied().collect();
    keys.sort_by_key(|k| {
        k.map(|id| storeys.get(&id).map(|s| s.0 .0))
            .unwrap_or(Some(0))
    });
    for k in keys {
        let items = &contained[&k];
        let container = k
            .and_then(|id| storeys.get(&id))
            .map(|s| s.0)
            .unwrap_or(building);
        let g = c.guid();
        c.w.add(
            "IFCRELCONTAINEDINSPATIALSTRUCTURE",
            format!("{g},$,$,$,{},{container}", list(items)),
        );
    }

    let header = format!(
        "FILE_DESCRIPTION(('ViewDefinition [DesignTransferView]'),'2;1');\n\
         FILE_NAME({},{},(''),(''),{},{},'');\n\
         FILE_SCHEMA(('IFC4'));\n",
        string(&format!("{}.ifc", opts.project_name)),
        string(&opts.timestamp),
        string(concat!("CreaBIM ", env!("CARGO_PKG_VERSION"))),
        string("CreaBIM"),
    );
    c.w.finish(&header)
}

/// The materials written, and their surface styles.
struct Looks<'a> {
    materials: &'a HashMap<ElementId, Ref>,
    styles: &'a HashMap<ElementId, Ref>,
}

/// An object type: its parts as one representation map, each part's face
/// set styled with its material's look, its plan symbol as another, its
/// materials as a constituent set; then every object of the type, each
/// mapping both maps where it stands. Returns the objects written, with
/// their levels.
fn object_type(
    c: &mut Ctx,
    model: &Model,
    e: &Element,
    t: &creabim_core::ElementType,
    o: &creabim_core::ObjectType,
    looks: &Looks,
    storeys: &HashMap<ElementId, (Ref, Ref)>,
) -> Vec<(ElementId, Ref)> {
    let kind = o.ifc.normalized();
    // the parts, in the type's own axes
    let mut items = vec![];
    for p in &o.parts {
        let coords: Vec<String> = p
            .mesh
            .vertices
            .iter()
            .map(|v| point3(v.x, v.y, v.z))
            .collect();
        let pts =
            c.w.add("IFCCARTESIANPOINTLIST3D", format!("({})", coords.join(",")));
        let mut tris = vec![];
        for f in 0..p.mesh.faces.len() {
            for [a, b, d] in p.mesh.face_triangles(f) {
                tris.push(format!("({},{},{})", a + 1, b + 1, d + 1));
            }
        }
        let fs = c.w.add(
            "IFCTRIANGULATEDFACESET",
            format!("{pts},$,.T.,({}),$", tris.join(",")),
        );
        if let Some(style) = p.material.and_then(|m| looks.styles.get(&m)) {
            c.w.add("IFCSTYLEDITEM", format!("{fs},({style}),$"));
        }
        items.push(fs);
    }
    let body_rep = c.body_rep("Tessellation", &items);
    let origin = c.placement3d(DVec3::ZERO, None, None);
    let body_map =
        c.w.add("IFCREPRESENTATIONMAP", format!("{origin},{body_rep}"));
    // the plan symbol, as polylines on the floor
    let mut curves = vec![];
    for s in o.plan_symbol() {
        let pts: Vec<DVec2> = match s {
            creabim_core::objects::SymbolItem::Loop(mut p) => {
                if let Some(first) = p.first().copied() {
                    p.push(first);
                }
                p
            }
            creabim_core::objects::SymbolItem::Line(a, b) => vec![a, b],
        };
        if pts.len() < 2 {
            continue;
        }
        let refs: Vec<Ref> = pts
            .iter()
            .map(|p| c.w.add("IFCCARTESIANPOINT", point3(p.x, p.y, 0.0)))
            .collect();
        curves.push(c.w.add("IFCPOLYLINE", list(&refs)));
    }
    let foot_map = (!curves.is_empty()).then(|| {
        let set = c.w.add("IFCGEOMETRICCURVESET", list(&curves));
        let rep = c.w.add(
            "IFCSHAPEREPRESENTATION",
            format!("{},'FootPrint','GeometricCurveSet',({set})", c.footprint),
        );
        let origin = c.placement3d(DVec3::ZERO, None, None);
        c.w.add("IFCREPRESENTATIONMAP", format!("{origin},{rep}"))
    });
    let maps: Vec<Ref> = std::iter::once(body_map).chain(foot_map).collect();
    let set = c.pset(TYPE_PSET, &type_props(t));
    let description = match t.description.trim() {
        "" => "$".to_string(),
        d => string(d),
    };
    let type_class = kind.type_class().to_ascii_uppercase();
    // AssemblyPlace comes before PredefinedType on furniture types
    let place = if type_class == "IFCFURNITURETYPE" {
        ".NOTDEFINED.,"
    } else {
        ""
    };
    let tr = c.w.add(
        &type_class,
        format!(
            "{},$,{},{description},$,({set}),{},$,{},{place}.{}.",
            string(&e.id.ifc_guid()),
            string(&e.name),
            list(&maps),
            string(&e.name),
            kind.predefined
        ),
    );
    // the materials of the parts
    let mut constituents = vec![];
    for p in &o.parts {
        let Some(m) = p.material.and_then(|m| looks.materials.get(&m)) else {
            continue;
        };
        let name = if p.name.is_empty() {
            "$".to_string()
        } else {
            string(&p.name)
        };
        constituents.push(c.w.add("IFCMATERIALCONSTITUENT", format!("{name},$,{m},$,$")));
    }
    if !constituents.is_empty() {
        let cs = c.w.add(
            "IFCMATERIALCONSTITUENTSET",
            format!("{},$,{}", string(&e.name), list(&constituents)),
        );
        let g = c.guid();
        c.w.add("IFCRELASSOCIATESMATERIAL", format!("{g},$,$,$,({tr}),{cs}"));
    }
    // the objects of the type
    let mut out = vec![];
    for id in creabim_core::types::instances(model, e.id) {
        let Some(oe) = model.get(id) else { continue };
        let ElementKind::Object(ob) = &oe.kind else {
            continue;
        };
        let Some(&(_, spl)) = storeys.get(&ob.level) else {
            continue;
        };
        let (s, co) = ob.rotation.to_radians().sin_cos();
        // hung from the ceiling or not, where it stands above its storey
        let floor = model.level(ob.level).map_or(0.0, |l| l.elevation);
        let z = creabim_core::objects::base_z_of(model, ob, o) - floor;
        let pl = c.local_placement(
            Some(spl),
            ob.position.extend(z),
            Some(DVec3::new(co, s, 0.0)),
        );
        let mut reps = vec![];
        for (i, map) in maps.iter().enumerate() {
            let at = c.w.add("IFCCARTESIANPOINT", "(0.,0.,0.)");
            let op = c.w.add(
                "IFCCARTESIANTRANSFORMATIONOPERATOR3D",
                format!("$,$,{at},$,$"),
            );
            let item = c.w.add("IFCMAPPEDITEM", format!("{map},{op}"));
            let (ctx, ident) = if i == 0 {
                (c.body, "Body")
            } else {
                (c.footprint, "FootPrint")
            };
            reps.push(c.w.add(
                "IFCSHAPEREPRESENTATION",
                format!("{ctx},'{ident}','MappedRepresentation',({item})"),
            ));
        }
        let shape =
            c.w.add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,{}", list(&reps)));
        let object_type = if kind.predefined == "USERDEFINED" {
            string(&e.name)
        } else {
            "$".into()
        };
        let r = c.w.add(
            &kind.class.to_ascii_uppercase(),
            format!(
                "{},$,{},$,{object_type},{pl},{shape},{},.{}.",
                string(&oe.id.ifc_guid()),
                string(&oe.name),
                c.tag(oe.id),
                kind.predefined
            ),
        );
        out.push((ob.level, r));
    }
    if !out.is_empty() {
        let g = c.guid();
        let users: Vec<Ref> = out.iter().map(|o| o.1).collect();
        c.w.add(
            "IFCRELDEFINESBYTYPE",
            format!("{g},$,$,$,{},{tr}", list(&users)),
        );
    }
    out
}

/// Whether element `e`'s layers are not all as its layer set puts them:
/// a layer end or edge moved by hand, or cores lapped at a corner.
fn layers_differ(e: &Element, joins: &HashMap<ElementId, creabim_core::WallEnds>) -> bool {
    match &e.kind {
        ElementKind::Wall(w) => {
            !w.layer_ends.is_empty()
                || joins
                    .get(&e.id)
                    .is_some_and(|j| j.core_line.iter().any(Option::is_some))
        }
        ElementKind::Slab(s) => !s.layer_edges.is_empty(),
        ElementKind::Roof(r) => !r.layer_edges.is_empty(),
        _ => false,
    }
}

/// Element `host`'s layers as `IfcBuildingElementPart`s that make it up,
/// each its own solid with its layer's material, where the layers are not
/// all as the layer set puts them; the host keeps its body and layer set
/// for tools that read only those. Placed in the storey at `spl`, `z0` up.
#[allow(clippy::too_many_arguments)]
fn write_layer_parts(
    c: &mut Ctx,
    model: &Model,
    e: &Element,
    host: Ref,
    spl: Ref,
    z0: f64,
    joins: &HashMap<ElementId, creabim_core::WallEnds>,
    by_material: &mut HashMap<ElementId, Vec<Ref>>,
    as_coverings: bool,
) {
    if as_coverings {
        write_layer_coverings(c, model, e, host, spl, z0, joins, by_material);
        return;
    }
    if !layers_differ(e, joins) {
        return;
    }
    let names: Vec<String> = creabim_core::assembly_of(model, e)
        .map(|(_, a)| {
            a.layers
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    l.material
                        .and_then(|m| model.get(m))
                        .map_or_else(|| format!("layer {}", i + 1), |m| m.name.clone())
                })
                .collect()
        })
        .unwrap_or_default();
    let mut parts = vec![];
    for (layer, mesh) in creabim_geom::scene::layer_parts(model, e, joins) {
        if mesh.indices.is_empty() {
            continue;
        }
        let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
        let fs = tessellate(c, &mesh, |p| p - DVec3::Z * z0);
        let shape = c.shape("Tessellation", &[fs]);
        let g = c.guid();
        let name = names
            .get(layer.index)
            .cloned()
            .unwrap_or_else(|| format!("layer {}", layer.index + 1));
        let p = c.w.add(
            "IFCBUILDINGELEMENTPART",
            format!(
                "{g},$,{},$,$,{pl},{shape},$,.NOTDEFINED.",
                string(&format!("{} {name}", e.name))
            ),
        );
        if let Some(m) = layer.material {
            by_material.entry(m).or_default().push(p);
        }
        parts.push(p);
    }
    if !parts.is_empty() {
        let g = c.guid();
        c.w.add(
            "IFCRELAGGREGATES",
            format!("{g},$,$,$,{host},{}", list(&parts)),
        );
    }
}

/// Element `host`'s layers other than its structure as `IfcCovering`s
/// covering it (MU20, "Layers as separate elements"): finishes as
/// cladding, flooring or roofing, insulation, membranes; air gaps are
/// left out. The host's own body is its structure.
#[allow(clippy::too_many_arguments)]
fn write_layer_coverings(
    c: &mut Ctx,
    model: &Model,
    e: &Element,
    host: Ref,
    spl: Ref,
    z0: f64,
    joins: &HashMap<ElementId, creabim_core::WallEnds>,
    by_material: &mut HashMap<ElementId, Vec<Ref>>,
) {
    use creabim_core::LayerFunction as F;
    let parts = creabim_geom::scene::layer_parts(model, e, joins);
    if parts.len() < 2 {
        return;
    }
    let mut covers = vec![];
    for (layer, mesh) in parts {
        if layer.core || mesh.indices.is_empty() || layer.function == F::AirGap {
            continue;
        }
        let kind = match (layer.function, &e.kind) {
            (F::Insulation, _) => "INSULATION",
            (F::Membrane, _) => "MEMBRANE",
            (_, ElementKind::Slab(_)) => "FLOORING",
            (_, ElementKind::Roof(_)) => "ROOFING",
            _ => "CLADDING",
        };
        let name = layer
            .material
            .and_then(|m| model.get(m))
            .map_or_else(|| format!("layer {}", layer.index + 1), |m| m.name.clone());
        let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
        let fs = tessellate(c, &mesh, |p| p - DVec3::Z * z0);
        let shape = c.shape("Tessellation", &[fs]);
        let g = c.guid();
        let r = c.w.add(
            "IFCCOVERING",
            format!(
                "{g},$,{},$,$,{pl},{shape},$,.{kind}.",
                string(&format!("{} {name}", e.name))
            ),
        );
        if let Some(m) = layer.material {
            by_material.entry(m).or_default().push(r);
        }
        covers.push(r);
    }
    if !covers.is_empty() {
        let g = c.guid();
        c.w.add(
            "IFCRELCOVERSBLDGELEMENTS",
            format!("{g},$,$,$,{host},{}", list(&covers)),
        );
    }
}

/// How a layered element takes its assembly's layer set.
enum LayerUse {
    /// Through an `IfcMaterialLayerSetUsage`: the axis the layers run
    /// along, whether they run its positive way, and where the first
    /// layer starts from the element's reference line.
    Usage(&'static str, bool, f64),
    /// The set itself, for shapes that aren't straight extrusions.
    Set,
}

/// A triangle mesh as an `IfcTriangulatedFaceSet`, its corners taken into
/// the product's own coordinates by `to_local`.
/// `solid`, in a wall's own axes (`x` along `d` and `y` along `n` from
/// `origin`, `z` up from `z0`), with what lies above the underside `f` cut
/// away: a half-space over the face's plane, bounded by its outline in
/// plan, as Revit writes walls attached to roofs.
#[allow(clippy::too_many_arguments)]
fn clip_under(
    c: &mut Ctx,
    solid: Ref,
    f: &creabim_core::walls::TopFace,
    to_local: impl Fn(DVec2) -> DVec2,
    d: DVec2,
    n: DVec2,
    origin: DVec2,
    z0: f64,
) -> Ref {
    // the plane z = h + a x + b y in the wall's axes
    let (a, b) = (f.g.dot(d), f.g.dot(n));
    let h = f.c + f.g.dot(origin) - z0;
    let normal = DVec3::new(-a, -b, 1.0).normalize();
    let along = DVec3::new(1.0, 0.0, a).normalize();
    let pos = c.placement3d(DVec3::new(0.0, 0.0, h), Some(normal), Some(along));
    let plane = c.w.add("IFCPLANE", pos.to_string());
    let mut pts: Vec<Ref> = f
        .outline
        .iter()
        .map(|p| {
            let q = to_local(*p);
            c.w.add("IFCCARTESIANPOINT", point2(q.x, q.y))
        })
        .collect();
    pts.push(pts[0]);
    let boundary = c.w.add("IFCPOLYLINE", list(&pts));
    let at = c.placement3d(DVec3::ZERO, None, None);
    // the material is above the plane, on the side its normal points to
    let half = c.w.add(
        "IFCPOLYGONALBOUNDEDHALFSPACE",
        format!("{plane},.F.,{at},{boundary}"),
    );
    c.w.add(
        "IFCBOOLEANCLIPPINGRESULT",
        format!(".DIFFERENCE.,{solid},{half}"),
    )
}

fn tessellate(
    c: &mut Ctx,
    m: &creabim_geom::mesh::TriMesh,
    to_local: impl Fn(DVec3) -> DVec3,
) -> Ref {
    let coords: Vec<String> = m
        .positions
        .iter()
        .map(|p| {
            let q = to_local(glam::Vec3::from(*p).as_dvec3());
            point3(q.x, q.y, q.z)
        })
        .collect();
    let pts =
        c.w.add("IFCCARTESIANPOINTLIST3D", format!("({})", coords.join(",")));
    let tris: Vec<String> = m
        .indices
        .chunks(3)
        .map(|t| format!("({},{},{})", t[0] + 1, t[1] + 1, t[2] + 1))
        .collect();
    c.w.add(
        "IFCTRIANGULATEDFACESET",
        format!("{pts},$,.T.,({}),$", tris.join(",")),
    )
}

/// The placement and body of opening `o` in `wall` (placed at `wall_pl`,
/// its axis `loc` off its centreline): a box through the wall, a little
/// proud of both faces.
fn opening_body(
    c: &mut Ctx,
    wall_pl: Ref,
    loc: f64,
    wall: &creabim_core::Wall,
    o: &creabim_core::Opening,
) -> (Ref, Ref) {
    let eps = 0.05;
    let pl = c.local_placement(Some(wall_pl), DVec3::new(o.offset, -loc, o.sill), None);
    let pos = c.placement3d(
        DVec3::new(o.width / 2.0, wall.thickness / 2.0 + eps, o.height / 2.0),
        Some(-DVec3::Y),
        Some(DVec3::X),
    );
    let solid = c.rect_extrusion(
        DVec2::ZERO,
        o.width,
        o.height,
        pos,
        wall.thickness + 2.0 * eps,
    );
    (pl, c.shape("SweptSolid", &[solid]))
}

/// Rooms as IfcSpace: each its outline extruded from its floor to its
/// top, numbered in Name and named in LongName, with its finishes,
/// occupancy and quantities, bounded (IfcRelSpaceBoundary) by the walls
/// round it and covered (IfcRelCoversSpaces) by its ceilings; room
/// separators as the IfcVirtualElements that bound rooms where there is
/// no wall. Spaces make up their storeys (IfcRelAggregates).
fn spaces(
    c: &mut Ctx,
    model: &Model,
    storeys: &HashMap<ElementId, (Ref, Ref)>,
    wall_refs: &HashMap<ElementId, (Ref, Ref)>,
    ceilings: &[(ElementId, Ref)],
) {
    let mut separators: HashMap<ElementId, Ref> = HashMap::new();
    let mut contained: HashMap<ElementId, Vec<Ref>> = HashMap::new();
    for e in model.iter() {
        let ElementKind::RoomSeparator(s) = &e.kind else {
            continue;
        };
        let Some(&(_, spl)) = storeys.get(&s.level) else {
            continue;
        };
        let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
        let a = c.w.add("IFCCARTESIANPOINT", point2(s.a.x, s.a.y));
        let b = c.w.add("IFCCARTESIANPOINT", point2(s.b.x, s.b.y));
        let line = c.w.add("IFCPOLYLINE", list([a, b]));
        let axis = c.w.add(
            "IFCSHAPEREPRESENTATION",
            format!("{},'Axis','Curve2D',({line})", c.axis),
        );
        let shape =
            c.w.add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,({axis})"));
        let r = product(c, "IFCVIRTUALELEMENT", e, pl, shape, ",$");
        separators.insert(e.id, r);
        contained.entry(s.level).or_default().push(r);
    }
    for (level, items) in contained {
        if let Some(&(storey, _)) = storeys.get(&level) {
            let g = c.guid();
            c.w.add(
                "IFCRELCONTAINEDINSPATIALSTRUCTURE",
                format!("{g},$,$,$,{},{storey}", list(&items)),
            );
        }
    }
    // how many rooms each wall bounds: one is an outside wall
    let shapes: Vec<(&Element, &creabim_core::Room, creabim_core::room::RoomShape)> = model
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Room(r) => Some((e, r, creabim_core::room::shape_of(model, r)?)),
            _ => None,
        })
        .collect();
    let mut bounding: HashMap<ElementId, usize> = HashMap::new();
    for (_, _, s) in &shapes {
        let mut seen: Vec<ElementId> = s.bounded_by.iter().flatten().copied().collect();
        seen.sort_by_key(|id| id.0);
        seen.dedup();
        for id in seen {
            *bounding.entry(id).or_default() += 1;
        }
    }
    let mut by_storey: HashMap<ElementId, Vec<Ref>> = HashMap::new();
    for (e, r, s) in &shapes {
        let Some(&(_, spl)) = storeys.get(&r.level) else {
            continue;
        };
        let height = creabim_core::room::height(model, r);
        let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
        let solid = c.plan_extrusion(&s.outline, 0.0, height);
        let shape = c.shape("SweptSolid", &[solid]);
        let number = if r.number.is_empty() {
            e.name.clone()
        } else {
            r.number.clone()
        };
        let space = c.w.add(
            "IFCSPACE",
            format!(
                "{},$,{},$,$,{pl},{shape},{},.ELEMENT.,.SPACE.,$",
                string(&e.id.ifc_guid()),
                string(&number),
                string(&e.name)
            ),
        );
        by_storey.entry(r.level).or_default().push(space);
        let outdoor = [&r.occupancy, &r.department]
            .iter()
            .any(|s| s.eq_ignore_ascii_case("outdoor"));
        let area = creabim_core::room::net_area(model, r, s);
        c.pset_on(
            &[space],
            "Pset_SpaceCommon",
            &[
                ("Reference", Prop::Label(number.clone())),
                ("IsExternal", Prop::Bool(outdoor)),
            ],
        );
        let finishes: Vec<(&'static str, Prop)> = [
            ("FloorCovering", &r.floor_finish),
            ("WallCovering", &r.wall_finish),
            ("CeilingCovering", &r.ceiling_finish),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k, Prop::Label(v.clone())))
        .collect();
        if !finishes.is_empty() {
            c.pset_on(&[space], "Pset_SpaceCoveringRequirements", &finishes);
        }
        if !r.occupancy.is_empty() {
            c.pset_on(
                &[space],
                "Pset_SpaceOccupancyRequirements",
                &[("OccupancyType", Prop::Label(r.occupancy.clone()))],
            );
        }
        c.pset_on(&[space], ROOM_PSET, &room_props(r));
        // its quantities, for take-off in other tools
        let q = [
            c.w.add(
                "IFCQUANTITYAREA",
                format!("'NetFloorArea',$,$,{},$", real(area)),
            ),
            c.w.add(
                "IFCQUANTITYLENGTH",
                format!("'GrossPerimeter',$,$,{},$", real(s.perimeter())),
            ),
            c.w.add(
                "IFCQUANTITYLENGTH",
                format!("'Height',$,$,{},$", real(height)),
            ),
        ];
        let g = c.guid();
        let quantities = c.w.add(
            "IFCELEMENTQUANTITY",
            format!("{g},$,'Qto_SpaceBaseQuantities',$,$,{}", list(q)),
        );
        let g = c.guid();
        c.w.add(
            "IFCRELDEFINESBYPROPERTIES",
            format!("{g},$,$,$,({space}),{quantities}"),
        );
        // what bounds it, once each
        let mut done: Vec<ElementId> = vec![];
        for id in s.bounded_by.iter().flatten() {
            if done.contains(id) {
                continue;
            }
            done.push(*id);
            let (element, physical) = match (wall_refs.get(id), separators.get(id)) {
                (Some(&(w, _)), _) => (w, true),
                (None, Some(&v)) => (v, false),
                _ => continue,
            };
            let side = if !physical || bounding.get(id).copied().unwrap_or(0) > 1 {
                ".INTERNAL."
            } else {
                ".EXTERNAL."
            };
            let kind = if physical { ".PHYSICAL." } else { ".VIRTUAL." };
            let g = c.guid();
            c.w.add(
                "IFCRELSPACEBOUNDARY",
                format!("{g},$,$,$,{space},{element},$,{kind},{side}"),
            );
        }
        let covering: Vec<Ref> = ceilings
            .iter()
            .filter(|(room, _)| *room == e.id)
            .map(|c| c.1)
            .collect();
        if !covering.is_empty() {
            let g = c.guid();
            c.w.add(
                "IFCRELCOVERSSPACES",
                format!("{g},$,$,$,{space},{}", list(&covering)),
            );
        }
    }
    let mut levels: Vec<ElementId> = by_storey.keys().copied().collect();
    levels.sort_by_key(|l| storeys.get(l).map(|s| s.0 .0));
    for level in levels {
        let (storey, _) = storeys[&level];
        let g = c.guid();
        c.w.add(
            "IFCRELAGGREGATES",
            format!("{g},$,$,$,{storey},{}", list(&by_storey[&level])),
        );
    }
}

fn product(c: &mut Ctx, class: &str, e: &Element, pl: Ref, shape: Ref, tail: &str) -> Ref {
    // the tail starts with the Tag, unset: the element's mark goes there
    let tagged;
    let tail = match tail.strip_prefix(",$") {
        Some(rest) if c.marks.contains_key(&e.id) => {
            tagged = format!(",{}{rest}", c.tag(e.id));
            tagged.as_str()
        }
        _ => tail,
    };
    let r = c.w.add(
        class,
        format!(
            "{},$,{},$,$,{pl},{shape}{tail}",
            string(&e.id.ifc_guid()),
            string(&e.name)
        ),
    );
    c.products.insert(e.id, r);
    r
}

/// The STEP entity and record tail of an extrusion, a sweep or a site
/// element: the class picked for it, or a proxy when the exporter can't
/// write that class.
fn part_entity(e: &Element) -> (String, String) {
    match &e.kind {
        ElementKind::Site(s) if matches!(s.form, creabim_core::SiteForm::Terrain { .. }) => {
            ("IFCGEOGRAPHICELEMENT".into(), ",$,.TERRAIN.".into())
        }
        _ => free_mesh_entity(e.ifc_class()),
    }
}

/// The marks of everything that has one, objects' from their types.
fn element_marks(model: &Model) -> HashMap<ElementId, String> {
    let mut out = creabim_core::marks(model);
    for e in model.iter() {
        if matches!(e.kind, ElementKind::Object(_)) {
            if let Some(m) = creabim_core::marks::object_mark(model, e) {
                out.insert(e.id, m);
            }
        }
    }
    out
}

/// The STEP entity and record tail (Tag, PredefinedType) of a member.
fn member_entity(k: &ElementKind) -> (&'static str, &'static str) {
    match k {
        ElementKind::Column(_) => ("IFCCOLUMN", ",$,.COLUMN."),
        ElementKind::Beam(_) => ("IFCBEAM", ",$,.BEAM."),
        ElementKind::Roof(r) => (
            "IFCROOF",
            match r.effective_shape() {
                creabim_core::RoofShape::Flat => ",$,.FLAT_ROOF.",
                creabim_core::RoofShape::Shed => ",$,.SHED_ROOF.",
                creabim_core::RoofShape::Gable => ",$,.GABLE_ROOF.",
                creabim_core::RoofShape::Hip => ",$,.HIP_ROOF.",
            },
        ),
        ElementKind::Stair(s) if s.landing.is_some() => ("IFCSTAIR", ",$,.QUARTER_TURN_STAIR."),
        ElementKind::Stair(_) => ("IFCSTAIR", ",$,.STRAIGHT_RUN_STAIR."),
        ElementKind::Footing(f) => match f.kind {
            creabim_core::FootingKind::Strip => ("IFCFOOTING", ",$,.STRIP_FOOTING."),
            creabim_core::FootingKind::Pad => ("IFCFOOTING", ",$,.PAD_FOOTING."),
        },
        _ => ("IFCRAILING", ",$,.GUARDRAIL."),
    }
}

/// The STEP entity and record tail for a free mesh's IFC class; classes the
/// exporter cannot write become proxies.
fn free_mesh_entity(class: &str) -> (String, String) {
    match crate::classes::ifc4_element_tail(class) {
        Some(tail) => (class.to_ascii_uppercase(), format!(",${tail}")),
        None => ("IFCBUILDINGELEMENTPROXY".into(), ",$,.NOTDEFINED.".into()),
    }
}

#[cfg(test)]
mod project_tests {
    use super::*;

    #[test]
    fn options_take_the_projects_names() {
        let mut doc = creabim_core::Document::new();
        let o = ExportOptions::for_model(doc.model(), "File name");
        assert_eq!(o.project_name, "File name");
        let info = creabim_core::ProjectInfo {
            name: "Maple Lane House".into(),
            site_name: "Plot 4".into(),
            building_name: "House".into(),
            ..Default::default()
        };
        doc.commit(creabim_core::set_project(doc.model(), info))
            .unwrap();
        let o = ExportOptions::for_model(doc.model(), "File name");
        assert_eq!(o.project_name, "Maple Lane House");
        assert_eq!(o.site_name, "Plot 4");
        assert_eq!(o.building_name, "House");
    }
}
