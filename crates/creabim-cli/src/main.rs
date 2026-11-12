//! `creabim` command-line tool: build the sample model and export it.
//!
//! Every command that reads a project takes either a `.creabim` database or
//! a legacy `.creabim.json` file, chosen by extension.

use creabim_core::{sample::sample_house, starter::starter_house, Document, Model};
use creabim_geom::sheet::{sheet_svg, sheets, sheets_pdf, SheetInfo};
use creabim_ifc::{export_ifc4, export_ifcx, import_ifc, import_ifcx, ExportOptions, ImportResult};
use std::process::ExitCode;

const USAGE: &str = "\
usage: (<model> is a .creabim project or a legacy .creabim.json file)
  creabim sample <out.creabim|out.creabim.json>
                                       write the starter house, with its sheets
  creabim sample-simple <out.creabim.json>
                                       the simple box house the tests use
  creabim sample-layered <out.creabim.json>
                                       the sample house built of layered assemblies
  creabim sample-typed <out.creabim.json>
                                       the sample house built of element types,
                                       with a curtain wall, openings and a shaft
  creabim sample-furnished <out.creabim.json>
                                       the sample house furnished from the
                                       starter catalogue
  creabim sample-garden <out.creabim.json>
                                       the sample house in its garden: a
                                       chimney, a screen wall, terrain,
                                       paving, planting and a car
  creabim catalogue [out-dir]          list the starter catalogue, or write
                                       each object as a catalogue file
  creabim catalogue-import <file> <dir>
                                       read a .blend, .FCStd, .gltf or .glb
                                       file as one catalogue object into dir
  creabim build <spec.json> <out>      build a model from a JSON description
                                       (docs/json-format.md)
  creabim build --check <spec.json>    check a JSON description, reporting each
                                       problem at its path, without saving
  creabim export-json <model> <out.json>
                                       describe a model in that JSON format
  creabim json-schema [out.json]       print or save the format's JSON Schema
  creabim save-db <in.creabim.json> <out.creabim>
                                       convert a JSON model to a project file
  creabim xlsx <model> <out.xlsx>      export the schedules to Excel
  creabim ifc <model> <out.ifc>        export a model to IFC4
  creabim import-ifc <in.ifc> <out>    import an IFC2X3, IFC4 or IFC4X3 file
  creabim ifcx <model> <out.ifcx>      export to IFC5 IFCX JSON (experimental)
  creabim import-ifcx <in.ifcx> <out>  import an IFC5 IFCX file (experimental)
  creabim plan <model> <level-name> <out.svg>
                                       draw a level's plan at 1:100
  creabim view <model> <view-name> <out.svg>
                                       draw any view (plan, section, elevation)
  creabim views <model>                list the model's views
  creabim label-clashes <model>        list labels, tags and notes that still
                                       overlap in each plan
  creabim sheets <model>               list the model's sheets
  creabim sheet <model> <number> <out.svg>
                                       draw one sheet
  creabim pdf <model> <out.pdf>        export every sheet to one PDF
  creabim apply-style <model> <pack> [--out <model>] [--pdf <out.pdf>]
                                       load a style pack (a .json file, or a
                                       shipped one by name: Standard, Light
                                       linework) into the project and make it
                                       the default; saved in place unless
                                       --out names another file
  creabim style-packs [out-dir]        list the shipped style packs, or write
                                       each as a .json file to start one from
  creabim schedule <model> <doors|windows|walls|slabs|rooms> [out.csv]
                                       print or save a schedule as CSV
  creabim script list <model>          list the model's script blocks
  creabim script run <model> <name|id> [--out <model>] [--param k=v ...]
                                       run a script block, committing its
                                       outputs as one undoable step
  creabim stats <model>                count elements by IFC class
  creabim check <model>                list objects standing in doorways or in
                                       a door's swing, and wall layers moved
                                       by hand into a gap or an overlap at a
                                       join; fails if there are any
  creabim rooms-from-notes <model> <out>
                                       turn room labels written as notes
                                       (a name over an area) into rooms
  creabim glb <model> <out.glb>        export glTF for Blender, with element ids
  creabim fcstd <model> <out.FCStd>    export a FreeCAD document
  creabim link <model> <file.blend|file.FCStd> <out-model>
                                       bring in (or reload) a linked file's objects";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["sample", out] if is_json(out) => {
            std::fs::write(out, starter_house().to_json()).map_err(|e| e.to_string())
        }
        ["sample-simple", out] if is_json(out) => {
            std::fs::write(out, sample_house().to_json()).map_err(|e| e.to_string())
        }
        ["sample-layered", out] if is_json(out) => {
            std::fs::write(out, creabim_core::sample::layered_house().to_json())
                .map_err(|e| e.to_string())
        }
        ["sample-typed", out] if is_json(out) => {
            std::fs::write(out, creabim_core::sample::typed_house().to_json())
                .map_err(|e| e.to_string())
        }
        ["sample-furnished", out] if is_json(out) => {
            std::fs::write(out, creabim_core::sample::furnished_house().to_json())
                .map_err(|e| e.to_string())
        }
        ["sample-garden", out] if is_json(out) => {
            std::fs::write(out, creabim_core::garden::garden_house().to_json())
                .map_err(|e| e.to_string())
        }
        ["catalogue"] => {
            for c in creabim_core::catalogue::starters() {
                let s = c.object.size();
                println!(
                    "{:10} {:28} {:.2} × {:.2} × {:.2} m  {} {}",
                    c.object.category.label(),
                    c.name,
                    s.x,
                    s.y,
                    s.z,
                    c.object.ifc.class,
                    c.object.ifc.predefined
                );
            }
            Ok(())
        }
        ["catalogue-import", file, dir] => {
            let path = std::path::Path::new(file);
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("object")
                .to_lowercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
            creabim_exchange::catalogue::read_file(path, &format!("user/{stem}"))
                .map_err(|e| e.to_string())
                .and_then(|c| {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                    let out = std::path::Path::new(dir).join(format!("{stem}.json"));
                    std::fs::write(&out, c.to_json()).map_err(|e| e.to_string())?;
                    let s = c.object.size();
                    println!(
                        "{}: {} parts, {:.2} × {:.2} × {:.2} m, {} {} → {}",
                        c.name,
                        c.object.parts.len(),
                        s.x,
                        s.y,
                        s.z,
                        c.object.ifc.class,
                        c.object.ifc.predefined,
                        out.display()
                    );
                    Ok(())
                })
        }
        ["catalogue", dir] => std::fs::create_dir_all(dir)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                for c in creabim_core::catalogue::starters() {
                    let name = c.key.rsplit('/').next().unwrap_or("object");
                    let path = std::path::Path::new(dir).join(format!("{name}.json"));
                    std::fs::write(&path, c.to_json()).map_err(|e| e.to_string())?;
                }
                Ok(())
            }),
        ["sample", out] => creabim_store::save(out, &Document::from_model(starter_house()))
            .map_err(|e| format!("{out}: {e}")),
        ["build", "--check", spec] | ["build", spec, "--check"] => build_spec(spec, None),
        ["build", spec, out] => build_spec(spec, Some(out)),
        ["export-json", input, out] => load(input).and_then(|m| {
            let x = creabim_spec::export(&m);
            for s in &x.skipped {
                eprintln!("left out: {s}");
            }
            std::fs::write(out, creabim_spec::to_text(&x.spec)).map_err(|e| format!("{out}: {e}"))
        }),
        ["json-schema", rest @ ..] if rest.len() <= 1 => {
            let text = creabim_spec::schema().pretty();
            match rest {
                [out] => std::fs::write(out, text).map_err(|e| format!("{out}: {e}")),
                _ => {
                    print!("{text}");
                    Ok(())
                }
            }
        }
        ["save-db", input, out] => creabim_store::import_json(input)
            .map_err(|e| format!("{input}: {e}"))
            .and_then(|doc| creabim_store::save(out, &doc).map_err(|e| format!("{out}: {e}"))),
        ["xlsx", input, out] => load(input).and_then(|m| {
            let project = std::path::Path::new(input)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.trim_end_matches(".creabim").to_string())
                .unwrap_or_default();
            creabim_geom::xlsx::schedules_xlsx(&m, &project)
                .and_then(|b| std::fs::write(out, b).map_err(|e| format!("{out}: {e}")))
        }),
        ["glb", input, out] => load(input).and_then(|m| {
            std::fs::write(out, creabim_exchange::gltf::export_glb(&m)).map_err(|e| e.to_string())
        }),
        ["fcstd", input, out] => load(input).and_then(|m| {
            let label = std::path::Path::new(out)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            creabim_exchange::freecad::export_fcstd(&m, &label)
                .map_err(|e| e.to_string())
                .and_then(|b| std::fs::write(out, b).map_err(|e| e.to_string()))
        }),
        ["link", input, linked, out] => load(input).and_then(|m| {
            let file = std::fs::canonicalize(linked)
                .map_err(|e| format!("{linked}: {e}"))?
                .display()
                .to_string();
            let path = std::path::Path::new(&file);
            let lower = file.to_ascii_lowercase();
            let incoming = if lower.ends_with(".fcstd") {
                creabim_exchange::freecad::read_file(path)
            } else {
                creabim_exchange::blend::read_file(path)
            }
            .map_err(|e| e.to_string())?;
            let (tx, report) = creabim_exchange::sync(&m, &file, &incoming);
            let mut doc = Document::from_model(m);
            doc.commit(tx).map_err(|e| e.to_string())?;
            println!("{}", report.summary());
            for n in &report.notes {
                println!("  {n}");
            }
            if is_json(out) {
                std::fs::write(out, doc.model().to_json()).map_err(|e| e.to_string())
            } else {
                creabim_store::save(out, &doc).map_err(|e| format!("{out}: {e}"))
            }
        }),
        ["ifc", input, out] => load(input).and_then(|m| {
            let name = std::path::Path::new(out)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "CreaBIM Project".into());
            let opts = ExportOptions::for_model(&m, &name);
            std::fs::write(out, export_ifc4(&m, &opts)).map_err(|e| e.to_string())
        }),
        ["plan", input, level, out] => load(input).and_then(|m| {
            let (le, _) = m
                .levels()
                .find(|(e, _)| e.name == *level)
                .ok_or_else(|| format!("no level named {level:?}"))?;
            let svg = creabim_geom::plan_view(&m, le.id, 1.2).to_svg(100.0);
            std::fs::write(out, svg).map_err(|e| e.to_string())
        }),
        ["view", input, name, out] => load(input).and_then(|m| {
            let e = m
                .iter()
                .find(|e| e.name == *name && matches!(e.kind, creabim_core::ElementKind::View(_)))
                .ok_or_else(|| format!("no view named {name:?}"))?;
            let scale = match &e.kind {
                creabim_core::ElementKind::View(v) => v.paper_scale(),
                _ => 100.0,
            };
            let d = creabim_geom::section::view_drawing(&m, e.id).expect("is a view");
            std::fs::write(out, d.to_svg(scale)).map_err(|e| e.to_string())
        }),
        ["views", input] => load(input).map(|m| {
            for e in m.iter() {
                if let creabim_core::ElementKind::View(v) = &e.kind {
                    let kind = match v.kind {
                        creabim_core::ViewKind::Plan { .. } => "plan",
                        creabim_core::ViewKind::Section { cut: true, .. } => "section",
                        creabim_core::ViewKind::Section { cut: false, .. } => "elevation",
                        creabim_core::ViewKind::Axonometric { .. } => "3d",
                        creabim_core::ViewKind::Schedule { .. } => "schedule",
                    };
                    println!("{kind:10} 1:{:<5} {}", v.scale, e.name);
                }
            }
        }),
        ["label-clashes", input] => load(input).map(|m| {
            for e in m.iter() {
                let creabim_core::ElementKind::View(v) = &e.kind else {
                    continue;
                };
                if !matches!(v.kind, creabim_core::ViewKind::Plan { .. }) {
                    continue;
                }
                let Some(d) = creabim_geom::section::view_drawing(&m, e.id) else {
                    continue;
                };
                let mm = v.scale / 1000.0;
                let clashes = creabim_geom::declutter::overlapping_labels(&d.items, mm);
                println!("{}: {} overlapping", e.name, clashes.len());
                for (a, b) in clashes {
                    let text = |k: usize| creabim_geom::declutter::label_text(&d.items[k]);
                    println!("  {:?} and {:?}", text(a), text(b));
                }
            }
        }),
        ["sheets", input] => load(input).map(|m| {
            for (id, s) in sheets(&m) {
                let name = m.get(id).map(|e| e.name.as_str()).unwrap_or("");
                let size = s.size_name().unwrap_or("custom");
                println!("{:8} {size:6} {name}", s.number);
            }
        }),
        ["sheet", input, number, out] => load(input).and_then(|m| {
            let (id, _) = sheets(&m)
                .into_iter()
                .find(|(_, s)| s.number == *number)
                .ok_or_else(|| format!("no sheet numbered {number:?}"))?;
            let svg = sheet_svg(&m, id, &info(input)).expect("is a sheet");
            std::fs::write(out, svg).map_err(|e| e.to_string())
        }),
        ["pdf", input, out] => load(input).and_then(|m| {
            if sheets(&m).is_empty() {
                return Err("the model has no sheets".into());
            }
            std::fs::write(out, sheets_pdf(&m, &info(input))).map_err(|e| e.to_string())
        }),
        ["apply-style", input, pack, rest @ ..] => apply_style(input, pack, rest),
        ["style-packs"] => {
            for p in creabim_core::styles::StylePack::shipped() {
                println!("{:16} {}", p.name, p.description);
            }
            Ok(())
        }
        ["style-packs", dir] => std::fs::create_dir_all(dir)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                for p in creabim_core::styles::StylePack::shipped() {
                    let name = p.name.to_lowercase().replace(' ', "-");
                    let path = std::path::Path::new(dir).join(format!("{name}.json"));
                    std::fs::write(&path, p.to_json()).map_err(|e| e.to_string())?;
                }
                Ok(())
            }),
        ["schedule", input, category, rest @ ..] if rest.len() <= 1 => load(input).and_then(|m| {
            let cat = creabim_core::ScheduleCategory::ALL
                .into_iter()
                .find(|c| c.label().eq_ignore_ascii_case(category))
                .ok_or_else(|| {
                    format!("no schedule of {category:?}; try doors, windows, walls, slabs, rooms, furniture or fixtures")
                })?;
            let csv = creabim_geom::schedule::to_csv(&creabim_geom::schedule::schedule(&m, cat));
            match rest {
                [out] => std::fs::write(out, csv).map_err(|e| e.to_string()),
                _ => {
                    print!("{csv}");
                    Ok(())
                }
            }
        }),
        ["import-ifc", input, out] => read_text(input)
            .and_then(|text| import_ifc(&text).map_err(|e| format!("{input}: {e}")))
            .and_then(|r| save_import(r, out)),
        ["script", "list", input] => load(input).and_then(|m| {
            let mut n = 0;
            for e in m.iter() {
                if let creabim_core::ElementKind::ScriptObject(s) = &e.kind {
                    n += 1;
                    let status = match s.declared() {
                        Ok(d) => format!("{} inputs", d.len()),
                        Err(e) => format!("bad header: {e}"),
                    };
                    println!(
                        "{} {} engine={} {} outputs={}",
                        e.name,
                        e.id.0,
                        s.engine,
                        status,
                        s.outputs.len()
                    );
                }
            }
            if n == 0 {
                println!("no script blocks");
            }
            Ok(())
        }),
        ["script", "run", input, name, rest @ ..] => script_run(input, name, rest),
        ["ifcx", input, out] => load(input)
            .and_then(|m| std::fs::write(out, export_ifcx(&m)).map_err(|e| e.to_string())),
        ["import-ifcx", input, out] => read_text(input)
            .and_then(|text| import_ifcx(&text).map_err(|e| format!("{input}: {e}")))
            .and_then(|r| save_import(r, out)),
        ["stats", input] => load(input).map(|m| {
            let mut counts = std::collections::BTreeMap::new();
            for e in m.iter() {
                *counts.entry(e.ifc_class().to_string()).or_insert(0) += 1;
            }
            for (class, n) in counts {
                println!("{class:28} {n}");
            }
        }),
        ["check", input] => load(input).and_then(|m| {
            let clashes = creabim_core::checks::objects_in_openings(&m);
            for line in creabim_core::checks::report(&m, &clashes) {
                println!("{line}");
            }
            let layers = creabim_core::checks::layer_issues(&m);
            for line in creabim_core::checks::layer_report(&m, &layers) {
                println!("{line}");
            }
            match (clashes.len(), layers.len()) {
                (0, 0) => {
                    println!("no objects in doorways or door swings, no layer gaps or overlaps at joins");
                    Ok(())
                }
                (n, 0) => Err(format!("{n} objects in the way of doors or windows")),
                (0, 1) => Err("1 layer end leaves a gap or overlap at a join".into()),
                (0, k) => Err(format!("{k} layer ends leave a gap or overlap at a join")),
                (n, k) => Err(format!(
                    "{n} objects in the way of doors or windows, {k} layer ends leave a gap or overlap at a join"
                )),
            }
        }),
        ["rooms-from-notes", input, out] => creabim_store::load(input)
            .map_err(|e| format!("{input}: {e}"))
            .and_then(|mut doc| rooms_from_notes(&mut doc, out)),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn info(path: &str) -> SheetInfo {
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    SheetInfo {
        project: name
            .split('.')
            .next()
            .unwrap_or("CreaBIM project")
            .to_string(),
        date: creabim_geom::sheet::today(),
    }
}

