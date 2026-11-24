<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="crates/creabim-app/assets/logo/png/creabim-lockup-white-640x200.png">
    <img src="crates/creabim-app/assets/logo/png/creabim-lockup-640x200.png" width="420" alt="CreaBIM: a weaver beside the woven nest it built">
  </picture>
</p>

<h1 align="center">One model. Every drawing.</h1>

<p align="center">
  A free, open-source BIM modeller for architects. Model your building in 3D, and its plans, sections, elevations, sheets, schedules, quantities and IFC follow on their own: always in step, on your own computer, with no subscription.
</p>

<p align="center">
  <a href="https://github.com/chopinregis/CreaBIM/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/chopinregis/CreaBIM?label=download&color=4F6B4C"></a>
  <img alt="Windows and Linux" src="https://img.shields.io/badge/Windows%20%C2%B7%20Linux-x64-3E5A3B">
  <img alt="GPL-3.0" src="https://img.shields.io/badge/licence-GPL--3.0-8FB08A">
  <img alt="IFC4 in and out" src="https://img.shields.io/badge/IFC4-in%20and%20out-6E8C68">
</p>

<p align="center">
  <a href="https://github.com/chopinregis/CreaBIM/releases/latest"><b>Download</b></a> ·
  <a href="#what-works-today">What it does</a> ·
  <a href="docs/KNOWN_GAPS.md">Where it stands</a> ·
  <a href="docs/ROADMAP.md">Roadmap</a> ·
  <a href="https://github.com/chopinregis/CreaBIM/discussions">Discussions</a>
</p>

![CreaBIM with the Maple Lane House from the garden, in 3D beside its ground-floor plan](docs/screenshot.png)

**CreaBIM is developed and designed by Regis Benoit Brice Nde Tene.** This repository is its official source. Forks and changes are welcome under the GPL.

**Status: 1.1. Ready to explore, not yet for production work.** The whole path works, from modelling to permit sheets and IFC. Some parts are young: IFC export of some elements, Blender exchange, DXF (lines only) and stairs. Everything that doesn't work yet is listed in [docs/KNOWN_GAPS.md](docs/KNOWN_GAPS.md).

**New in 1.1:** box, circle and lasso selection with a live count · a Blender-style Edit mode (points, edges and faces; G, R, S) · curved, circular and rectangular walls · twelve modifiers for masses · profiles with arcs for columns, beams and railings, and beams drawn on roof slopes · Masters: title blocks drawn in a view of their own, and pictures and logos on sheets · a Divide bar and a Split tool · eight themes and a selection colour of your own · a Log view and *Help › Report a problem*. The full list is in [docs/releases/1.1.0.md](docs/releases/1.1.0.md).

## Get it

