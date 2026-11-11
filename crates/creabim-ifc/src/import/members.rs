//! Columns, beams, roofs, stairs and railings. Those CreaBIM wrote carry
//! their own definition in a property set and come back exactly; columns
//! and beams from other tools become native when their body is a straight
//! prism (an upright one with a rectangular or round section for a column,
//! a level bar with a rectangular section for a beam). The rest stay
//! meshes that keep their IFC class.

use super::brep;
use super::{remove_collinear, Importer};
use crate::shape::RevolvedSweep;
use crate::step_read::Entity;
use creabim_core::{Beam, Column, Element, ElementKind};
use glam::DMat4;

/// The property set holding a member's own definition.
pub(crate) const MEMBER_PSET: &str = "CreaBIM_Element";

const COLUMNS: &[&str] = &["IFCCOLUMN", "IFCCOLUMNSTANDARDCASE"];
const BEAMS: &[&str] = &["IFCBEAM", "IFCBEAMSTANDARDCASE"];

impl Importer<'_> {
    pub(super) fn import_members(&mut self) {
        let classes = [COLUMNS, BEAMS, &["IFCROOF"], &["IFCSTAIR"], &["IFCRAILING"]];
        for class in classes.concat() {
            let mut ids = self.ids(class).to_vec();
            ids.sort_unstable();
            for id in ids {
                self.import_member(id, class);
            }
        }
    }

    fn import_member(&mut self, id: u64, class: &str) {
        let Some(e) = self.f.get(id) else { return };
        if self.written_as_mesh(id) {
            return;
        }
        let kind = match self.member_definition(id) {
            Some(k) => Ok(k),
            None => {
                let world = self.g.placement(e.arg(5));
                // a clean revolution becomes a native arc sweep before
                // the prism analysis, which would only fail on its
                // curved faces
                if let Some(swept) = self.g.revolved_sweep(e, &world) {
                    match swept {
                        Ok(s) => Ok(self.part_member(id, e, s)),
                        Err(why) => Err(why),
                    }
                } else if COLUMNS.contains(&class) {
                    // a section no column builds (an I- or cross-section)
                    // still extrudes cleanly as a part
                    match self.native_column(id, e, &world) {
                        Ok(c) => Ok(ElementKind::Column(c)),
                        Err(why) => match self.g.extruded_part(e, &world) {
                            Some(Ok(s)) => Ok(self.extruded_part_ctor(id, e, s)),
                            Some(Err(w)) => Err(w),
                            None => Err(why),
                        },
                    }
                } else if BEAMS.contains(&class) {
                    // a section no beam builds, or a bar no beam follows
                    // (sloped, L- or I-section), still extrudes as a part
                    match self.native_beam(id, e, &world) {
                        Ok(b) => Ok(ElementKind::Beam(b)),
                        Err(why) => match self.g.extruded_part(e, &world) {
                            Some(Ok(s)) => Ok(self.extruded_part_ctor(id, e, s)),
                            Some(Err(w)) => Err(w),
                            None => Err(why),
                        },
                    }
                } else {
                    // roofs, stairs and railings from other tools stay
                    // meshes with their class
                    return;
                }
            }
        };
        let kind = match kind {
            Ok(k) => k,
            Err(why) => {
                if self.g.body(e).is_some() {
                    let body = self.body_kind(e);
                    self.mesh_reason
                        .insert(id, format!("is {body} that {why}; imported as a mesh"));
                }
                return;
            }
        };
        self.done.insert(id);
        let fallback = match &kind {
            ElementKind::Column(_) => "Column",
            ElementKind::Beam(_) => "Beam",
            ElementKind::Roof(_) => "Roof",
            ElementKind::Stair(_) => "Stair",
            ElementKind::Part(_) => "Sweep",
            ElementKind::Railing(_) => "Railing",
            _ => "Railing",
        };
        let element = Element {
            mark: None,
            source: None,
            of_type: self.type_ref(id),
            id: self.new_id(e),
            name: Self::name(e, fallback),
            kind,
        };
        // a railing may follow a stair, so it comes after them
        if matches!(element.kind, ElementKind::Railing(_)) {
            self.opening_elems.push(element);
        } else {
            self.other_elems.push(element);
        }
    }

    /// The imported revolution as a native part (shared construction).
    fn part_member(&mut self, id: u64, e: &Entity, s: RevolvedSweep) -> ElementKind {
        self.swept_part(id, e, s)
    }

    /// The member CreaBIM wrote in the element's property set, on the
    /// element's storey and in its material as the file has them now.
    fn member_definition(&mut self, id: u64) -> Option<ElementKind> {
        let props = self.props(id, MEMBER_PSET);
        let json = props.get("Definition")?.1.clone()?;
        let mut kind: ElementKind = match serde_json::from_str(&json) {
            Ok(k) => k,
            Err(err) => {
                self.note(
                    id,
                    "members whose CreaBIM definition could not be read were read from their shape",
                    format!("has a CreaBIM definition that could not be read ({err})"),
                );
                return None;
            }
        };
        let (level, _) = self.level_for(id, 0.0);
        let material = self.material_for(id);
        match &mut kind {
            ElementKind::Column(c) => (c.level, c.material) = (level, material),
            ElementKind::Beam(b) => (b.level, b.material) = (level, material),
            ElementKind::Stair(s) => (s.level, s.material) = (level, material),
            ElementKind::Railing(r) => {
                (r.level, r.material) = (level, material);
                // the stair it follows keeps its id from its GlobalId
                if r.stair.is_some_and(|s| !self.used_ids.contains(&s.stair)) {
                    return None;
                }
            }
            ElementKind::Roof(r) => {
                let layered = self.assembly_for(id, creabim_core::AssemblyUse::Roof);
                r.level = level;
                r.assembly = layered.map(|a| a.0);
                r.material = match layered {
                    Some((a, _)) => self.core_material(a),
                    None => material,
                };
            }
            _ => return None,
        }
        Some(kind)
    }

    /// A column from an upright prism with a rectangular or round section.
    fn native_column(
        &mut self,
        id: u64,
        e: &crate::step_read::Entity,
        world: &DMat4,
    ) -> Result<Column, String> {
        let shape = match self.prism(e, world).filter(|p| !p.clipped) {
            Some(p) => brep::SlabShape {
                outline: remove_collinear(&p.footprint),
                holes: p.holes,
                z0: p.z0,
                z1: p.z1,
            },
            None => brep::slab_of_mesh(&self.g.product_mesh(e, world))?,
        };
        if !shape.holes.is_empty() {
            return Err("is hollow".into());
        }
        let (position, profile, rotation) = brep::column_section(&shape.outline)?;
        let (level, elev) = self.level_for(id, shape.z0);
        Ok(Column {
            rotation,
            base_offset: shape.z0 - elev,
            height: Some(shape.z1 - shape.z0),
            material: self.material_for(id),
            section: None,
            ..Column::new(level, position, profile)
        })
    }

    /// A beam from a level bar with a rectangular section, its top
    /// measured down from the top of its storey.
    fn native_beam(
        &mut self,
        id: u64,
        e: &crate::step_read::Entity,
        world: &DMat4,
    ) -> Result<Beam, String> {
        let mesh = self.g.product_mesh(e, world);
        let (start, end, width, depth, top) = brep::beam_of_mesh(&mesh)?;
        let (level, elev) = self.level_for(id, top - depth);
        let storey_top = self
            .levels
            .iter()
            .map(|l| l.0)
            .filter(|z| *z > elev + 1e-6)
            .fold(f64::INFINITY, f64::min);
        let storey_top = if storey_top.is_finite() {
            storey_top
        } else {
            elev + self.level_height(level)
        };
        Ok(Beam {
            width,
            depth,
            top_offset: storey_top - top,
            material: self.material_for(id),
            section: None,
            ..Beam::new(level, start, end)
        })
    }

    /// A level's own height, as the import gave it.
    fn level_height(&self, level: creabim_core::ElementId) -> f64 {
        self.level_elems
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Level(l) if e.id == level => Some(l.height),
                _ => None,
            })
            .unwrap_or(3.0)
    }
}
