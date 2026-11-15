//! Georeferencing round trip: a placed project exports
//! `IfcProjectedCRS` + `IfcMapConversion` (+ lat/long on the site) and
//! imports back to the same placement. Unplaced models stay clean.

use creabim_core::{Document, Element, ElementKind, Level, Model, ProjectInfo, Transaction, Wall};
use creabim_ifc::{export_ifc4, import_ifc, ExportOptions};
use glam::DVec2;

fn placed_model() -> Model {
    let mut doc = Document::from_model(Model::new());
    doc.commit(Transaction::new("level").insert(Element::new(
        "Ground",
        ElementKind::Level(Level {
            elevation: 0.0,
            height: 2.7,
        }),
    )))
    .unwrap();
    let level = doc
        .model()
        .levels()
        .find(|(e, _)| e.name == "Ground")
        .map(|(e, _)| e.id)
        .unwrap();
    doc.commit(Transaction::new("wall").insert(Element::new(
        "Wall",
        ElementKind::Wall(Wall::new(
            level,
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            0.1,
            2.7,
        )),
    )))
    .unwrap();
    let mut info = ProjectInfo {
        srid: Some("EPSG:28992".into()),
        easting: Some(136331.0),
        northing: Some(455986.0),
        true_north: 30.0,
        elevation: 1.5,
        latitude: Some(52.0907),
        longitude: Some(5.1214),
        ..Default::default()
    };
    info.name = "Georef house".into();
    doc.commit(creabim_core::set_project(doc.model(), info))
        .unwrap();
    doc.model().clone()
}

#[test]
fn placement_exports_map_conversion_and_lat_long() {
    let ifc = export_ifc4(&placed_model(), &ExportOptions::default());
    assert!(ifc.contains("IFCPROJECTEDCRS('EPSG:28992'"), "CRS travels");
    let conv = ifc
        .lines()
        .find(|l| l.contains("IFCMAPCONVERSION("))
        .expect("a map conversion");
    for probe in ["136331.", "455986.", "1.5", "0.8660254", "-0.5,"] {
        assert!(conv.contains(probe), "conversion carries {probe}: {conv}");
    }
    assert!(ifc.contains("(52,5,26,520000)"), "latitude travels as DMS");
    assert!(ifc.contains("(5,7,17,40000)"), "longitude travels as DMS");
}

#[test]
fn placement_round_trips_through_ifc() {
    let model = placed_model();
    let ifc = export_ifc4(&model, &ExportOptions::default());
    let back = import_ifc(&ifc).expect("valid IFC").model;
    let (_, p) = back.project_info().expect("placement survives");
    let g = p.placement().expect("complete placement");
    assert_eq!(g.srid, "EPSG:28992");
    assert!((g.easting - 136331.0).abs() < 1e-6);
    assert!((g.northing - 455986.0).abs() < 1e-6);
    assert!((g.height - 1.5).abs() < 1e-9);
    assert!((g.rotation_deg - 30.0).abs() < 1e-6);
    assert!((p.latitude.unwrap() - 52.0907).abs() < 1e-6);
    assert!((p.longitude.unwrap() - 5.1214).abs() < 1e-6);
    // The transform itself survives too.
    let fwd = model
        .project_info()
        .unwrap()
        .1
        .placement()
        .unwrap()
        .world_of(DVec2::new(10.0, 0.0));
    let back_fwd = g.world_of(DVec2::new(10.0, 0.0));
    assert!((fwd - back_fwd).length() < 1e-3);
}

#[test]
fn unplaced_models_export_no_geo() {
    let mut doc = Document::from_model(Model::new());
    doc.commit(Transaction::new("level").insert(Element::new(
        "Ground",
        ElementKind::Level(Level {
            elevation: 0.0,
            height: 2.7,
        }),
    )))
    .unwrap();
    let ifc = export_ifc4(doc.model(), &ExportOptions::default());
    assert!(
        !ifc.contains("IFCMAPCONVERSION"),
        "no conversion without placement"
    );
    assert!(!ifc.contains("IFCPROJECTEDCRS"), "no CRS without placement");
    let back = import_ifc(&ifc).expect("valid IFC").model;
    assert!(back.project_info().is_none(), "no phantom project info");
}
