# Known gaps

This is the one place to look for what CreaBIM doesn't do yet, or does wrong. It gathers the "Known gaps" sections of every milestone README in [docs/evolution](evolution), the review of the starter house on 2026-10-28, and gaps found while working. Each milestone README keeps its own list as it was written, and points here.

Last checked against `main` on 2026-11-04, after the open JSON import (#44) and graphic styles and view ranges (#45).

**How to read it**

- **Size** is a rough guess. **S** is a day or less, a few lines up to a small function. **M** is a few days. **L** is a milestone of its own.
- **Found in** names the milestone folders in `docs/evolution`, without the `phase1-` prefix. *review* is the 2026-10-28 review of the starter house sheets. *sweep* is the gaps sweep of 2026-11-04.
- **Checked** ✓ means the gap was confirmed in the code, a test or the running app on 2026-11-04. A blank means it is carried over from its README and was not checked again.
- Within each theme, the gaps that hurt users most come first.
- When you close a gap, move it to [Recently fixed](#recently-fixed) with the branch that fixed it. When you find one, add it here and in your milestone README.

## Summary

| Theme | S | M | L | Open |
| --- | ---: | ---: | ---: | ---: |
| [Plans, sections and drawing quality](#plans-sections-and-drawing-quality) | 14 | 7 | 0 | 21 |
| [Rooms and ceilings](#rooms-and-ceilings) | 3 | 2 | 0 | 5 |
| [Walls, joins and details](#walls-joins-and-details) | 9 | 5 | 0 | 14 |
| [Openings](#openings-doors-windows-curtain-walls) | 3 | 2 | 0 | 5 |
| [Roofs, stairs, structure](#roofs-stairs-structure) | 7 | 3 | 2 | 12 |
| [Furniture, fixtures, parts and site](#furniture-fixtures-parts-and-site) | 11 | 1 | 1 | 13 |
| [Annotation and marks](#annotation-and-marks) | 15 | 4 | 0 | 19 |
| [Sheets and schedules](#sheets-and-schedules) | 8 | 3 | 0 | 11 |
| [Editing and manipulation](#editing-and-manipulation) | 22 | 7 | 0 | 29 |
| [3D view](#3d-view) | 10 | 4 | 0 | 14 |
| [IFC import and export](#ifc-import-and-export) | 6 | 6 | 2 | 14 |
| [JSON model format](#json-model-format) | 3 | 5 | 0 | 8 |
| [DXF, SVG and other exchange](#dxf-svg-and-other-exchange) | 3 | 2 | 1 | 6 |
| [Interface and help](#interface-and-help) | 13 | 4 | 1 | 18 |
| [Performance](#performance) | 0 | 2 | 0 | 2 |
| **Total** | **127** | **57** | **7** | **191** |
| [Brief 02](#brief-02-2026-11-04) (counted on its own; M–L as M, S–M as S; with addendum A3) | 7 | 9 | 4 | 20 |

The sweep of 2026-11-04 fixed 8 gaps and found about 25 others already fixed since they were reported; see [Recently fixed](#recently-fixed).

---

## Brief 02 (2026-11-04)

Every item of *CreaBIM brief 02: consolidated*, approved by the owner on 2026-11-04 with its mockup boards. **Item** is the brief's section; **Phase** is the brief's order of work (1 quick wins, 2 layout and editing, 3 structure, 4 heavy modelling, R release basics). **Source** is the brief's tag: *MU01-nn* and *MU02…MU11* are the owner's markups, *B01* the first brief, *RR* the release review, *tour* the tour films. Items move to [Recently fixed](#recently-fixed) as they land.

| Item | Gap | Phase | Size | Source |
| --- | --- | --- | --- | --- |
| 0 | The waiting branches `claude/demo-player`, `claude/starter-house-polish` and `claude/backlog` were never pushed. Their multi-select Properties edits and `creabim-app <file>` were rebuilt in Phase 1; category names still wrap mid-word in the visibility list. | 1 | S | B01 |
| 2.6 | View areas and workspaces are in. Still to do: saved workspaces live in the user's settings, not in the project; only one area can show the 3D view; a number control (such as *Cut height*) that doesn't fit an area header has no place in its ⋯ menu; a sheet has no *Open in area ▾* (double-click opens it in place); a corner join always closes the area dragged into. | 3 | S | MU02-02…05 |
| 2.10 | The tracker shows Length and Angle while a line is drawn in a plan, but Move, Rotate and Copy show no Δx / Δy in it, and slab and region edges don't use it. Temporary dimensions and the padlock work in plans; levels have none in sections yet, walls measure centreline to centreline only, a door measures to its wall's centreline end, only walls exactly on a grid line follow it, a door's gap can't be locked, and the padlock glyph is not from the icon set. | 2 (tracker), 3 (temporary dimensions) | M |
| 2.11 | Room tags: the four layouts are drawn, but there is no room tag designer, and tags don't yet keep level in rotated views. | 2 | S | MU09 |
| 2.12 | View markers: the four families and your own mixes are drawn. A marker's colour applies to fills and text, not lines. A single click on a bubble doesn't open its view (a double-click on the selected mark does). | 2 | S | MU09 |
| 2.13 | Choices with pictures and live previews are in *Tags and marks* and the grip choice only; pens, patterns, object styles and view templates are still plain lists. No room tag designer. | 3 | M | MU10 |
| 2.14 | Grips have their four states and a live pill (old → new ± change, A3-3), but no gap to the nearest neighbour while dragging, no "Widened D03 by 300 mm" wording in the status bar, and the grip accent colour isn't a personal setting (the grip style is). | 2 | M | MU11, MU12 |
| 3.3 | Views have no linked "Open in 3D" crop box (callouts, sections, elevations, plans). (A callout's box resizes and moves with grips.) | 2 | M | MU01-07 |
| 3.5 | Drawing on a face and mirror, offset, move, rotate, align and push/pull there are done (Phase 4 item 2). Still to do: room outline and detail region on a face; the plan's 2D tools on a face; a mirrored object or asymmetric column keeps its own shape (only position and facing mirror); proper icons for Mirror and Offset (owner's OK); stray grey slivers in 3D while the workplane is on a face. | 4 | S | MU01-12 |
| 3.6 | Edge palette done (offset edge, offset all, insert point, in 3D and plan). Still to do: icons for the palette (owner's OK); walls have no outline edges there. | 4 | S | B01-7 |
| 3.7 | Layer editing is in (Phase 4 item 1). Left over: a layer run on by hand into the wall it meets overlaps that wall's layers in plan (Check layer joins lists it); a grip drag's preview leaves out the joins around it; thin layers' grips crowd together in plan at small scales; corner presets don't cover tees or three-way joints; a slab's arc edge keeps its bulge ratio when its layer moves; adding or removing an outline corner shifts the layer edits on the edges after it; the open JSON format doesn't carry layer ends, layer edges or corner presets yet (the project file does); an IFC read back takes a wall's layers from its layer set, so its parts, and the edits they show, are not read; Check layer joins covers wall ends only, not slab and roof edges. | 4 | S | MU08-01 |
| 5.2 | Take-off leaves out stairs, railings, parts, masses, doors, windows and objects; a wall's area isn't trimmed at joins; one rate per material, no currency. | 4 | M | B01 |
| 5.3 | No image or PDF references with calibration. | 4 | L | B01 |
| 5.4 | No profile library for beams and columns as sweep presets. | 4 | L | B01 |
| 5.5 | No curved walls. | 4 | L | B01 |
| 5.6 | Stairs: no "Cut a well for this stair", headroom check, short-landing or re-flow warnings. Roofs: no overhang per edge, no offer to attach the walls under a new roof. No Check model panel (openings across joins, beams or footings in slabs, missing wells). | 4 | M | RR |
| A3-1 | A selected footing has no x-ray outline in 3D, and plans have no "Footings in this plan: Hidden / Dashed / Shown" choice. (Footings show while the tool is on, and a selected one always shows.) | 2 | S | MU13_01, MU13_05 |
| A3-2 | Railing on a stair, Tag and Attach top don't yet let their host win over snaps before the first click. (Footing, Door, Window and Opening do.) | 2 | S | MU13_02 |
| 6 | Release basics: Windows installer and portable zip, a file-format version in `.creabim`, README limits, issue templates, the Terrace House sample, CONTRIBUTING. | R | M | RR |

---

## Plans, sections and drawing quality

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| The plan and section panes on screen draw with CreaBIM's own pens at fixed pixel widths. The view's line weights and graphic styles reach sheets, the sheet view, PDF and SVG only. | M | drawing-quality, styles-ranges | |
| A plan on its own level's range doesn't show the roof of the level below. It shows only with a view range of *Levels in range* reaching down to it. | S | building-elements | |
| There is no coarse, medium or fine detail level. | M | styles-ranges | |
| Sun shadows are cast only on the ground, from from-above plans. They don't fall on lower roofs and aren't drawn in elevations or sections. | M | styles-ranges | |
| Detail items (filled regions, components) have no category, so they follow pack patterns by hatch name but take no object style or override. | S | styles-ranges | |
| The label placement pass runs on plans only, not on sections or elevations. | M | drawing-quality | |
| Dimension text is not moved off other lines; labels keep clear of it instead. | M | drawing-quality, annotation | |
| In a crowded room, a tag can move a fair way from its door or window (its leader still points back). | S | drawing-quality | |
| The below-cut tint of furniture is the convex outline of each part, so an L-shaped worktop made as one part is tinted as its hull. | S | drawing-quality | |
| Grid lines in plans and sections use the plain grid pen, not their group's colour, and have a bubble at one end only (`Grid` has no bubble settings). | S | grids-visibility | ✓ |
| A wall whose attached top drops below the cut along part of its length is still drawn cut along all of it. (A wall wholly under the cut is drawn in projection since the sweep.) | S | sweep | ✓ |
| Parts that lie wholly above the plan cut, such as a screen wall's coping, are not drawn in plan. There is no overhead line style for them. | S | parts-site | |
| Elevations draw each wall's own outline at corners. Joins clean up plans and sections only. | M | details-joins | |
| In Section A-A, a short outline of a partition beyond the cut shows above the ceiling line over the landing. | S | rooms-ceilings | |
| In hidden-line sections and 3D views, a hidden door or window panel still hides the lines behind it. | S | grids-visibility | |
| In sections, where a beam's cut is subtracted from a wall's cut, the beam is not drawn dashed. | S | ui-visibility | |
| In sections, the cavity (air gap) of a layered wall is left open at the wall's ends. | S | layered-assemblies | |
| In sections, a hidden element's coloured fill is the convex hull of its shape, so it is rough for L-shaped or hollow elements. | S | picking-xray | |
| A selected shaft is highlighted only on its base level's plan. | S | types-curtainwalls | |
| Fine patterns (block paving) fill solid when zoomed far out; their lines don't thin out with the scale. | S | parts-site | |
| Plants in section are drawn with the facets of their 3D form, not a soft outline. | M | parts-site | |

## Rooms and ceilings

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| The Slab tool's *Fill room* fills to wall centrelines rather than inner faces (the Room tool fills to inner faces); its preview shows which. | S | workflow | ✓ |
| Rooms in sections are coloured only when *Colour rooms* is on. There are no room tags in sections or elevations. | M | rooms-ceilings | |
| An `IfcSpace` from another program keeps the outline of its body. It doesn't follow the walls until it is placed again with the Room tool. | M | rooms-ceilings | |
| A ceiling is drawn only in ceiling plans, sections and 3D. Selecting one in a floor plan doesn't highlight it there. | S | rooms-ceilings | |
| The Room and Ceiling tools' `1`–`4` keys work only while the pointer is over the drawing, as with the Slab tool. | S | rooms-ceilings | |

## Walls, joins and details

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| Flipping a wall, changing its thickness about a line other than its centre, or pushing its side doesn't stretch the walls joined to it, so a corner can open or overlap until a neighbour is touched. | M | wall-behaviour, openings-frames | |
| Walls on different levels are not joined, and crossings are worked out per level. Moving a ground-floor wall leaves the first-floor wall above where it was. | M | layered-assemblies, wall-behaviour | |
| A wall attached to a roof that only partly covers it keeps its own height on the uncovered part in 3D; in IFC that part is extruded to the roof's highest point. | M | wall-behaviour | |
| A junction rule changes how overlapping pieces are cut, not how wall ends are trimmed, so at a wall-to-wall node *Switch join order* may make no visible difference. | M | details-joins | |
| A slab drawn past the wall's structure into its insulation leaves the part in the insulation as a separate island of concrete. | M | details-joins | |
| Typing new start or end coordinates in Properties moves only that wall. Joined walls follow drags and moves, not typed values. | S | layered-assemblies | |
| Where three walls meet and no two are in line (a Y), the third wall's trim can leave a small overlap. | S | details-joins | |
| Layer priorities can only be overridden in the assembly file. *Edit layers…* has no priority column. | S | details-joins | |
| A detail stores a copy of its parent's view settings when made. If the parent changes, the stored copy goes stale, and a detail whose parent is deleted draws from that old copy. | S | details-joins | |
| A callout box is always a rectangle aligned with the view. A detail of a detail has had little testing. | S | details-joins | |
| Core-line locations are not kept when the layers inside an assembly are edited. | S | wall-behaviour | |
| Editing the thickness in a tool's option strip clears that tool's type. | S | types-curtainwalls | |
| A copied wall (*Copy to levels*, *Duplicate level*) is made *Unconnected* and keeps its fixed height, so it may not meet the slab above where storey heights differ. | S | levels-footings | ✓ |
| Window reveals on shaded 3D views placed on sheets show no linings, because drawings leave them out. The 3D viewport shows them. | S | drawing-quality | |

## Openings (doors, windows, curtain walls)

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| A door or window has one material, so frame and glass can't differ (`OpeningFrame` has no material). In IFC its parts have no materials of their own. | M | element-materials, openings-frames | ✓ |
| Curtain panels can be swapped in the data, but the only UI is resetting them. | M | types-curtainwalls | |
| Nothing stops a curtain wall's opening from overlapping a door or window in the same wall. | S | edit-mode | |
| There is no exterior sill nosing. | S | openings-frames | |
| A type's *Overrides* are listed in one summary line, with no marker on each field. | S | types-curtainwalls | |

## Roofs, stairs, structure

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| Gable and hip roofs need a convex outline. On an L shape they fall back to flat, and valleys aren't computed. | L | building-elements | ✓ |
| Stairs are straight or L-shaped only: no U-shaped stairs, winders or spirals. | L | building-elements | |
| A stair doesn't cut the slab above by itself. A Shaft does, as in the starter house, but it isn't tied to the stair. | M | building-elements | ✓ |
| Railings are rails and posts only: no balusters, glass panels or handrail profiles. | M | building-elements | |
| Beams don't join or cut back at columns or at each other. They overlap. | M | building-elements | |
| A strip footing is always centred under its wall: no eccentric offset. | S | levels-footings | |
| Curtain walls get no strip footing. | S | levels-footings | |
| Arcs in a roof outline are flattened to straight segments. | S | building-elements | |
| Pushing a sloped roof face changes its thickness rather than moving the eave along the slope. | S | manipulation | |
| Pushing a stair's side widens the first flight only, not the landings. | S | manipulation | |
| Railings that follow a stair only change height with push and pull. | S | manipulation | |
| Grips not done: flipping a stair and its landing grips; a wall's base offset and a beam's height in section. | S | edit-safety | |

## Furniture, fixtures, parts and site

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| There are no pipes, ducts, cable runs or circuits, and no `IfcDistributionSystem` or ports. | L | mep-fixtures | |
| Sloped terrain doesn't leave holes for areas on its surface. Areas cut the ground only when the terrain is flat. | M | parts-site | |
| Changing a type between *On the floor / On a wall* and *From the ceiling* keeps the stored offset, so existing instances jump. | S | mep-fixtures | |
| Pasting objects into a project that lacks their type drops them. | S | object-catalogue | |
| Mounting heights in the schedule are to the underside of a wall fixture, not its centre line. | S | mep-fixtures | |
| Catalogue pictures show ceiling fixtures from above, as if they stood on the floor. | S | mep-fixtures | |
| The user catalogue folder is read once per session, and the name guess knows only English words. | S | object-catalogue | |
| *Make a type of this* on a screen wall loses the screen pattern: the new type is a solid wall type. | S | parts-site | |
| New hard landscape areas start at top 0, so on ground at -150 their top must be set by hand. | S | parts-site | |
| Entourage (cars, people) is exported as `IfcFurniture` and has no visibility category of its own. | S | parts-site | |
| There is no preview of a sweep's path while drawing it. | S | parts-site | |
| Wall-hosted sweeps have no corner grips; they follow their wall. | S | parts-site | |
| Objects have no host: they can't snap to a wall face and slide along it with a grip. | S | edit-safety | |

## Annotation and marks

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| Annotation tools don't work in the live 3D viewport. | M | annotation, annotations | |
| Sketch constraints are set in Properties only, and there are no tangent, angle or fixed-point constraints. | M | editing | |
| The duplicates list in the Mark numbering dialog is the only place that lists duplicate marks. There is no model-check window. | M | marks-annotation | |
| Dimension lines break only at tags and notes, not at room names or other text. The declutter pass may still move a tag off a dimension line on its own. | M | marks-annotation | |
| At 1:100, tags take their size from the annotation style, so type marks are large next to small symbols such as sockets. | S | mep-fixtures | |
| Roof tags point to the centre of the roof's bounding box, which for pitched roofs is inside the attic. | S | ui-visibility | |
| On screen, Serif is drawn with the sans font and bold is faked. PDF and SVG are correct. | S | annotation | |
| Leaders of notes placed on sheets end at a free point and are not attached to an element. | S | annotation | |
| In sections, dimensions snap only to levels and cut corners, not to edges seen beyond the cut. | S | annotations | |
| A dimension on a slab doesn't follow edits to the slab's first two corners. One on a mass doesn't follow vertex edits. | S | workflow | |
| Radius, diameter and arc length can only pick drawn arcs. Other curves need the three-point pick. | S | annotation | |
| Tags and dimensions can be box-selected and deleted, but not moved with G. Leader and witness grips on tags, notes and dimensions are not done. | S | editing, edit-safety | |
| Colours are chosen with the plain egui picker, not a named palette. | S | annotation | |
| IFC5 (`.ifcx`) export doesn't carry marks. IFC4 does. | S | marks-annotation | |
| Object categories (furniture, fixtures) have no prefixes in the mark rules. A mark typed on an object or its type does show. | S | marks-annotation | |
| Spot levels work in plans, sections and elevations, but not in axonometric views. | S | marks-annotation | |
| The Spot level tool takes its pattern from the last spot level made; its option strip has no pattern field. | S | marks-annotation | |
| Level heads spread apart only up and down, and only in sections and elevations. Dragging the end of a level's label is not done. | S | marks-annotation, edit-safety | |
| Level lines can't be shortened by dragging their ends. | S | levels-units | |

## Sheets and schedules

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| Schedule columns are fixed per category. They can't be added, removed or reordered (`ViewKind::Schedule` stores only the category and `by_type`). | M | annotations | ✓ |
| A viewport can't override its view's visibility, so you need a duplicate view. | M | grids-visibility | |
| Shaded images on sheets are pixels without anti-aliasing, so they look stepped when zoomed in the PDF. | M | annotations | |
| Schedules can't be sorted or filtered by clicking a header. | S | ui-pass2 | ✓ |
| Viewport and view crops are rectangles only. Crop grips on sheet viewports are not done. | S | annotations, edit-safety | ✓ |
| The underlay is not printed on sheets, and it shows the other level at this plan's cut height. | S | levels-units | |
| Views on a sheet have no alignment guides and can be dragged on top of each other. | S | sheets | |
| Title blocks can't be imported from PDF or raster files (DXF and SVG only). In the template editor, lines can't be dragged. | S | drafting-tools | |
| In the schedule, the material and thickness cells of layered walls stay editable, and the values snap back. | S | layered-assemblies | |
| Distance fade doesn't lighten shaded raster images (3D views placed on sheets). | S | styles-ranges | |
| Through several scale changes, a viewport's centre can drift by a millimetre or so: its middle is taken from the drawing's extent, which includes symbols and text drawn at a fixed paper size. | S | ui-workflow | |

## Editing and manipulation

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| With many elements selected, Properties edits only the active one. | M | editing | |
| Walls, doors, windows and masses always draw on the level in 3D, not on a custom workplane. | M | workflow | |
| A sketch is a single profile: no holes, and a circle is a sketch of its own. | M | editing | |
| Modifiers are for masses only, mirror only uses the mass's own axes, and values are edited in Properties, not with handles. | M | modeling | |
| Scale is uniform only and uses pointer distance, without base points. | M | modeling, move-layout | |
| Rotate turns about the vertical in plans and 3D and tilts only in sections. Masses can't be tilted freely in 3D. | M | modeling, move-layout | |
| Footings have no polygon outline, and there is no free 2D region element, so neither is in the shared Edit mode grammar. | M | edit-safety | |
| Paste goes only to the same place or onto another level. There is no paste at the cursor. | S | editing | |
| Drag-move moves the element under the pointer, not the whole selection (Move with M moves all of it). | S | move-layout | |
| Box selection while in a tool exists only for footings. Other tools still treat a drag as drawing. | S | ui-workflow | |
| In Edit mode, dragging the body of the element draws a selection box rather than moving it. Finish editing to move it. | S | edit-safety | |
| A shaft dragged across a slab's edge is reported, not clipped to the slab. | S | edit-safety | |
| Holes are drawn as polygons only, with no rectangle mode. A shaft's outline can't be redrawn. | S | types-curtainwalls | |
| In Edit mode, sketches with arcs or constraints only let you move corners, not add or remove them. | S | edit-mode | |
| A slab drawn in a section is a two-corner rectangle with its top on the nearest level; free heights are set afterwards. | S | annotations, workflow | |
| Align moves what is picked as a whole, not a single slab edge or sketch vertex. In sections it only moves. | S | drafting-tools | |
| Stretch needs the edge to be nearly parallel to the reference (within 5°) and doesn't rotate a face to meet a slanted line. | S | manipulation, stretch-3d | |
| Temporary guides don't work in 3D, on sheets or while dragging grips, and circle guides can't be kept with K. | S | drafting-tools | |
| In 3D, alignment tracking ignores guides on a workplane and uses model X and Y. Rest-on point acquisition works only in 2D panes. | S | levels-window | |
| The workplane is not saved with the project. It is set only by clicking a face, with no three-point or offset option. | S | workflow, modeling | |
| *Copy to levels* doesn't copy tags, grids or dimensions in the selection. | S | levels-footings | |
| *Duplicate level* always puts the copy at the top. | S | levels-units | ✓ |
| The Grid tool doesn't work in sections. A grid dragged in 3D snaps only to points lined up with it on screen. Grids can be references in 3D but not targets. | S | grids-visibility, move-layout, stretch-3d | |
| The *Bounds centre* pivot uses each element's origin, not its full geometry. | S | editing | |
| After a section is flipped, its pane keeps the old camera until Home. | S | manipulation | |
| Arc edges of slabs can't be pushed. | S | openings-frames | |
| An arrayed mesh offers the centre lines of the whole group, not of each copy. There is no centre plane across a wall's or beam's length. | S | stretch-3d | |
| Thin faces, such as a wall's top seen from far away, are hard to pick. | S | stretch-3d | |
| Grips not done: typing an opening's distance to the wall end; move and rotate grips on free meshes. A mass doesn't keep its sketch after extrusion. | S | edit-safety, modeling | |

## 3D view

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| No textures. glTF base-colour textures and Blender image textures are ignored. | M | materials | |
| See-through faces (glass, X-ray) are not depth-sorted, so overlapping ones blend in the wrong order. | M | materials, ui-visibility | |
| Crop boxes stay square to the model axes and can't be turned. | M | crop-boxes | |
| Crop boxes affect 3D only. Plans, sections and elevations ignore them and don't draw them. | M | crop-boxes | |
| Crop caps are a flat shade, not hatched with the material's cut pattern, and open meshes get no cap. | S | crop-boxes | |
| The crop box's move handle slides only in plan. Vertical moves are typed. | S | crop-boxes | |
| 3D visibility (single hidden elements, quick switches) is per session and not saved in the project. | S | grids-visibility, ui-visibility, editing | |
| The face highlight is drawn over the image, so it shows through what is in front. The 2D view shown in 3D is drawn on top of the model, without depth testing. | S | openings-frames, sections | |
| "Buried" (for hidden elements in colour) is decided by bounding box. | S | picking-xray | |
| Grid lines in 3D are one pixel wide and get no blue hover wash, and *Frame all* doesn't include grids. | S | grids-visibility, picking-xray | |
| The orthographic camera has no clipping control. | S | ui-pass2 | |
| The 3D view's buttons hide when the view is narrower than 320 px. Above that they scroll sideways. | S | sweep | ✓ |
| The app frames the 3D view and the plan for the window size it starts at, so on a large screen the plan first looks small. | S | starter-house | |
| Voids are hidden in presentation 3D views, together with openings; turn on *Openings, shafts and void cutters* to see one. | S | parts-site | |

## IFC import and export

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| Roofs, stairs and railings from other tools stay typed meshes, and `IfcMember` stays a mesh. Only CreaBIM's own exports bring them back native. | L | ifc-import, building-elements | |
| Openings are not cut in walls imported as meshes, and their doors and windows don't become native openings. | L | exchange, review | |
| L-shaped or T-shaped wall B-reps stay meshes rather than being split into straight walls. Notched or stepped beams, and sloped columns and beams, stay meshes. | M | ifc-import | |
| IFC STEP import of parts and site elements brings back plain shapes, not parametric elements — except arc sweeps and single clean extrusions, which round-trip as `IfcRevolvedAreaSolid` / `IfcExtrudedAreaSolid`. IFCX round-trips them. | M | parts-site | |
| NURBS and curved advanced B-rep faces are not read. | M | exchange | |
| Each element is exported as one body. Layers are described only by the material layer set, not as one solid per layer. | M | layered-assemblies | |
| Cut elements export as meshes, not `IfcBooleanResult` (attached wall tops already use `IfcBooleanClippingResult`). | M | exchange, wall-behaviour | |
| IFC5 (`.ifcx`) export leaves out furniture, fixtures, objects and curtain-wall openings. | M | mep-fixtures, object-catalogue, edit-mode | |
| IFC4 export writes no grids, so the starter house round trip loses them. A shaft comes back as the holes it cuts. | S | grids-visibility, ifc-import | ✓ |
| Door hand and swing are exported but not read back on import. | S | wall-behaviour | ✓ |
| The import report shows the element list in a small font, has no filter, and clicking a line doesn't select the element. | S | ifc-import | |
| A typed door or window's per-instance frame override is lost on export. | S | types-curtainwalls | |
| Materials: layer, profile and constituent sets are not read as separate materials, and *Open* creates only the materials that elements use. (`IfcSpecularExponent` is ignored on purpose.) | S | materials | ✓ |
| Importing a material whose name already exists replaces it, with no "keep both". | S | materials | |

## JSON model format

From the open JSON import (#44). [`docs/json-format.md`](json-format.md) keeps the full list, and `export-json` reports what it leaves out.

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| Annotation is not in the format: dimensions, spot levels, text styles, a tag's room fields, and note leaders that point at an element. | M | open-import | |
| Per-view settings are not in the format: visibility, plan graphics and view range, underlays, level heads, crop boxes of 3D views, view templates and style packs. | M | open-import | |
| Curtain walls and their types, openings in curtain walls, footings, shafts, ceilings, room separators, sketches, guides, free meshes and booleans are not in the format. | M | open-import | |
| Title block templates (sheets get the built-in one), mark rules, join rules, disallowed wall joins and detail components are not in the format. | M | open-import | |
| Only the starter catalogue's objects can be named; a user catalogue folder can't be pointed to. | M | open-import | |
| `--check` reports shape problems first, and references and geometry only once the shape is right, so a file with both takes two runs. | S | open-import | |
| Walls are always given by their centreline. The location line only says which line stays put in the app. | S | open-import | |
| The app's help guide doesn't cover the format; it is documented in `docs/json-format.md`. | S | open-import | |

## DXF, SVG and other exchange

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| No DXF or DWG export, including dimensions and annotations. | L | annotation | |
| DXF import skips blocks (INSERT), hatches, dimensions and splines. | M | drafting-tools | |
| FreeCAD export gives plain solids with a BIM type, not native Arch walls, and curved faces are faceted. Only rendering colours are read from FreeCAD materials. | M | exchange, materials | |
| SVG import turns curves into chords and reads only plain text and `tspan`. | S | drafting-tools | |
| Blender: only the Principled BSDF's unlinked inputs are read. | S | materials | |
| A Blender object becomes one part in its first material. | S | object-catalogue | |

## Interface and help

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| Not yet run on Windows. The build compiles in CI but hasn't been started on a Windows machine, and WebView2 page-size handling for help pictures is untested. This blocks the first Windows release. | M | interface, help-guide | |
| The Properties panel and the in-view bars (slab modes, the move, rotate and scale panel, the sketch tools) are still egui, restyled. The quick-help popover in Properties is an egui popup. | L | interface, ui-workflow | ✓ |
| On Linux the panes need X11 (XWayland on Wayland), and the file picker needs xdg-desktop-portal. | M | interface, editing | |
| The classic panels are behind: menus are text only, the outliner lists views in one group, there are no *Hide in view* items, and the guide has no pictures or links. | M | ui-visibility, levels-window, grids-visibility, help-guide | |
| The shortcuts page has no table per tool. Tools have no single-key shortcuts; the keys used while drawing are under *While drawing*. | S | help-guide, ui-workflow | ✓ |
| The project browser can't rename or hide elements. | S | interface, ui-pass2 | ✓ |
| Guide pictures are whole-window screenshots, so they are small in a narrow panel. | S | help-guide | |
| A plan opened in its own window keeps the old right-click menu. | S | interface | |
| Keyboard focus left in a web panel after clicking it is not taken back by Tab. | S | picking-xray | |
| The type preview's plan shows the sample section's marks, labelled *Preview*. | S | ui-workflow | |
| The 3D type preview is a shaded axonometric drawing that turns as you drag, not the live GPU view. | S | ui-workflow | |
| In a type's Properties, the class line repeats inside *Settings*. | S | ui-workflow | |
| *Assign as…* and *Booleans* are not foldable sections in Properties. | S | ui-workflow | |
| The Graphic styles editor doesn't cover object styles for one material, surface patterns, draw order or transparency, and the Materials window has no pattern picker. These are set in the pack's JSON. | M | styles-ranges | |
| A view template's overrides and depth cues are edited in the pack file, not in the app. | S | styles-ranges | |
| Packs carry mark rules, title blocks and text styles by name, but the Graphic styles editor doesn't show them. | S | styles-ranges | |
| In the narrowest windows (under about 1000 px) the menu row scrolls sideways too, so its right end (the view switcher) is reached by scrolling. | S | sweep | ✓ |
| The command palette finds actions and elements, but not properties. | S | ui-pass2 | |

## Performance

| Gap | Size | Found in | Checked |
| --- | --- | --- | --- |
| The scene is cropped again on every change, including every frame of a handle drag, which may lag on large models. | M | crop-boxes | |
| Orbiting a 3D view on a sheet redraws the whole view, which is slow on bigger models. | M | sheets | |

---

## Pending pull requests

Gaps reported by work that is still a pull request go here until it merges, then into the themes above.

- **Brief 02, Phase 2, first-hour fixes:** an export holds the window until it is written. In a debug build the starter house's 13 sheets take about a minute as PDF. A partition is judged by its thickness alone (under 150 mm), since walls have no load-bearing flag yet.

---

## Recently fixed

**In Brief 02, Phase 4, item 4, take-off (`claude/project-thread-2a2eta`, 2026-11-05;** screenshots in [evolution/brief02-phase4](evolution/brief02-phase4)**):**

- Schedules export to Excel (5.1): *File › Export › Schedules to Excel*, *Export Excel* on a schedule, or `creabim xlsx`. The workbook has a summary, a sheet for each schedule with numbers as numbers, and Take-off sheets.
- Quantities per assembly layer, a rate per material (per m², m³, m or piece) and a live Take-off schedule with totals by material and by level (5.2).

**Help pictures (`claude/project-thread-2a2eta`, 2026-11-05):**

- All 27 pictures in the Help guide are retaken in the new light interface. The door and wall pages no longer mention the orange flip arrows, which are gone: flips are `Space`, `Shift+Space` and the Flip buttons in Properties.

**In Brief 02, Phase 4, item 2, drawing on any face (`claude/project-thread-2a2eta`, 2026-11-05;** screenshots in [evolution/brief02-phase4](evolution/brief02-phase4)**):**

- In the 3D view, Extrusion, Void, Sweep, Slab and Wall opening draw on the face under the pointer (or the workplane). An extrusion or void keeps its plane and runs square to it: a void on a sloped roof cuts square into the slope. A sweep's path runs on the face with its profile standing out of it. A slab goes on a level face. A wall opening is the rectangle around what is drawn on a wall face (3.5).
- Snaps while drawing on a face only count on that face. A corner hidden behind the face no longer pulls the point somewhere else (3.5).
- Edge palette (3.6): select an element with an outline, rest the pointer on an edge in 3D or a plan, and choose Offset edge, Offset all or Insert point. Each follows the pointer with snaps, or goes a typed distance. In Mirror and Offset, Ctrl only switches the copy now; Shift snaps (15° steps, 50 mm).
- New Mirror and Offset commands (Edit menu, right-click menu, modify bar), in plan and on a face workplane, copying by default. Move slides along a face workplane, Rotate turns about its normal, Align turns a part on the face to a reference edge, and push/pull works on an extrusion drawn on a face (3.5).

**In Brief 02, Phase 4, item 1, layer editing (`claude/project-thread-2a2eta`, 2026-11-05;** screenshots in [evolution/brief02-phase4](evolution/brief02-phase4)**):**

- In Edit mode, `Tab` on a layered wall shows Layers: a grip at each end of each layer, in plans, sections, callouts and 3D. Drag one, or type how far, to run the layer on past its join or hold it back; the moved end's grip is filled, and right-click resets it or every layer. Each edit is kept as an offset from the join, through moves, stretches and a change of type (layers the new type lacks drop theirs, and the status bar says so) (3.7, in part).
- Layered slabs and roofs have the same Layers: a grip on each edge of each layer, to push that layer out past the edge or pull it in (a slate past the fascia), kept as a distance square to the edge, with reset (3.7, in part).
- Corners where two walls meet have join presets, in the right-click menu (*Corner join*) and *Properties › Junctions*: Auto, Cores lap and finishes stop, This wall runs through, The other wall runs through, Mitre every layer, Not joined (3.7, in part).
- A wall can be drawn by any line between its layers: *Draw by its exterior face* or *interior face* on a layer grip's right-click menu in Layers, or the boundaries listed under *Properties › Location line*. The wall does not move; the file keeps it as `"location": "layer-boundary"` with `"boundary"` (3.7, in part).
- *Create › Check layer joins* (and `creabim check`) lists and selects the walls with a layer held back from a join, leaving a gap, or run on into the wall it meets, overlapping it; ends that meet nothing are left alone (3.7).
- IFC export writes each layer of a wall, slab or roof whose layers were moved by hand, or whose cores lap at a corner, as an `IfcBuildingElementPart` with its layer's material, aggregated by the element, which keeps its own body and layer set. Checked with `tools/validate_ifc.py` (IfcOpenShell); reading such a file back doesn't add the parts as extra elements (3.7).
- The typed length in a grip drag showed a box where its caret should be. It shows `|`.

**In Brief 02, Phase 3, the view cube (`claude/project-thread-2a2eta`, 2026-11-05;** screenshots in [evolution/brief02-phase3](evolution/brief02-phase3)**):**

- A view cube in the 3D view's corner: 26 hot zones (faces, edges, corners) that turn the view smoothly to look from there, a compass ring under it, two quarter-turn arrows and a home button that frames everything. Its faces are named from north, so *Front* is the south side (4.2).

**In Brief 02, Phase 3, choices with pictures and your own styles (`claude/project-thread-2a2eta`, 2026-11-05;** screenshots in [evolution/brief02-phase3](evolution/brief02-phase3)**):**

- *Tags and marks* shows every choice as a row with a picture; resting on one shows it on the drawings until the pointer leaves, and a click keeps it (2.13).
- Room tags have all four layouts: Stacked, Number badge, Boxed and Inline (2.11).
- View markers have all four families: Classic, Minimal, Bold and Plain (2.12).
- A marker designer mixes head, fill, text, pointer, line, callout edge and colour, with a preview, and saves the mix under *Your styles* in the style pack, so *Save pack as…* and *Load pack…* carry it (2.13).
- Settings keeps your appearance apart from the project's, and the grip style (Quiet, Edges are the handles, One grip shape) is a personal choice with a preview (2.13, 2.14 in part).

**In Brief 02, Phase 3, temporary dimensions (`claude/project-thread-2a2eta`, 2026-11-05;** screenshots in [evolution/brief02-phase3](evolution/brief02-phase3)**):**

- Selecting a wall, a grid line, a door or window, or a column in a plan shows blue dimensions to its nearest neighbours (2.10).
- Clicking a value and typing a new one moves the element to make it true; a grid line takes the walls and columns on it, a wall its openings and joined walls (2.10).
- The padlock keeps a value as a dimension that holds the spacing: moving either end moves the other, in the same undo step (2.10).

**In Brief 02, Phase 3, view areas and workspaces (`claude/project-thread-2a2eta`, 2026-11-05;** screenshots in [evolution/brief02-phase3](evolution/brief02-phase3)**):**

- The middle of the window is split into view areas. Each has its own header: what it shows (3D, a view, or a sheet) and that view's controls. The 3D view's floating toolbar and the view controls in the options row are gone (2.6).
- A narrow area moves the controls that don't fit into its ⋯ menu, with Split, Close and *Show this area alone* (2.6).
- Four workspaces, Model, Drawing, Document and Sheets, are in the top-right menu. Ctrl+PgUp and Ctrl+PgDn step through them, and *Save current layout…* keeps your own (2.6).
- Ctrl+Space shows the area under the pointer alone, and again brings the others back; `/` isolates the selection or shows everything again (2.6).
- Dragging an area's corner grip into itself splits it, and into the area beside it joins the two, with a preview while dragging. Dividers resize the areas, and a double-click evens them out (2.6).
- The hint for where the pointer is now sits in the status bar after the message, instead of over the plan (2.6).

**In Brief 02, Phase 2, first-hour fixes (`claude/project-thread-2a2eta`, 2026-11-05;** screenshots in [evolution/brief02-phase2](evolution/brief02-phase2)**):**

- A new strip footing starts under the floor slab on its level instead of at the wall base, so it no longer overlaps a ground slab (1.4).
- *Under selected* leaves out partitions thinner than 150 mm and says how many; picking only partitions still puts footings under them (1.4).
- In a plan, what is in the cut wins a click over what is drawn dashed below it: a click in a door's opening takes the door, not the footing under the wall (1.4).
- A new beam's top starts under the slab above instead of inside it (1.5).
- Over an underlay, snaps take the traced walls' faces and their centrelines; the snap mark says *Underlay face* or *Underlay centreline* (1.6).
- Widening a door or window in Properties keeps it centred, or keeps the jamb nearest a wall end within 500 mm; the status bar warns when it then runs into a wall join (1.7).
- Home frames only the pane under the pointer, and the 3D toolbar's *Frame all* only the 3D view (1.8).
- Exports ask where to save, starting next to the project, and the status bar then offers *Open the folder* (1.8).
- A box selection that takes several kinds says what it took and offers *Keep only…* in the status bar (1.8).
- Guide lines from resting on a point or edge go after six seconds unused, instead of staying until Esc (1.8).
- Ctrl+A, H and the other letter shortcuts stay in a text field; the Catalogue window opens over the other pane, clear of where things are placed; the Materials window closes after *Assign* (1.8).

**In Brief 02, Phase 2, tool tabs and the action bar (`claude/project-thread-2a2eta`, 2026-11-05;** screenshots in [evolution/brief02-phase2](evolution/brief02-phase2)**):**

- The tools row has Model, Annotate, View and Manage tabs, with Select and Align on every tab. Each tab holds its tools and the commands that go with them. A tool picked from a menu or a key brings up its own tab (2.1).
- The tools row shows icons only, with names in tooltips. The tool in use shows its name in the options row. Nothing wraps, and nothing scrolls sideways at 1366 px (2.1). Moving the view controls out of the options row stays with 2.6.
- An element action bar sits under the name in Properties. The element's own actions come first, tinted, then Duplicate, Select similar, Hide, Isolate and Frame, then More and a red Delete. A mixed selection shows only the actions all of it shares, and a narrow panel moves actions into More. The Edit and Delete text buttons are gone (2.7).

**In Brief 02, addendum A3 (2026-11-05;** screenshots in [evolution/brief02-phase2](evolution/brief02-phase2)**):**

- Every toolbar and action icon is copied verbatim from the approved set (`ui/approved/icons_final.json` and `icons_actions.json`): Section is the scissors with the dashed line, Grid the straight four-line grid, Shaft has no level lines, Roof is the approved tipi, and Door, Paving, Level, Spot level and Footing are as drawn. A test compares `ui/icons.js` with the JSON, so they can't drift again (A3-6).
- Hovering a view mark, text, tag or dimension traces the mark itself, 1.5 times heavier, instead of boxing it; selected, it is traced in the accent with its grips, and a room tag loses its dashed box. A marker's text has the extent of its paper size, so it fits its bubble (A3-5).
- A grip's drag shows a pill by the grip with only the value changing ("Length 5000 → 5707 +707"), in that pane only, themed and legible in every theme; it fades about a second after release, the full summary goes to the status bar, and D hides or shows it (A3-3).
- No slide grip floats off a wall: the wall's body slides it, joined walls stretching; Shift frees it, Tab types the distance; a wall too thin on screen shows its thickness grip only near its face (A3-4).
- The tracker: while a line tool draws in a plan, Length and Angle can be typed (Tab between them, Enter places), and the step shows beside them (2.10).
- *Step* in the status bar and the Snap menu: Off, 10, 50, 100 mm or Custom (imperial: 1/8", 1/2", 1", 6"); free points and grip drags round to it, and Alt ignores it (2.10).
- While the Footing tool is on, plans show their level's footings dashed and tinted; a selected footing always shows; after placing, the status bar offers *Show in Foundation plan* or *Make a Foundation plan* (A3-1, most of it).
- The Footing, Door, Window and Opening tools let the wall or column under the pointer win over snaps and guide lines before the first click, with a preview of what a click makes; Tab goes through overlapping hosts; walls are hit across their whole thickness (A3-2, most of it).
- Detail components have grips: a square on each point, a width bar (batt, flashing, blocking, repeating detail), a unit bar on a repeating detail, corners and edges on a filled region, and Edit mode to add and remove points. They move, turn, mirror and copy with M, G, R, Shift+D and Ctrl+D, on their own view's paper (3.1, A3-7).
- A detail path finishes with Enter, a double-click or a right-click; Esc or another tool keeps a path with enough points; the hint counts the points (A3-7).
- A membrane draws 2 mm on paper off the line it follows, on the side it was clicked, with a dashed partner; the style pack can make it a double line or a zig-zag (A3-7).
- Plants have a canopy grip in plan and a height grip in 3D and sections (3.2).
- A callout's box has corner and edge grips on its parent view, and its detail view follows; a ringed callout stays round; G and M move the box, not its view's cut line (3.3, most of it).
- *Fill room* (Room, Ceiling, Slab) shows the space a click fills and its area under the pointer, and the gaps in red where walls don't close it (3.4).

**In Brief 02, addendum A2 (2026-11-05):** the logo, the weaver and its nest, is the brand mark in the top bar, the window and taskbar icon, the Windows .exe icon (not yet tried on a Windows build), the lockup in About, and the full mark in New project and the README. The `.creabim` file icon comes with the installer.

**In Brief 02, Phase 1 (`claude/project-thread-2a2eta`, 2026-11-04;** screenshots in [evolution/brief02-phase1](evolution/brief02-phase1)**):**

- A Properties change with several elements selected goes to every selected element of the same kind, in one undo step; each keeps its own place (brief 0).
- `creabim-app <file>` opens that file at launch (brief 0).
- Duplicate leaves its copy in place on Esc and says "Copy made: click a base point, then where it goes" (1.1).
- Drawing a callout keeps the parent view; a double-click on the selected mark opens its view (1.2).
- Tool help cards sit clear of the 3D view's toolbar and close on Esc or when the pointer leaves (1.3).
- An application menu (About, version and build, updates, Settings Ctrl+,, shortcuts Ctrl+/, Exit) and the file name at the right of the top bar (2.1, menu part).
- Properties rows put the label first and the control in one column; choices of four or more are dropdowns; on/off settings are accent switches; section headers are full-width rows with a turning chevron (2.2, 2.3).
- The final icon set, a FreeCAD icon of its own and a units icon instead of the ruler (2.4).
- A Units switch in the status bar, saved with the project and undone in one step (2.5).
- One **New project…** window (Ctrl+N) with Empty and Starter house; the six samples are under Help (2.8).
- Quiet grips: squares for points, bars for edges and jambs, no circles on stalks or flip circles, "+" only on the hovered edge in Edit mode, flips on Space and Shift+Space (2.9).
- Grip states: under the pointer a grip grows, names itself and shows a two-way pointer; held it has a white core and a halo, the others fade and its old place stays as a tick; let go, its value shows for a moment (2.14, states).
- Stacked room tags and Classic view markers by default, sized on paper, with the sheet reference or "–"; the first ones stay selectable under Graphic styles › Tags and marks (2.11, 2.12, in part).
- Top and Bottom look straight down and up with north up, and orbiting snaps to an axis view within 2° (4.1).

**In the gaps sweep (`claude/gaps-sweep`, 2026-11-04):**

- `Ctrl+S` pressed over a view after clicking a web pane read as `S` and started Scale. Keys typed while a page has the keyboard now keep their Ctrl, Shift and Alt, so `Ctrl+S` saves, and `Ctrl+Z`, `Shift+D` and `Alt+H` work the same way (rooms-ceilings).
- Under about 1100 px the menu and tool rows were cut off at the right. They now tighten, so the menu row fits down to 1000 px, and below that both rows scroll sideways with the wheel and fade at an end that has more (ui-workflow).
- The 3D window zoomed toward the middle of the view. It now zooms toward the pointer (stretch-3d).
- A wall attached to a roof or slab under the plan's cut was still drawn cut. It is now drawn in projection (wall-behaviour).
- On the floor a stair arrives at, its well kept a cross over the stair. The well keeps its outline without the cross (review, drawing-quality).
- *Attach top to roof/slab* was offered in the right-click menu with nothing overhead. It is greyed out then, as in Properties (wall-behaviour).
- The distance in the 3D Align and Stretch preview was orange text with no background. It sits on a dark pill (stretch-3d).
- The project browser's search didn't find elements by mark. It finds `D01` or `W05` as well as names (marks-annotation).

**Found fixed by earlier milestones since they were reported** (checked on 2026-11-04):

- **Graphic styles and view ranges (#45):** object styles give each category a projection pen, cut pen, colour, line pattern, cut fill and cut pattern, so there is line colour per category; plans have a view range (its level, *Levels in range* with top, bottom and view depth, or *From above*) with overhead elements dashed (styles-ranges).

- **Plans:** the stair is drawn through its well on the upper floor; cut fills of one material merge, with no squares at footing and wall junctions; line weights step down from cut to hatch; furniture and meshes below the cut are tinted; roof plans have slope arrows; wall openings above the cut are dashed; window reveals in 3D are lined with the wall's finishes, not insulation; labels are moved off lines and each other, with leaders (drawing-quality).
- **Rooms and ceilings:** there are Room and Ceiling elements, room schedules and tags, areas that follow the walls, and partitions that stop at ceilings; `IfcSpace` and `IfcCovering CEILING` import as rooms and ceilings; fixtures and the sofa no longer stand in doorways, `objects::against_wall` keeps clear of doors and windows, and `creabim check` reports objects in doorways (rooms-ceilings).
- **IFC import:** walls, slabs, columns and beams that are prismatic B-reps become native; a wall along a polyline axis becomes one wall per run; import warnings list each element with its class and GlobalId; edit meshes are welded in `edit_mesh_to_tri`; scaled and mirrored mapped items use their full transform (ifc-import).
- **Interface:** the command palette finds elements; links to websites in the guide open in the browser; the 3D buttons step aside for egui windows and scroll sideways when the view is narrow; the element count leaves out the units element; the browser has a button that folds every group (ui-workflow, ui-pass2).
- **Editing:** Edit mode is safe (Delete removes picked corners, never the element); grips slide doors and windows, turn objects, columns and stairs, set column and beam sizes and split walls; a slab's arcs can be traced in Edit mode (edit-safety).

For what was fixed before 2026-10-28, see the history of this file and the milestone READMEs.

## Not gaps

These were listed as gaps but are design choices, notes, or about other branches. They are kept here so they aren't added again:

- Deleting a host deletes its tags, dimensions and notes. A dimension on a hidden element is hidden with it.
- The last riser is the edge of the floor it climbs onto.
- Shift+D places a copy beside the original on the same wall. Flip mirrors free dimension points.
- A hosted footing ignores its own plan moves until it is let go. Footing tool fields are in metres.
- Sheet units stay in millimetres. Rehosting takes the new level's storey height. The Level tool in plans points to sections. The underlay level is a soft reference.
- The cut hatch is stored in `IfcMaterial.Description`. IFC5 is an alpha schema. Blender slabs snap to a level, and the Blender file's class wins on reload.
- The live 3D render mode is per user. Orbit is about the selection's box. Sheet viewports keep their own projection. A cropped 3D view stays centred on the model.
- 2D X-ray is per session and never on sheets. The plan pick ray only finds drawn elements.
- No new keyboard shortcuts were added in a milestone (marks-annotation): a note, not a gap.
- Screenshot and test-display notes (the levels-window rename shot, the import dialog missing from the materials shots), Ctrl+K typing through the app window, typing a path only in the classic panels, Esc in the help panel, the edge push grips' spacing, the style coverage note on level and grid marks, the manipulation coverage table, the Create menu "another branch" would fill, and the 3D colour encoding.