/// Read a text file; IFC files are ASCII by the standard, but some tools
/// write Latin-1 or UTF-8 into strings, so invalid bytes are replaced, not fatal.
fn read_text(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Save an imported model in the format `out` names, reporting what was skipped.
fn save_import(r: ImportResult, out: &str) -> Result<(), String> {
    for w in &r.warnings {
        eprintln!("warning: {w}");
    }
    if !r.details.is_empty() {
        eprintln!("details, element by element:");
        for d in &r.details {
            eprintln!("  {d}");
        }
    }
    // native elements and meshes apart: "13 IfcWall (6 meshes)"
    let mut counts = std::collections::BTreeMap::new();
    for e in r.model.iter() {
        let c: &mut (usize, usize) = counts.entry(e.ifc_class().to_string()).or_default();
        c.0 += 1;
        c.1 += usize::from(matches!(e.kind, creabim_core::ElementKind::FreeMesh(_)));
    }
    let summary: Vec<String> = counts
        .iter()
        .map(|(c, (n, meshes))| match meshes {
            0 => format!("{n} {c}"),
            m if m == n => format!("{n} {c} (meshes)"),
            1 => format!("{n} {c} (1 mesh)"),
            m => format!("{n} {c} ({m} meshes)"),
        })
        .collect();
    eprintln!("imported {} ({})", r.schema, summary.join(", "));
    if is_json(out) {
        std::fs::write(out, r.model.to_json()).map_err(|e| e.to_string())
    } else {
        creabim_store::save(out, &Document::from_model(r.model)).map_err(|e| format!("{out}: {e}"))
    }
}

/// Run a script block, committing its outputs as one undoable step.
fn script_run(input: &str, name: &str, rest: &[&str]) -> Result<(), String> {
    let mut out = input.to_string();
    let mut params: Vec<(String, String)> = vec![];
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match *flag {
            "--out" => {
                out = it
                    .next()
                    .ok_or_else(|| "--out needs a value".to_string())?
                    .to_string()
            }
            "--param" => {
                let kv = it
                    .next()
                    .ok_or_else(|| "--param needs key=value".to_string())?;
                let (k, v) = kv
                    .split_once('=')
                    .ok_or_else(|| format!("--param {kv:?} is not key=value"))?;
                params.push((k.to_string(), v.to_string()));
            }
            other => return Err(format!("unknown flag {other:?}")),
        }
    }
    let model = load(input)?;
    let found = model
        .iter()
        .find(|e| e.name == *name || e.id.0.to_string() == *name)
        .ok_or_else(|| format!("no script block named {name:?}"))?;
    let script_id = found.id;
    let script = match &found.kind {
        creabim_core::ElementKind::ScriptObject(s) => s.clone(),
        _ => return Err(format!("{name:?} is not a script block")),
    };
    let decls = script
        .declared()
        .map_err(|e| format!("{}: {e}", found.name))?;
    let home = creabim_script::ensure_provisioned().map_err(|e| e.to_string())?;
    creabim_script::ensure_packages(&["numpy"]).map_err(|e| e.to_string())?;
    let engine = creabim_script::CPythonEngine::new(home);
    let inputs = creabim_script::resolve(&model, &decls, &script.inputs, &params)
        .map_err(|e| e.to_string())?;
    let label = format!("script {}", found.name);
    let (tx, _) = creabim_script::run_to_transaction(
        &engine,
        &model,
        script_id,
        &script,
        &inputs,
        &label,
    )
    .map_err(|e| e.to_string())?;
    let (mut added, mut replaced, mut removed) = (0, 0, 0);
    for op in &tx.ops {
        match op {
            creabim_core::Op::Insert(_) => added += 1,
            creabim_core::Op::Replace { .. } => replaced += 1,
            creabim_core::Op::Remove(_) => removed += 1,
        }
    }
    let mut doc = Document::from_model(model);
    doc.commit(tx).map_err(|e| e.to_string())?;
    if is_json(&out) {
        std::fs::write(&out, doc.model().to_json()).map_err(|e| format!("{out}: {e}"))
    } else {
        creabim_store::save(&out, &doc).map_err(|e| format!("{out}: {e}"))
    }?;
    println!("{label}: {added} added, {replaced} replaced, {removed} removed");
    Ok(())
}

