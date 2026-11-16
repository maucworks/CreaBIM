# Site: terrain, hard landscape and planting

The **Site** tools (*Create › Site*) model the ground around the building: the terrain, paved and gravelled areas, and trees and shrubs. Cars and people come from the catalogue.

## Terrain

1. Pick the **Terrain** tool.
2. Set its *Depth* in the tool options.
3. Draw the ground's outline like a slab: `1` Polygon, `2` Rectangle, `3` Fill room or `4` From sketch.

New terrain is flat at its level and covered in *Grass* from the material library. To shape it, add **spot heights** in *Properties*. Click *Add spot height*, then type its X and Y position and its height above the level. The surface runs through every spot height and slopes between them. The outline's corners stay at the terrain's *Top* unless you put a spot height on them, so one spot height in the middle makes a mound and spot heights on the corners tilt the whole site. Set *Contours every* to draw contour lines in plans, for example every 250 mm. Spot heights show in plans as small crosses.

## Hard landscape

**Hard landscape** areas are slab-like layers on the ground, each with its own material and surface pattern.

1. Pick the **Hard landscape** tool.
2. In the tool options, pick what to draw:
   - **Paving**: 600 × 400 concrete flags.
   - **Driveway**: block paving laid in stretcher bond.
   - **Gravel**: stippled.
   - **Decking**: boards.
   - **Asphalt**: no pattern.
3. Draw the area's outline like a slab.

The picks are only starting points. In *Properties*, change the area's *Top*, *Depth* and **Pattern**:

- **Units**: their length, width, staggered courses and angle.
- **Lines**: their spacing and angle.
- **Stipple**: its spacing.

Plans draw the pattern over the area, and 3D shows the area in its material. An area drawn inside flat terrain sits flush: the ground leaves room for it.

## Planting

1. Pick the **Planting** tool.
2. In the tool options, pick a **Tree**, **Conifer**, **Palm** or **Shrub**, and its *Canopy* (the spread of its crown) and *Height*.
3. Click to plant. Keep clicking to plant more.

In plan, each plant is a symbol as wide as its canopy: a scalloped crown with its trunk for a tree, a ringed star for a conifer, fronds for a palm, a smaller scalloped outline for a shrub. In 3D, sections and elevations it is a simple form: a trunk under a round crown, a cone, a tall trunk with a head of fronds, or a low mound. Give it a material to change the colour of its crown.

## Cars and people

*Create › Site › Cars and people…* opens the [Catalogue](furniture) at **Entourage**: a car, a person and a child, for scale. Place them like any other object.

## Reshaping

Terrain and hard landscape areas have the same grips as a slab: drag a corner or an edge, and open them in [Edit mode](edit-mode) (`Enter`) to add or remove corners or change an area's holes. Move plants with the [Move](move) tool.

## Linked geography (GeoPackage)

*File › Import and link…* reads a GeoPackage of map geography (for example BGT or BAG) as one underlay. One homogenous element per linked file holds every layer; each draws its outlines on every plan, in its category's hatch. GeoPackage files stay in their own CRS: the layers are projected into model metres on load.

- **Linking**: pick a `.gpkg` in *File › Import and link…*. The first link places the project in the file's CRS, with the model origin at the extent's middle. Files in another CRS are refused — reproject them first, never on load.
- **Editing**: select the linked file in the project browser — its *Properties* offer *Site layers…*, which opens *Settings › Site*, where the per-layer editor lives (the *CreaBIM* menu's *Site layers…* goes to the same place).
- **Location**: shows the project's CRS, its origin eastings/northings, and how far model-up is turned from grid north.
- **Set zero**: click a point of the geography on a plan; it becomes the model origin (0,0), and every layer re-projects onto it at once.
- **North line N/E/S/W**: pick a line of the geography; the project turns until it runs north, east, south or west, and every layer re-projects with it. So site and GIS stay synched around one reference point and one rotation.
- **Per layer**: the eye hides the whole file in the active view only; the *visible* box draws the layer project-wide. **Fill** colours under the category hatch, **Line** colours the outlines and polylines, **Line type** (solid, dashed, dotted, centre, dash-dot) breaks them up.
- **Reload** re-reads the file and syncs by source row: changed rows update, new rows insert, vanished rows go. **Unlink** removes the whole package.

BAG3D layers are 3D masses; only their 2D footprints draw for now — their 3D ingest is still to come, and reload says so rather than loading them silently.

## Exchange and visibility

Terrain exports to IFC as *IfcGeographicElement* (TERRAIN), plants as *IfcGeographicElement*, and hard landscape as *IfcCivilElement*. The IFCX format reads them back as the same editable elements. Views can hide them through the *Terrain and hard landscape* and *Planting* categories (see [Visibility](visibility)).
