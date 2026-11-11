//! Extrusions, sweeps, voids, screen walls and site elements in IFC: the
//! classes they export as, voids as IfcOpeningElements in what they cut,
//! and a native round trip through IFCX.

use creabim_core::garden::garden_house;
use creabim_core::parts::Path;
use creabim_core::{Element, ElementKind, Model, Part, Transaction};
use creabim_ifc::{export_ifc4, export_ifcx, import_ifc, import_ifcx, ExportOptions};
use glam::{DVec2, DVec3};

fn natives(m: &Model) -> Vec<Element> {
    let mut v: Vec<Element> = m
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Part(_) | ElementKind::Site(_)))
        .cloned()
        .collect();
    v.sort_by_key(|e| e.id.0);
    v
}

fn count(text: &str, entity: &str) -> usize {
    text.lines().filter(|l| l.contains(entity)).count()
}

#[test]
fn parts_and_site_export_as_their_classes() {
    let m = garden_house();
    let text = export_ifc4(&m, &ExportOptions::default());
    // the stack as the class picked for it, the hearth as a proxy
    assert!(text
        .lines()
        .any(|l| l.contains("IFCCHIMNEY(") && l.contains("'Chimney stack'")));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCBUILDINGELEMENTPROXY(") && l.contains("'Hearth'")));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCCOVERING(") && l.contains("'Coping'")));
    // terrain, plants and hard landscape
    assert!(text
        .lines()
        .any(|l| l.contains("IFCGEOGRAPHICELEMENT(") && l.contains(".TERRAIN.")));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCGEOGRAPHICELEMENT(") && l.contains("'Oak'")));
    assert!(text
        .lines()
        .any(|l| l.contains("IFCCIVILELEMENT(") && l.contains("'Patio'")));
    // the firebox is no element of its own: one opening in each thing it cuts
    assert!(!text.contains("'Firebox'"));
    let firebox_openings = text
        .lines()
        .filter(|l| l.contains("IFCOPENINGELEMENT(") && l.contains("'Firebox in "))
        .count();
    assert_eq!(firebox_openings, 2);
    assert!(count(&text, "IFCRELVOIDSELEMENT(") >= 2);
}

#[test]
fn the_step_file_reads_back() {
    let m = garden_house();
    let r = import_ifc(&export_ifc4(&m, &ExportOptions::default())).unwrap();
    // without a native property set yet the parts come back as shapes,
    // but nothing is lost
    for name in [
        "Chimney stack",
        "Coping",
        "Ground",
        "Patio",
        "Oak",
        "Garden screen",
    ] {
        assert!(
            r.model.iter().any(|e| e.name == name),
            "{name}: {:?}",
            r.warnings
        );
    }
}

#[test]
fn parts_and_site_round_trip_natively_through_ifcx() {
    let m = garden_house();
    let r = import_ifcx(&export_ifcx(&m)).unwrap();
    let (before, after) = (natives(&m), natives(&r.model));
    assert_eq!(before.len(), after.len(), "{:?}", r.warnings);
    for (a, b) in before.iter().zip(&after) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.kind, b.kind, "{} comes back as the same element", a.name);
    }
    // the coping still follows its wall, and the void still cuts
    let cuts = creabim_core::parts::void_cuts(&r.model);
    assert_eq!(cuts.len(), creabim_core::parts::void_cuts(&m).len());
    let screen = r.model.iter().find(|e| e.name == "Garden screen").unwrap();
    assert!(creabim_core::screen::is_screen(&r.model, screen.id));
}

/// An arc sweep bent in plan, one in elevation turning back, both as beams.
fn with_arc_sweeps() -> Model {
    use creabim_core::sample::sample_house;
    let mut m = sample_house();
    let level = m
        .levels()
        .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
        .unwrap()
        .0
        .id;
    // counter-clockwise, like an authored profile
    let section = vec![
        DVec2::new(-0.1, -0.15),
        DVec2::new(0.1, -0.15),
        DVec2::new(0.1, 0.15),
        DVec2::new(-0.1, 0.15),
    ];
    let mut ring = Part::sweep(
        level,
        section.clone(),
        Path::Arc {
            center: DVec3::new(0.0, 0.0, 0.5),
            axis: DVec3::Z,
            start: DVec3::new(5.0, 0.0, 0.5),
            sweep: std::f64::consts::FRAC_PI_2,
        },
    );
    ring.ifc_class = "IfcBeam".into();
    let mut band = Part::sweep(
        level,
        section,
        Path::Arc {
            center: DVec3::new(0.0, 0.0, 3.0),
            axis: DVec3::Y,
            start: DVec3::new(4.0, 0.0, 0.0),
            sweep: -std::f64::consts::PI,
        },
    );
    band.ifc_class = "IfcBeam".into();
    Transaction::new("arc sweeps")
        .insert(Element::new("Ring beam", ElementKind::Part(ring)))
        .insert(Element::new("Band", ElementKind::Part(band)))
        .apply(&mut m)
        .unwrap();
    m
}

fn arc_of(e: &Element) -> (DVec3, DVec3, DVec3, f64, Vec<DVec2>) {
    let ElementKind::Part(p) = &e.kind else {
        panic!("{} is no part", e.name);
    };
    let creabim_core::parts::Form::Sweep {
        profile,
        path:
            Path::Arc {
                center,
                axis,
                start,
                sweep,
            },
    } = &p.form
    else {
        panic!("{} is no arc sweep", e.name);
    };
    (*center, *axis, *start, *sweep, profile.clone())
}