/// Turn the room labels written as notes into rooms, and save the result.
fn rooms_from_notes(doc: &mut Document, out: &str) -> Result<(), String> {
    let (tx, skipped) = creabim_core::room::from_notes(doc.model());
    let made = tx
        .ops
        .iter()
        .filter(|o| {
            matches!(o, creabim_core::Op::Insert(e)
                if matches!(e.kind, creabim_core::ElementKind::Room(_)))
        })
        .count();
    if made > 0 {
        doc.commit(tx).map_err(|e| e.to_string())?;
    }
    println!("{made} rooms made from notes");
    for name in skipped {
        println!("  left as a note (no walls close it off, or a room is there): {name}");
    }
    if is_json(out) {
        std::fs::write(out, doc.model().to_json()).map_err(|e| e.to_string())
    } else {
        creabim_store::save(out, doc).map_err(|e| format!("{out}: {e}"))
    }
}

/// `apply-style`: the pack into the project, one undo step, then saved and
/// the sheets exported with it.
fn apply_style(input: &str, pack: &str, rest: &[&str]) -> Result<(), String> {
    use creabim_core::styles::{apply_pack, StylePack};
    let (mut out, mut pdf) = (input.to_string(), None);
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match *flag {
            "--out" => out = value.to_string(),
            "--pdf" => pdf = Some(value.to_string()),
            _ => return Err(format!("unknown option {flag}\n{USAGE}")),
        }
    }
    let pack = match StylePack::shipped()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(pack))
    {
        Some(p) => p,
        None => std::fs::read_to_string(pack)
            .map_err(|e| format!("{pack}: {e}"))
            .and_then(|t| StylePack::from_json(&t).map_err(|e| format!("{pack}: {e}")))?,
    };
    let mut doc = if is_json(input) {
        creabim_store::import_json(input).map_err(|e| format!("{input}: {e}"))?
    } else {
        creabim_store::load(input).map_err(|e| format!("{input}: {e}"))?
    };
    doc.commit(apply_pack(doc.model(), &pack))
        .map_err(|e| e.to_string())?;
    if is_json(&out) {
        std::fs::write(&out, doc.model().to_json()).map_err(|e| e.to_string())?;
    } else {
        creabim_store::save(&out, &doc).map_err(|e| format!("{out}: {e}"))?;
    }
    println!("{}: style pack {} applied", out, pack.name);
    if let Some(pdf) = pdf {
        if sheets(doc.model()).is_empty() {
            return Err("the model has no sheets".into());
        }
        std::fs::write(&pdf, sheets_pdf(doc.model(), &info(&out))).map_err(|e| e.to_string())?;
        println!("{pdf}: sheets exported");
    }
    Ok(())
}