| | |
|---|---|
| **Windows** | Download the `-setup.exe` from the [latest release](https://github.com/chopinregis/CreaBIM/releases/latest). It installs for you, with no administrator rights; uninstall from Settings › Apps. A portable `.zip` is there too. |
| **Ubuntu** | Download the `.deb` from the [latest release](https://github.com/chopinregis/CreaBIM/releases/latest) and install it with `sudo apt install ./CreaBIM-*-linux-x64.deb`. Launch CreaBIM from your applications menu. |
| **Fedora and RHEL 10** | On x86-64 systems with glibc 2.35 or later (Fedora 36+, RHEL 10 and compatible; not RHEL 9), GTK 3 and WebKitGTK 4.1, download the `.rpm` from the [latest release](https://github.com/chopinregis/CreaBIM/releases/latest) and install it with `sudo dnf install ./CreaBIM-*-linux-x64.rpm`. Launch CreaBIM from your applications menu. |
| **Other Linux distributions** | Download the `.tar.gz`, untar it and run `./creabim`. Needs WebKitGTK 4.1 and GTK 3. |
| **macOS** | Download and open the universal `.dmg` from the [latest release](https://github.com/chopinregis/CreaBIM/releases/latest), then drag CreaBIM to Applications. The app isn't code-signed or notarized yet, so the first time macOS refuses to open it: open **System Settings › Privacy & Security** and choose **Open Anyway**. |
| **From source** | Install Rust (stable), then `cargo run --release -p creabim-app`. |

Windows may warn that the publisher is unknown, because the app isn't code-signed yet: choose *More info*, then *Run anyway*. Each release lists SHA-256 checksums.

## Design in one tool, document in another: the seam

Most practices run two programs and a handover between them: a modeller for the design, and a drafting or BIM tool for the permit set. Every revision crosses that boundary by hand.

| The usual pipeline | CreaBIM |
|---|---|
| 1. Concept massing (design tool) | **One element store**: walls, slabs, openings, levels, grids, rooms |
| 2. Design development (design tool) | 3D view · live |
| 3. Redraw the plans (CAD) | Plans · 1:50 |
| 4. Rebuild in BIM (authoring tool) | Sections and elevations, cut from the model |
| 5. Reconcile the set, every revision | Sheets, schedules, take-off and IFC, all from the same model |

One edit lands in every view, in the same moment and the same undo step.

## At a glance

| **640** elements | **13** sheets | **3** IFC schemas | **1** model |
|:---:|:---:|:---:|:---:|
| in the starter house, footings to fixtures | drawn from one model, as vector PDF | IFC2X3, IFC4 and IFC4X3 read | behind every view, sheet, schedule and export |

<table>
<tr>
<td width="50%"><img src="docs/readme/plan-ground-floor.png" alt="Part of the Maple Lane House ground-floor plan, drawn by CreaBIM"></td>
<td width="50%">

### Drawn from the model, not traced over it

Sections and elevations are generated from the model. Cut walls, slabs and openings are drawn heavy; what lies beyond is projected with hidden lines removed.

- Hatches, dimension chains, room tags, door swings, window symbols
- Sheets with a title block and live views, exported as vector PDF
- **Exports:** SVG plans · PDF sheets · CSV and XLSX schedules and take-off · IFC4 · glTF for Blender · FreeCAD document

</td>
</tr>
<tr>
<td width="50%">

### Your own profiles

Draw a coping, a gutter or a steel section in its own view, with snaps, guides and typed lengths. Then sweep it, or use it for columns, beams and railings. Several shapes, voids you pick, and traces you draw over.

</td>
<td width="50%"><img src="docs/readme/profile-view.png" alt="The profile library with its twelve presets, and the I-section in the Profile view"></td>
</tr>
</table>

## Small, fast, and quiet

Written in Rust and drawn on your graphics card. Sections and 3D drawings are built on every core in the background, so dragging a wall never waits for the other views.

| **30 MB** | **0.2 s** | **0.3 %** | **16 cores** |
|:---:|:---:|:---:|:---:|
| the whole app, for Windows | from double-click to a window | CPU when you leave it alone | used to draw sections and 3D views |

<sub>Measured with the starter house on a 16-core Windows desktop. Your numbers will vary.</sub>

## The starter house

CreaBIM opens on the **Maple Lane House**, a contemporary two-storey family house with a kitchen wing, built to show what the program does and to be taken apart:

- three levels (foundation, ground and first floor) and a structural grid;
- layered walls in render and timber cladding, joined at every corner and tee, with typed doors and windows (some flipped), a curtain wall that cuts its host and a plain wall opening;
- floors with a riser hole and a stair shaft, an L-shaped stair with railings, a slate gable roof, a green flat roof and a zinc canopy;
- a steel column rising through both floors, terrace posts, steel beams, and concrete strip and pad footings;
- more than a hundred catalogue pieces of furniture and services fixtures;
- room names and areas, tags on every door and window, chains of dimensions;
- thirteen sheets with a filled-in title block: cover, ground, first floor, foundation and roof plans, two sections, four elevations, schedules and 3D views.

![The starter house's cover sheet: a live 3D view, the drawing list and a filled-in title block](docs/screenshot-sheet.png)

*File → New starter house* brings it back; *File → New simple sample house* opens the smaller house the tests use. The [Help](crates/creabim-app/help/starter-house.md) has a tour of it.

## What works today

- **One model, many views.** The 3D view and the plan read the same element store. An edit in either shows up in both at once.
- **Views are model elements:** plans, sections and elevations live in the model, so they save, undo and update with it. Pick one from the *View* list or the *Views* group in the model list.
- **2D view in three places:** docked beside 3D, in its own window (for a second monitor), or overlaid inside the 3D workspace. *View → Show the 2D view in 3D space* draws a plan at its cut height, or a section standing on its section line.
- **View areas and workspaces:** split the window into areas that each show 3D, a view or a sheet, with that view's controls in the area's header. Drag an area's corner grip to split or join areas. Four workspaces (Model, Drawing, Document, Sheets) follow the drawing you are in, and you can save your own. Ctrl+Space shows one area alone, Ctrl+PgUp/PgDn steps through workspaces, and `/` isolates the selection.
- **Sheets with live views:** place plans, sections, elevations, 3D views and pictures on A0 to A4 sheets with a title block. Title blocks are *masters*, drawn in a Masters view with lines, shapes, text, fields and your logo; change one and every sheet on it follows. Views on a sheet are generated from the current model, so they never go stale. Drag a view to move it; double-click it to work on the model right there, with the same tools as the view itself (orbit a 3D view by dragging). *File → Export sheets as PDF* writes every sheet as a vector PDF page.
- **Clean joins and materials:** wall corners are mitred and walls meeting another wall's side are trimmed to its face, in 3D, plans, sections and IFC. Walls cut through the slabs that sit in them. Each material has its own hatch pattern and colour, used for cut faces, shaded views and 3D, and exported to IFC.
- **Dimensions and tags:** draw dimensions that snap to wall corners, openings and levels. They stay attached, so moving a door updates its dimensions. Select a wall, grid line, door or column to see temporary dimensions to its neighbours: click one and type to move it there, or lock it with the padlock to hold the spacing. Tag doors and windows with their marks (D01, W01) and walls with their material and thickness, one at a time or all at once.
- **Schedules:** door, window, wall, slab, structure, furniture and fixture schedules are views too. Edit sizes and materials right in the table, place them on sheets and export them as CSV or to Excel.
- **Take-off:** quantities by material and assembly layer (area, volume, length, pieces), priced by a rate on each material, in a live Take-off table and in the Excel export.
- **Shaded and cropped views:** 3D views and elevations can be shaded in material colours, on screen, on sheets and in the PDF. Crop any view on a sheet by dragging its crop frame.
- **Sections and elevations generated from the model:** cut walls, slabs and openings are drawn heavy; what lies beyond is projected with hidden lines removed. Level datums are labelled. Section and elevation marks appear in plans. Draw new ones with the *Section* and *Elevation* tools, in plans or inside another section, and drag a selected section line by its ends or middle. Walls, doors, windows, slabs and masses can be drawn in sections and elevations too. Progress screenshots are in [docs/evolution](docs/evolution).
- **BIM elements:** levels, grids, walls (straight, arc, circle or a four-wall rectangle), curtain walls, slabs, roofs, stairs, railings, shafts, columns, beams and footings. Doors, windows and openings are hosted by walls and cut real openings. Walls, slabs and roofs can be layered assemblies, and most elements can share types. Columns and beams take any profile from the library; beams can be drawn on a roof slope or a wall, chained with mitred or butt corners.
- **Furniture and fixtures:** a catalogue of furniture and electrical, plumbing and HVAC fixtures, each with a 3D shape, a plan symbol and its IFC class, listed in their own schedules.
- **Freeform meshes:** add a mass, click a face in 3D and extrude (E), inset, taper, move or subdivide it (Catmull-Clark). In Edit mode (Tab), pick points, edges and faces as in Blender and move (G), turn (R) or scale (S) them. Give any mesh an IFC class such as `IfcRoof` and it exports as that.
- **Modifier stack:** give a mass array, radial and path array, mirror, bevel, shell, solidify, slice by levels, panelize, louvres, taper, twist and smooth modifiers. They change its shape without touching its mesh, so you can turn them off, reorder them or change their values at any time, and apply one to make it permanent. Faces stay clickable through the modifiers.
- **Sketches on workplanes:** the Sketch tool draws lines on any face in 3D, on a level in a plan, or on the section plane in a section, with lines, arcs (A), circles (C) and rectangles (R). Length, equal, parallel, perpendicular, horizontal, vertical, coincident and radius constraints drive its shape. A closed sketch extrudes (E) into a mass; an open one into a surface that Solidify thickens.
- **Move, rotate, scale, duplicate and stretch:** G, R and S (and Shift+D to copy) work Blender-style on the selection in 3D, plans and sections, with axis keys and typed values. R then X or Y tilts; S then an axis scales along it only. Pick the pivot (median, bounds, each element's own, the active element or a 3D cursor placed with Shift+right-click).
- **Selecting many:** Shift+click adds or removes, Ctrl+A selects everything, and a box, circle or lasso picks what lies inside (drag left to right) or what it touches (right to left), with a live count, on empty paper in plans and sections or after pressing B in 3D. The *Select* menu adds invert, select linked, select random, all of a type and select similar. Delete, move, copy and hide act on all of it.
- **Divide and Split:** cut a wall, beam or part into even parts, panels or courses from a bar over the views, or split anything along a line you click.
- **Copy, paste, hide:** Ctrl+C, Ctrl+X and Ctrl+V copy elements with their openings, also between two CreaBIM windows. H hides the selection, Shift+H shows only it, Alt+H shows everything again.
- **Open and save with the system file picker,** with a list of recent files. In plans, drag a selected element to move it, or its square grips to stretch walls, slabs and grids; joined walls follow.
- **Undo and redo** for every edit. Dragging a value in the properties panel undoes as one step.
- **Plan drawings generated from the model:** walls cut at a set height, door swings, window symbols, slabs and masses in projection. Export as SVG at 1:100.
- **Drag handles and snaps everywhere:** select a wall, slab, door or window and drag its handles in 3D or in plans to change length, height, thickness, width or sill. Snaps to ends, midpoints, intersections, edges, perpendiculars, guides and axes. Walls, slabs, doors, windows and masses can be drawn right in the 3D view.
- **Booleans, guides and redraw:** subtract (or unite, intersect) any solid from a wall, slab or mass; the cutter stays as a movable ghost. Construction guides through two points or parallel to an edge. Redraw a wall's path or a slab's outline.
- **Blender and FreeCAD, natively:** link a `.blend` or `.FCStd` file and its objects come in, reloading whenever the file is saved. Make any object a wall, slab, door or window with *Assign as*, or mark it in the file. Export glTF for Blender (with the [CreaBIM Link add-on](tools/blender-addon) for the way back) and FreeCAD documents.
- **IFC in and out:** open IFC2X3, IFC4 and IFC4X3 files (walls, slabs, doors and windows become native elements, everything else a mesh with its class) and IFC5 (`.ifcx`, alpha); export IFC4 and IFC5.
- **Project file:** `.creabim` is a SQLite file that keeps the undo history.
- **IFC4 export** with storeys, walls, slabs, openings, doors, windows and tessellated freeform elements. CI checks every export against the IFC schema and builds its geometry with [IfcOpenShell](https://ifcopenshell.org).
- **IFC import** of IFC2X3, IFC4 and IFC4X3 files: straight walls, slabs on a storey and the doors and windows in those walls become native, editable elements; everything else with a body (columns, beams, roofs, stairs, furniture, terrain…) comes in as a free mesh that keeps its IFC class. Units, placements, mapped types, materials and GlobalIds are kept.
- **IFC5 (experimental):** import and export of buildingSMART's alpha IFCX JSON format.

## Build and run

You need a recent stable Rust toolchain (1.95 or newer).

```sh
cargo run --release -p creabim-app   # the desktop app
cargo test --workspace               # the test suite
```

On Linux the app also needs a Vulkan or GL driver and `libxkbcommon-x11` at runtime. CI builds a Windows `.exe` on every pull request, downloadable from the run's artifacts.

### Command line

```sh
cargo run -p creabim-cli -- sample house.creabim.json            # write the starter house
cargo run -p creabim-cli -- sample-simple simple.creabim.json    # write the simple sample house
cargo run -p creabim-cli -- save-db house.creabim.json house.creabim  # convert to a SQLite project file
# every command that reads a model accepts a .creabim or a .creabim.json file
cargo run -p creabim-cli -- ifc house.creabim.json house.ifc     # export IFC4
cargo run -p creabim-cli -- import-ifc other.ifc other.creabim.json   # import IFC2X3/4/4X3
cargo run -p creabim-cli -- ifcx house.creabim.json house.ifcx   # export IFC5 (IFCX)
cargo run -p creabim-cli -- import-ifcx house.ifcx back.creabim.json
cargo run -p creabim-cli -- glb house.creabim house.glb          # glTF for Blender, with element ids
cargo run -p creabim-cli -- fcstd house.creabim house.FCStd      # FreeCAD document
cargo run -p creabim-cli -- link house.creabim garden.blend out.creabim  # bring in a .blend or .FCStd
cargo run -p creabim-cli -- plan house.creabim.json "Ground Floor" gf.svg
cargo run -p creabim-cli -- views house.creabim.json             # list plans, sections, elevations
cargo run -p creabim-cli -- view house.creabim.json "Section A-A" a.svg
cargo run -p creabim-cli -- sheets house.creabim.json            # list sheets
cargo run -p creabim-cli -- pdf house.creabim.json house.pdf      # every sheet, one PDF
cargo run -p creabim-cli -- schedule house.creabim.json windows w.csv  # doors, windows, walls or slabs
cargo run -p creabim-cli -- xlsx house.creabim.json house.xlsx  # every schedule and the take-off
python3 tools/validate_ifc.py house.ifc                          # needs: pip install ifcopenshell pytest
# a whole model from a JSON description any script can write (docs/json-format.md)
cargo run -p creabim-cli -- build examples/small-house.json small-house.creabim
cargo run -p creabim-cli -- build --check my-model.json          # report problems by JSON path
cargo run -p creabim-cli -- export-json house.creabim house.json  # and back to that format
```

## Using the app

| Action | How |
| --- | --- |
| Orbit, pan, zoom (3D) | Left drag, right drag, wheel |
| Pan, zoom (plan) | Right drag, wheel |
| Draw walls | Wall tool, click points; Esc or right-click ends; Shift keeps it orthogonal |
| Doors, windows | Door or Window tool, click on a wall |
| Slab | Slab tool, click two corners. In a section, click the two ends of its edge: the top snaps to the nearest level |
| Section, elevation | In a plan: Section or Elevation tool, click two points; it looks to the left of the direction you draw. In a section: click where it cuts, then the side it looks towards |
| Move a section line | Select it in a plan, drag the square end handles or the round middle handle |
| Draw in a section | Wall: click two points (length along the line, height as drawn). Door, Window: click on a wall. Mass: click to place |
| Sheets | Pick a sheet from the View list. *Place view…* adds a view, drag to move it, double-click to edit through it, Esc to finish |
| Orbit a 3D view | Drag in it, on its own or on a sheet |
| Dimension | Dimension tool, click two points (they snap to corners and levels), then a third for the offset. Drag a selected dimension to move its line |
| Tag | Tag tool, click a door, window or wall. *Tag doors and windows* tags every untagged one in the view. Drag a tag to move it |
| Schedule | *File → New schedule*, then edit cells in the table. *Export CSV* saves it, *Export Excel* saves every schedule in one workbook |
| Take-off | *File → New schedule → Take-off*; give materials a *Rate* in the Materials window or in the table |
| Crop a view on a sheet | Select the viewport, drag the dashed frame's edge handles, or set the crop in Properties |
| Materials | Select a material in the model list to change its hatch and colour; pick a wall's or slab's material in Properties |
| Freeform mass | Mass tool, click to place; click a face in 3D, then E to extrude |
| Modifiers | Select a mass, then *+ Add modifier* in its Modifiers tab. Each is a card: its icon turns it off, its `…` menu applies, duplicates or moves it, the cross removes it |
| Sketch | Sketch tool. In 3D, click a face to sketch on it (or the ground); in a plan or section, click on the drawing. Click corners; click the first corner to close it, Enter or right-click to leave it open. Shift keeps lines square. Then E extrudes it by the distance in Properties |
| Move, rotate, scale | Pointer over a view, selected element: G, R or S. Move the pointer or type a value; X, Y or Z keeps to an axis; Ctrl snaps; click or Enter finishes, Esc or right-click cancels. Shift+D copies first |
| Stretch, drag (plan) | Drag a selected element to move it, or its square grips: wall ends, slab corners and edge midpoints, grid ends |
| Undo, redo | Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y |
| Delete | Delete key (also removes doors, windows, tags and dimensions attached to it) |

Files: *File → Save* and *Open* use the system file picker and keep a list of recent files. *File → IFC4 (.ifc)*, *Sheets as PDF* and the other exports write next to the project file.

## How it is built

| Crate | Role |
| --- | --- |
| `creabim-core` | Element store, edit transactions, undo history, Blender-style edit mesh, modifier stack, workplanes and transforms. Every edit is a reversible op, which later also powers version history and live collaboration. |
| `creabim-geom` | Element geometry (walls with openings, slabs, meshes), plan drawing generation, picking. This is where OpenCascade plugs in for NURBS and B-rep work. |
| `creabim-ifc` | IFC4 STEP writer, STEP reader and IFC2X3/IFC4/IFC4X3 importer, IFCX (IFC5 alpha) reader and writer. Element ids are IFC GlobalIds, so round trips keep identity. |
| `creabim-store` | The `.creabim` project file: a SQLite database of the current elements plus the journal of every edit, so undo history survives closing the app. Records each edit incrementally (WAL) for cheap, crash-safe autosave. |
| `creabim-app` | Desktop app: egui UI, wgpu 3D viewport rendered offscreen so any number of live views can exist. |
| `creabim-cli` | Command-line export tools. |

What comes next is in [docs/ROADMAP.md](docs/ROADMAP.md); ideas are welcome in [Discussions](https://github.com/chopinregis/CreaBIM/discussions).

## Contributing and support

Contributions are welcome, in two kinds:
- **Any time:** making CreaBIM work on more systems, bugs, tests, file exchange, the guide and translations.
- **Proposal first:** new tools, workflows and interface changes. Show the idea working in your fork before the change comes in.

AI-assisted work is welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for how. Report a problem or share an idea through [Issues](https://github.com/chopinregis/CreaBIM/issues/new/choose). You can also [sponsor the work](https://github.com/sponsors/chopinregis); sponsoring never unlocks features.

## License

CreaBIM is free software under the GNU General Public License v3.0 or later. See [LICENSE](LICENSE).
