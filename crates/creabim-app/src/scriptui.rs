//! Script block properties: inputs, source editor, run controls.
//!
//! M-S2: egui-native authoring. Parameter widgets are generated from
//! the script's front-matter header; the source editor is a multiline
//! field with a hand-rolled Python highlighter (no new dependencies).
//! Recompute itself lives in [`crate::scriptrun`]; this panel only
//! edits the element and pokes the runner.

use crate::scriptrun::{ScriptRunner, Status};
use creabim_core::{ElementId, ParamKind, ParamValue, ScriptObject};
use eframe::egui;
use std::sync::Arc;

/// Edit a script element's inputs and source; Run/Stop the runner.
/// Returns the undo label when the element changed. Linking and
/// embedding also touch the project's script registry, committed
/// straight to `doc`; the `s` edit itself commits through the label.
pub fn script_panel(
    ui: &mut egui::Ui,
    runner: &mut ScriptRunner,
    doc: &mut creabim_core::Document,
    id: ElementId,
    s: &mut ScriptObject,
) -> Option<&'static str> {
    let mut label = None;
    ui.label(format!("Engine: {}", s.engine));

    // Disc source of truth (M-S6): the block names a registry entry and
    // the registry maps it onto a file; edits in the panel are written
    // straight back to that file.
    let dir = runner.project_dir().map(|d| d.to_path_buf());
    let reg = creabim_core::registry_of(doc.model()).map(|(_, r)| r);
    match s.link.clone() {
        Some(entry) => {
            let path = reg.as_ref().and_then(|r| r.path_of(&entry));
            ui.horizontal(|ui| {
                match path {
                    Some(p) => {
                        ui.label(egui::RichText::new(format!("Linked: {p}")).weak());
                        if let Some(d) = &dir {
                            if ui.small_button("Open in editor").clicked() {
                                if let Err(e) = crate::files::open_file(&d.join(p)) {
                                    ui.colored_label(egui::Color32::RED, e);
                                }
                            }
                        }
                    }
                    None => {
                        ui.colored_label(
                            egui::Color32::RED,
                            format!("Entry {entry:?} is not in the registry — embedded copy runs."),
                        );
                    }
                }
                if ui.small_button("Embed").clicked() {
                    // drop the link (pruning the entry when unused); the
                    // file is left on disc untouched
                    match creabim_core::drop_link_tx(doc.model(), id) {
                        Ok(tx) => match doc.commit(tx) {
                            Ok(()) => {
                                s.link = None;
                                label = Some("Embed script");
                            }
                            Err(e) => {
                                ui.colored_label(egui::Color32::RED, e.to_string());
                            }
                        },
                        Err(e) => {
                            ui.colored_label(egui::Color32::RED, e);
                        }
                    }
                }
            });
            if path.is_some() {
                ui.label(
                    egui::RichText::new(
                        "The file is the source of truth; edits here are written to it.",
                    )
                    .weak(),
                );
            }
        }
        None => {
            let name = doc
                .model()
                .get(id)
                .map(|e| e.name.clone())
                .unwrap_or_default();
            ui.horizontal(|ui| {
                let b = ui.add_enabled(dir.is_some(), egui::Button::new("Link to scripts/…"));
                if b.clicked() {
                    if let Some(d) = &dir {
                        match creabim_script::link::link_from_embedded(d, &name, s) {
                            Ok((entry, rel)) => {
                                match creabim_core::set_link_tx(doc.model(), id, &entry, &rel) {
                                    Ok(tx) => match doc.commit(tx) {
                                        Ok(()) => {
                                            s.link = Some(entry);
                                            label = Some("Link script");
                                        }
                                        Err(e) => {
                                            ui.colored_label(egui::Color32::RED, e.to_string());
                                        }
                                    },
                                    Err(e) => {
                                        ui.colored_label(egui::Color32::RED, e);
                                    }
                                }
                            }
                            Err(e) => {
                                crate::actlog::line(format!("script {name}: {e}"));
                                ui.colored_label(egui::Color32::RED, e);
                            }
                        }
                    }
                }
                if dir.is_none() {
                    ui.label(egui::RichText::new("Save the project to link its scripts").weak());
                }
            });
        }
    }

    match s.declared() {
        Err(e) => {
            ui.colored_label(egui::Color32::RED, format!("Header: {e}"));
        }
        Ok(decls) => {
            for d in &decls {
                if input_row(ui, doc, s, d) {
                    label = Some("Edit script");
                }
            }
        }
    }

    ui.separator();
    ui.label("Source");
    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::widgets::TextBuffer, wrap_width: f32| {
        python_layout(ui, buf.as_str(), wrap_width)
    };
    let editor = egui::TextEdit::multiline(&mut s.source)
        .code_editor()
        .desired_rows(24)
        .layouter(&mut layouter);
    if ui.add(editor).changed() {
        label = Some("Edit script");
        // Write the panel's edit straight to the linked file (M-S6):
        // the disc copy must not go stale behind the model.
        if let Some(d) = &dir {
            let reg = creabim_core::registry_of(doc.model()).map(|(_, r)| r);
            if let Err(e) = creabim_script::link::write_through(s, reg.as_ref(), d) {
                ui.colored_label(egui::Color32::RED, e);
            }
        }
    }

    ui.horizontal(|ui| {
        match runner.status(id) {
            Status::Running { .. } => {
                if ui.button("Stop").clicked() {
                    runner.cancel(id);
                }
            }
            _ => {
                if ui.button("Run script").clicked() {
                    runner.force(id);
                }
            }
        }
        status_line(ui, runner, id);
    });

    if !runner.console(id).is_empty() {
        egui::CollapsingHeader::new("Console").show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(140.0)
                .show(ui, |ui| ui.selectable_label(false, runner.console(id)));
        });
    }
    if let Status::Failed { traceback } = runner.status(id) {
        egui::CollapsingHeader::new("Error")
            .default_open(true)
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(traceback).color(egui::Color32::RED))
                        .selectable(true),
                );
            });
    }
    label
}