/// Build a model from a JSON description (or only check it, with no
/// `out`), reporting each problem at its path in the file.
fn build_spec(spec: &str, out: Option<&str>) -> Result<(), String> {
    let text = std::fs::read_to_string(spec).map_err(|e| format!("{spec}: {e}"))?;
    let report = |problems: Vec<creabim_spec::Problem>| {
        for p in &problems {
            eprintln!("{spec}: {p}");
        }
        match problems.len() {
            1 => "1 problem; nothing was built".to_string(),
            n => format!("{n} problems; nothing was built"),
        }
    };
    let parsed = creabim_spec::parse(&text).map_err(report)?;
    let built = creabim_spec::build(&parsed).map_err(report)?;
    for w in &built.warnings {
        eprintln!("{spec}: warning: {w}");
    }
    let m = &built.model;
    let mut counts = std::collections::BTreeMap::new();
    for e in m.iter() {
        *counts.entry(e.kind.name()).or_insert(0) += 1;
    }
    let summary: Vec<String> = counts.iter().map(|(k, n)| format!("{n} {k}")).collect();
    println!("{} elements: {}", m.len(), summary.join(", "));
    match out {
        None => {
            println!("{spec} is valid");
            Ok(())
        }
        Some(out) if is_json(out) => {
            std::fs::write(out, m.to_json()).map_err(|e| format!("{out}: {e}"))
        }
        Some(out) => creabim_store::save(out, &Document::from_model(built.model))
            .map_err(|e| format!("{out}: {e}")),
    }
}

fn is_json(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".json")
}

/// Read a project in either format; the history is not needed here.
fn load(path: &str) -> Result<Model, String> {
    creabim_store::load(path)
        .map(|doc| doc.into_parts().0)
        .map_err(|e| format!("{path}: {e}"))
}