#[test]
fn arc_sweeps_export_as_revolved_solids_and_come_back_native() {
    use creabim_core::sample::sample_house;
    let m = with_arc_sweeps();
    let text = export_ifc4(&m, &ExportOptions::default());
    assert_eq!(count(&text, "IFCREVOLVEDAREASOLID("), 2);
    // the arcs add no tessellation: same faceset count as the bare house
    let bare = export_ifc4(&sample_house(), &ExportOptions::default());
    assert_eq!(
        count(&text, "IFCTRIANGULATEDFACESET("),
        count(&bare, "IFCTRIANGULATEDFACESET(")
    );
    let r = import_ifc(&text).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    for name in ["Ring beam", "Band"] {
        let before = m.iter().find(|e| e.name == name).unwrap();
        let after = r.model.iter().find(|e| e.name == name).unwrap();
        // the same element: GlobalId stable, still a beam
        assert_eq!(after.id.ifc_guid(), before.id.ifc_guid());
        assert_eq!(after.ifc_class(), "IfcBeam");
        let (bc, ba, bs, bw, bp) = arc_of(before);
        let (ac, aa, as_, aw, ap) = arc_of(after);
        assert!((ac - bc).length() < 1e-6, "{ac} {bc}");
        assert!((aa - ba).length() < 1e-9, "{aa} {ba}");
        assert!((as_ - bs).length() < 1e-6, "{as_} {bs}");
        assert!((aw - bw).abs() < 1e-9, "{aw} {bw}");
        assert_eq!(ap.len(), bp.len());
        for (a, b) in ap.iter().zip(&bp) {
            assert!((a - b).length() < 1e-6, "{a} {b}");
        }
    }
}

/// An upright L-column and a tilted L-beam: profiles no member kind builds.
fn with_extruded_parts() -> Model {
    use creabim_core::sample::sample_house;
    use creabim_core::Workplane;
    let mut m = sample_house();
    let level = m
        .levels()
        .min_by(|a, b| a.1.elevation.total_cmp(&b.1.elevation))
        .unwrap()
        .0
        .id;
    // counter-clockwise L, like an authored outline
    let angle = vec![
        DVec2::new(0.0, 0.0),
        DVec2::new(0.08, 0.0),
        DVec2::new(0.08, 0.008),
        DVec2::new(0.008, 0.008),
        DVec2::new(0.008, 0.08),
        DVec2::new(0.0, 0.08),
    ];
    let mut column = Part::extrusion(level, angle.clone(), 0.0, 2.75);
    column.ifc_class = "IfcColumn".into();
    let n = DVec3::new(0.0, -0.5, 0.8660254);
    let x = DVec3::X;
    let mut raked = Part::extrusion(level, angle, 0.0, 2.0);
    if let creabim_core::parts::Form::Extrusion { plane, .. } = &mut raked.form {
        *plane = Some(Workplane {
            origin: DVec3::new(1.0, 2.0, 0.5),
            x,
            y: n.cross(x),
        });
    }
    raked.ifc_class = "IfcBeam".into();
    Transaction::new("extruded parts")
        .insert(Element::new("Angle column", ElementKind::Part(column)))
        .insert(Element::new("Raked angle", ElementKind::Part(raked)))
        .apply(&mut m)
        .unwrap();
    m
}

type Extrusion = (Vec<DVec2>, f64, f64, Option<(DVec3, DVec3, DVec3)>);

fn extrusion_of(e: &Element) -> Extrusion {
    let ElementKind::Part(p) = &e.kind else {
        panic!("{} is no part", e.name);
    };
    let creabim_core::parts::Form::Extrusion {
        outline,
        holes,
        base,
        height,
        plane,
    } = &p.form
    else {
        panic!("{} is no extrusion", e.name);
    };
    assert!(holes.is_empty());
    (
        outline.clone(),
        *base,
        *height,
        plane.map(|pl| (pl.origin, pl.x, pl.y)),
    )
}

#[test]
fn extrusions_export_as_extruded_solids_and_come_back_native() {
    use creabim_core::sample::sample_house;
    let m = with_extruded_parts();
    let text = export_ifc4(&m, &ExportOptions::default());
    let bare = export_ifc4(&sample_house(), &ExportOptions::default());
    // the two parts add two extrusions and no tessellation
    assert_eq!(
        count(&text, "IFCEXTRUDEDAREASOLID("),
        count(&bare, "IFCEXTRUDEDAREASOLID(") + 2
    );
    assert_eq!(
        count(&text, "IFCTRIANGULATEDFACESET("),
        count(&bare, "IFCTRIANGULATEDFACESET(")
    );
    let r = import_ifc(&text).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    for (name, class) in [("Angle column", "IfcColumn"), ("Raked angle", "IfcBeam")] {
        let before = m.iter().find(|e| e.name == name).unwrap();
        let after = r.model.iter().find(|e| e.name == name).unwrap();
        assert_eq!(after.id.ifc_guid(), before.id.ifc_guid());
        assert_eq!(after.ifc_class(), class);
        let (bo, bb, bh, bp) = extrusion_of(before);
        let (ao, ab, ah, ap) = extrusion_of(after);
        assert_eq!(ao.len(), bo.len());
        for (a, b) in ao.iter().zip(&bo) {
            assert!((a - b).length() < 1e-6, "{a} {b}");
        }
        assert!((ab - bb).abs() < 1e-6, "{ab} {bb}");
        assert!((ah - bh).abs() < 1e-6, "{ah} {bh}");
        match (bp, ap) {
            (None, None) => {}
            (Some((bo_, bx, by)), Some((ao_, ax, ay))) => {
                assert!((ao_ - bo_).length() < 1e-6, "{ao_} {bo_}");
                // axes to STEP's 9-decimal precision
                assert!((ax - bx).length() < 1e-6, "{ax} {bx}");
                assert!((ay - by).length() < 1e-6, "{ay} {by}");
            }
            _ => panic!("{name} lost its plane"),
        }
    }
}