/// One declared input's widget; writes back into `script.inputs`.
fn input_row(
    ui: &mut egui::Ui,
    doc: &creabim_core::Document,
    script: &mut ScriptObject,
    decl: &creabim_core::ParamDecl,
) -> bool {
    let stored = script.inputs.iter().find(|i| i.name == decl.name);
    match decl.kind {
        ParamKind::Float => {
            let mut v = match stored.map(|i| &i.value) {
                Some(ParamValue::Float(x)) => *x,
                _ => match decl.default {
                    Some(ParamValue::Float(x)) => x,
                    _ => 0.0,
                },
            };
            let changed = crate::props::row(ui, &decl.name, |ui| {
                ui.add(egui::DragValue::new(&mut v).speed(0.1)).changed()
            });
            if changed {
                set_input(script, &decl.name, ParamValue::Float(v));
            }
            changed
        }
        ParamKind::Int => {
            let mut v = match stored.map(|i| &i.value) {
                Some(ParamValue::Int(x)) => *x,
                _ => match decl.default {
                    Some(ParamValue::Int(x)) => x,
                    _ => 0,
                },
            };
            let changed = crate::props::row(ui, &decl.name, |ui| {
                ui.add(egui::DragValue::new(&mut v).speed(1.0)).changed()
            });
            if changed {
                set_input(script, &decl.name, ParamValue::Int(v));
            }
            changed
        }
        ParamKind::Bool => {
            let mut v = match stored.map(|i| &i.value) {
                Some(ParamValue::Bool(x)) => *x,
                _ => matches!(decl.default, Some(ParamValue::Bool(true))),
            };
            let changed = crate::props::row(ui, &decl.name, |ui| ui.checkbox(&mut v, "").changed());
            if changed {
                set_input(script, &decl.name, ParamValue::Bool(v));
            }
            changed
        }
        ParamKind::Text => {
            let mut v = match stored.map(|i| &i.value) {
                Some(ParamValue::Text(x)) => x.clone(),
                _ => match &decl.default {
                    Some(ParamValue::Text(x)) => x.clone(),
                    _ => String::new(),
                },
            };
            let changed = crate::props::row(ui, &decl.name, |ui| {
                ui.text_edit_singleline(&mut v).changed()
            });
            if changed {
                set_input(script, &decl.name, ParamValue::Text(v));
            }
            changed
        }
        // Element inputs name their target in the CLI for now; the
        // panel shows what they point at, read-only (M-S2).
        ParamKind::Element => {
            let what = match stored.map(|i| &i.value) {
                Some(ParamValue::Element(target)) => doc
                    .model()
                    .get(*target)
                    .map(|e| e.name.clone())
                    .unwrap_or_else(|| "(missing)".to_string()),
                _ => "(unset)".to_string(),
            };
            crate::props::row(ui, &decl.name, |ui| {
                ui.weak(format!("→ {what}"));
            });
            false
        }
    }
}

fn set_input(script: &mut ScriptObject, name: &str, value: ParamValue) {
    if let Some(found) = script.inputs.iter_mut().find(|i| i.name == name) {
        found.value = value;
    } else {
        script.inputs.push(creabim_core::ParamInput {
            name: name.to_string(),
            value,
        });
    }
}

fn status_line(ui: &mut egui::Ui, runner: &ScriptRunner, id: ElementId) {
    match runner.status(id) {
        Status::Idle => {
            ui.weak("idle");
        }
        Status::Running { since } => {
            ui.spinner();
            ui.weak(format!(
                "running… {}s (Stop to interrupt)",
                since.elapsed().as_secs()
            ));
        }
        Status::Ok {
            ms,
            added,
            replaced,
            removed,
        } => {
            ui.weak(format!("ok in {ms} ms (+{added} ~{replaced} -{removed})"));
        }
        Status::Failed { .. } => {
            ui.colored_label(egui::Color32::RED, "failed — see Error below");
        }
        Status::Cancelled => {
            ui.weak("cancelled");
        }
        Status::TimedOut => {
            ui.colored_label(egui::Color32::RED, "timed out");
        }
    }
}

/// A tiny Python highlighter for the editable source: comments,
/// keywords, strings and numbers. Hand-rolled on purpose — one less
/// dependency for a first editor (M-S4 gets a real one in the webview).
fn python_layout(ui: &egui::Ui, text: &str, wrap_width: f32) -> Arc<egui::Galley> {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let plain = TextFormat::simple(
        egui::TextStyle::Monospace.resolve(ui.style()),
        ui.visuals().text_color(),
    );
    let mut comment = plain.clone();
    comment.color = egui::Color32::GRAY;
    comment.italics = true;
    let mut keyword = plain.clone();
    keyword.color = egui::Color32::from_rgb(204, 120, 50);
    let mut string = plain.clone();
    string.color = egui::Color32::from_rgb(106, 135, 89);
    let mut number = plain.clone();
    number.color = egui::Color32::from_rgb(104, 151, 187);
    for line in text.split_inclusive('\n') {
        // A comment runs to end of line; strings are single-quote runs
        // without escapes (good enough for an M-S2 editor).
        let mut rest = line;
        if let Some(hash) = rest.find('#') {
            let (code, comment_text) = rest.split_at(hash);
            highlight_code(&mut job, code, &plain, &keyword, &string, &number);
            job.append(comment_text, 0.0, comment.clone());
            rest = "";
        }
        if !rest.is_empty() {
            highlight_code(&mut job, rest, &plain, &keyword, &string, &number);
        }
    }
    ui.ctx().fonts_mut(|f| f.layout_job(job))
}

fn highlight_code(
    job: &mut egui::text::LayoutJob,
    code: &str,
    plain: &egui::text::TextFormat,
    keyword: &egui::text::TextFormat,
    string: &egui::text::TextFormat,
    number: &egui::text::TextFormat,
) {
    const KEYWORDS: &[&str] = &[
        "import", "from", "as", "def", "return", "for", "in", "while", "if", "elif", "else",
        "None", "True", "False", "and", "or", "not", "with", "class", "pass", "raise", "try",
        "except", "lambda",
    ];
    let mut token = String::new();
    let mut chars = code.chars().peekable();
    let flush = |token: &mut String, job: &mut egui::text::LayoutJob| {
        if token.is_empty() {
            return;
        }
        let format = if KEYWORDS.contains(&token.as_str()) {
            keyword.clone()
        } else if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            number.clone()
        } else {
            plain.clone()
        };
        job.append(&std::mem::take(token), 0.0, format);
    };
    while let Some(c) = chars.next() {
        if c == '\'' || c == '"' {
            flush(&mut token, job);
            let quote = c;
            let mut lit = String::from(c);
            for c2 in chars.by_ref() {
                lit.push(c2);
                if c2 == quote {
                    break;
                }
            }
            job.append(&lit, 0.0, string.clone());
        } else if c.is_alphanumeric() || c == '_' || c == '.' {
            token.push(c);
        } else {
            flush(&mut token, job);
            job.append(&c.to_string(), 0.0, plain.clone());
        }
    }
    flush(&mut token, job);
}
