# The CreaBIM model JSON format

A plain JSON file that describes a building model: levels, grids, walls
with their types and layers, doors and windows placed by wall and offset,
slabs with holes, roofs, columns, beams, stairs, railings, extrusions and
sweeps, site elements, furniture from the catalogue, rooms, views and
sheets with views placed on them. Any script, in any language, can write
one, and the `creabim` command-line tool turns it into a project you can
open in the app. Nothing has to be compiled against CreaBIM's code.

```sh
creabim build house.json house.creabim     # build a project
creabim build --check house.json           # only check the file
creabim export-json house.creabim out.json # describe a project in the format
creabim json-schema creabim-model.schema.json
```

The format has a JSON Schema, [`creabim-model.schema.json`](creabim-model.schema.json).
Point an editor at it with a `"$schema"` field, and it completes and checks
the field names as you type. A bigger example, a small house with a site,
furniture and three sheets, is [`examples/small-house.json`](../examples/small-house.json).

## A full small example

A one-room studio on a level, with a door, two windows, a floor, a gable
roof, a plan, a section, a 3D view, a door schedule and one sheet.

```json
{
  "$schema": "https://github.com/chopinregis/CreaBIM/blob/main/docs/creabim-model.schema.json",
  "version": 1,
  "units": "m",
  "materials": [
    { "id": "brick", "library": "Brick, common red" },
    { "id": "roof tiles", "hatch": "diagonal", "colour": "#7a3b2e" }
  ],
  "types": [
    { "id": "Brick wall", "category": "wall", "thickness": 0.3, "material": "brick" },
    { "id": "Window 1000", "category": "window", "width": 1.0, "height": 1.2, "sill": 0.9 }
  ],
  "levels": [
    { "id": "Ground", "elevation": 0, "height": 2.7 },
    { "id": "Eaves", "elevation": 2.7 }
  ],
  "walls": [
    { "id": "S", "level": "Ground", "start": [0, 0], "end": [6, 0], "type": "Brick wall", "top_level": "Eaves" },
    { "id": "E", "level": "Ground", "start": [6, 0], "end": [6, 4], "type": "Brick wall", "top_level": "Eaves" },
    { "id": "N", "level": "Ground", "start": [6, 4], "end": [0, 4], "type": "Brick wall", "top_level": "Eaves" },
    { "id": "W", "level": "Ground", "start": [0, 4], "end": [0, 0], "type": "Brick wall", "top_level": "Eaves" }
  ],
  "openings": [
    { "id": "Door", "kind": "door", "wall": "S", "centre": 1.5, "width": 0.9, "height": 2.1 },
    { "id": "South window", "kind": "window", "wall": "S", "centre": 4.2, "type": "Window 1000" },
    { "id": "North window", "kind": "window", "wall": "N", "centre": 3, "type": "Window 1000", "width": 1.8 }
  ],
  "slabs": [
    { "id": "Floor", "level": "Ground", "thickness": 0.2,
      "outline": [[-0.15, -0.15], [6.15, -0.15], [6.15, 4.15], [-0.15, 4.15]] }
  ],
  "roofs": [
    { "id": "Roof", "level": "Eaves", "shape": "gable", "pitch": 35, "overhang": 0.3,
      "thickness": 0.2, "material": "roof tiles", "edge": 0,
      "outline": [[-0.15, -0.15], [6.15, -0.15], [6.15, 4.15], [-0.15, 4.15]] }
  ],
  "rooms": [
    { "id": "Studio", "number": "01", "level": "Ground", "at": [3, 2] }
  ],
  "views": [
    { "id": "Plan", "kind": "plan", "level": "Ground", "scale": 50 },
    { "id": "Section", "kind": "section", "start": [-2, 2.5], "end": [8, 2.5], "depth": 5 },
    { "id": "3D", "kind": "3d", "azimuth": 30, "altitude": 25 },
    { "id": "Doors", "kind": "schedule", "category": "doors" }
  ],
  "sheets": [
    { "id": "A101", "name": "Studio", "size": "A3",
      "viewports": [
        { "view": "Plan", "at": [100, 190], "crop": { "min": [-1.5, -1.5], "max": [7.5, 5.5] } },
        { "view": "Section", "at": [300, 220] },
        { "view": "3D", "at": [300, 120] },
        { "view": "Doors", "at": [100, 60] }
      ] }
  ]
}
```

## How a file is put together

- **The top level** is an object with a `version` (required) and lists:
  `materials`, `assemblies`, `types`, `levels`, `grid_groups`, `grids`,
  `walls`, `openings`, `slabs`, `roofs`, `columns`, `beams`, `stairs`,
  `railings`, `parts`, `site`, `objects`, `rooms`, `views`, `sheets`,
  `tags` and `notes`.
  Every list may be left out, and the lists may come in any order.
- **Ids.** Every element has an `id` you choose: any non-empty string,
  unique across the whole file (a wall and a view can't share one). Elements
  refer to each other by these ids: a door names its wall, a plan names its
  level, a viewport names its view. The id also names the element in the app
  unless you give a `name`. A sheet's id is its number ("A101").
- **References are checked by kind.** A door's `wall` must be the id of a
  wall, not of a level. An element may refer to one listed after it.
- **Units.** Lengths are in metres. A top-level `"units"` of `"mm"`, `"cm"`,
  `"ft"` or `"in"` makes every length in the file that unit instead. Angles
  (pitch, rotation, direction, azimuth) are always degrees. Positions and
  sizes on sheets (`at`, a sheet's `width` and `height`) are always paper
  millimetres. `display_units` only sets how the app shows lengths.
- **Points** are arrays: `[x, y]` in plan (x east, y north), `[x, y, z]`
  in space, `z` above the element's level.
- **Heights hang off levels.** Every building element names its `level`;
  its heights (`base_offset`, a slab's `top_offset`, a railing's path `z`)
  are measured from that level's elevation, so moving a level in the app
  moves what is on it.
- **Types.** A wall, door, window, slab, roof, column or beam may name a
  `type`; it then takes the type's values, and any value it gives itself
  overrides the type's for that element only (as in the app).
- **Defaults.** A field with a default may be left out. The reference
  below gives each one.
- **Colours** are `"#rrggbb"`, or `[r, g, b]` from 0 to 1.
- **No nulls.** Leave a field out instead of writing `null`.

### Placing things

- A wall runs along its **centreline** from `start` to `end`. Its exterior
  face is on its right, looking from start to end, unless `flip` is set.
  Give `top_level` to tie its top to a level (it then follows that level),
  or a `height`; left out, it is as tall as its level's `height`.
- An opening sits in its `wall`, `offset` along it from the wall's start to
  the opening's near edge, or with `centre` to its middle. `sill` is its
  bottom above the wall's base.
- A slab's outline is the slab's edge in plan; its top face is
  `top_offset` above its level and it is `thickness` deep below that.
- A roof's outline is the wall plate, where the slopes start; `overhang`
  reaches past it. A shed roof rises from outline edge `edge`; a gable
  roof's ridge runs along it.
- An object from the catalogue is placed by its origin, the middle of its
  back edge (or by its middle, with `"anchor": "centre"`); at a `rotation`
  of 0 its back is towards +y (north). `creabim catalogue` lists the keys.
- A room is the space the walls close off round its `at` point, unless
  you give it an `outline`.
- A section or elevation looks to the left of its line: a line drawn west
  to east (`[0, 0]` to `[10, 0]`) looks north.
- A viewport puts a view's drawing on a sheet: `at` is where on the sheet
  (mm from its lower left corner), `target` the drawing point that goes
  there (left out: the middle of the crop, or of the whole drawing), `crop`
  the part of the drawing shown.

## Checking a file

`creabim build --check file.json` reads the file and builds the model in
memory, and reports every problem it finds, each at its JSON path:

```text
house.json: $.walls[3].thikness: unknown field `thikness`: did you mean `thickness`?
house.json: $.openings[0].wall: there is no wall with the id "W9"; did you mean "W1"?
house.json: $.openings[2]: it runs from 3.600 to 4.500 m along its wall, which is 4.200 m long
house.json: $.views[1].level: `level` is not used by a section view
4 problems; nothing was built
```

Fields the format doesn't know are errors, never ignored, so a misspelt
field can't silently fall back to its default. An element with a problem
is left out of the check that follows, so what refers to it doesn't repeat
the message. Problems with the file's shape (unknown fields, wrong types,
missing fields) are reported first; what refers to what is checked once
the shape is right. A file with problems builds nothing; the command fails with a
non-zero exit code, which makes it usable in a script or a CI job.

Some things build but look wrong; those are printed as warnings (a window
whose top is above its wall's).

## Exporting

`creabim export-json project.creabim out.json` writes a project in the
format, ids made from the elements' names (with `-2`, `-3` added where
names repeat). Building the exported file gives back the same model for
everything the format covers: exporting that again gives the same file.
What the format can't describe yet is left out and listed on the terminal,
for example:

```text
left out: 14 Tag (annotation is not in the format yet)
left out: the title block template of 12 sheets (they get the built-in one)
```

## Versions and what comes next

The format is versioned by the top-level `version`. This is version 1.
CreaBIM refuses a file of a newer version with a message naming the version,
rather than misreading it. New elements and fields are added to the format
as new optional fields and lists, so a version 1 file stays valid; a change
that would make an existing file mean something else raises the version.

### Not yet in the format

These exist in CreaBIM but can't be written in a file yet. A model made
from a file can have them added in the app, and `export-json` lists any it
leaves out.

- Annotation: dimensions, spot levels, text styles, a tag's room fields,
  and note leaders that point at an element (rather than a place).
- Title block templates (sheets get the built-in title block) and mark
  numbering rules.
- Per-view settings: visibility (hidden categories and elements), plan
  graphics and view range, underlays, level heads, crop boxes of 3D views,
  view templates and style packs.
- Curtain walls and curtain wall types, and openings in curtain walls.
- Footings, shafts, ceilings, room separators, sketches, guides, free
  meshes (with their modifiers) and booleans between elements.
- Objects other than the starter catalogue's (a catalogue folder of your
  own, or objects imported from Blender, FreeCAD or IFC), and footing types.
- Wall joins switched off at a wall's end.
- Join rules between walls (which wall cuts which, cleanup switched off)
  and 2D detail components drawn in detail views. Callout views and layer
  join priorities are in the format.
- Elements added by work in progress (graphic styles and view ranges)
  will join the format as they land.

## Reference

Every object of the format, field by field. This section is made from the
same declarations the reader uses, so it can't drift from what CreaBIM
accepts; the schema is made from them too.

<!-- reference:start (made by `CREABIM_BLESS=1 cargo test -p creabim-spec`) -->

### LengthIn

A unit lengths are written in.

- `"m"`
- `"mm"`
- `"cm"`
- `"ft"`
- `"in"`

### DisplayLength

How lengths are shown.

- `"mm"`
- `"cm"`
- `"m"`
- `"ft-in"`: Feet and fractional inches: 3'-6 1/2".
- `"ft"`: Decimal feet.
- `"in"`: Fractional inches.

### DisplayUnits

The project's display units.

| Field | Type | | Description |
|---|---|---|---|
| `length` | [DisplayLength](#displaylength) | required |  |
| `precision` | integer | default `0` | Decimal places; for feet and inches, the smallest fraction of an inch as a power of two (3 is 1/8"). |

### Hatch

The pattern on a material's cut faces.

- `"empty"`
- `"solid"`
- `"diagonal"`
- `"cross"`
- `"brick"`
- `"concrete"`
- `"timber"`

### RatePer

What a material's rate is priced per.

- `"m2"`
- `"m3"`
- `"m"`
- `"each"`

### Material

What elements are made of: their colour in 3D and the hatch on their cut faces. Start from a library material by its name, or give the values yourself; any value given overrides the library's.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  | Shown in the app; the id when left out. |
| `library` | string |  | A material of CreaBIM's library, by name ("Concrete, cast in place"), to start from. |
| `hatch` | [Hatch](#hatch) |  |  |
| `colour` | colour |  |  |
| `category` | string |  | IFC's material category: concrete, brick, wood, steel, glass... |
| `description` | string |  |  |
| `roughness` | number |  | 0 mirror-smooth to 1 fully matte. |
| `metallic` | number |  | 0 for almost everything, 1 for bare metal. |
| `opacity` | number |  | 1 opaque; glass is about 0.3. |
| `rate` | number |  | Its price for the take-off, in the project's currency, for each `rate_per`. |
| `rate_per` | [RatePer](#rateper) |  | What `rate` prices: square metres (the default), cubic metres, metres or pieces. |

### AssemblyUsage

What an assembly is for.

- `"wall"`
- `"floor"`
- `"roof"`

### LayerFunction

What a layer does. The core runs from the first structure layer to the last.

- `"structure"`
- `"substrate"`
- `"insulation"`
- `"air-gap"`
- `"membrane"`
- `"finish"`

### Layer

One layer of an assembly.

| Field | Type | | Description |
|---|---|---|---|
| `function` | [LayerFunction](#layerfunction) | default `"structure"` |  |
| `material` | string |  | A material's id; none for an air gap. |
| `thickness` | number | required |  |
| `priority` | integer |  | Which layer wins where walls join: 1 cuts through 5. Left out, it follows the layer's function (structure 1, finish 5). |

### Assembly

A layered build-up. Walls list layers from the exterior face in; floors and roofs from the top down. An element built of one is as thick as its layers.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `usage` | [AssemblyUsage](#assemblyusage) | default `"wall"` |  |
| `layers` | array of [Layer](#layer) |  |  |
| `description` | string |  |  |

### TypeCategory

What a type is for.

- `"wall"`
- `"door"`
- `"window"`
- `"slab"`
- `"roof"`
- `"column"`
- `"beam"`

### Location

The line a wall is drawn along and kept on when it gets thicker. The exterior is on the wall's right, looking from start to end, unless the wall is flipped.

- `"centre"`
- `"core-centre"`
- `"exterior-face"`
- `"interior-face"`
- `"core-exterior"`
- `"core-interior"`
- `"layer-boundary"`: One of the boundaries between layers, named by `boundary`.

### Frame

How a door or window is built inside its opening.

| Field | Type | | Description |
|---|---|---|---|
| `width` | number | default `0.06` | Face width of the frame members. |
| `depth` | number | default `0.1` | Depth of the frame through the wall. |
| `panes_across` | integer | default `1` | Window panes side by side. |
| `panes_up` | integer | default `1` | Window panes one above the other. |

### Rect

A rectangle by its lower left and upper right corners.

| Field | Type | | Description |
|---|---|---|---|
| `min` | [x, y] | required |  |
| `max` | [x, y] | required |  |

### Screen

The repeating unit of a screen wall (breeze block, louvres, lattice).

| Field | Type | | Description |
|---|---|---|---|
| `unit` | [x, y] | required | The unit's size, joints included: along the wall and up. |
| `holes` | array of [Rect](#rect) |  | The open rectangles of one unit, from its lower left corner. |
| `frame` | number | default `0.0` | A solid border this wide round the screen. |
| `stagger` | boolean | default `false` | Every other course shifted half a unit along. |

### Type

An element type: shared values its instances take, so editing the type in the app changes them all. An instance names it in its `type` field and may still set any of those values itself.

Which fields a category uses: wall: `thickness` (or `assembly`), `material`, `location`, `height`, `screen`; door and window: `width`, `height`, `sill`, `frame`, `material`; slab and roof: `thickness` (or `assembly`), `material`; column: `width` and `depth`, or `diameter`, and `material`; beam: `width`, `depth`, `material`.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `category` | [TypeCategory](#typecategory) | required |  |
| `description` | string |  |  |
| `thickness` | number |  |  |
| `assembly` | string |  | An assembly's id. |
| `material` | string |  | A material's id. |
| `location` | [Location](#location) |  | A wall type's location line. |
| `boundary` | integer |  | With location `layer-boundary`: which boundary, counted from the exterior face (0) to the interior face (the layer count). |
| `height` | number |  | A wall type's height for new walls; a door or window's height. |
| `width` | number |  |  |
| `sill` | number |  |  |
| `frame` | [Frame](#frame) |  |  |
| `depth` | number |  |  |
| `diameter` | number |  |  |
| `screen` | [Screen](#screen) |  | Makes a wall type a screen wall: a repeating unit with holes. |

### Level

A storey. Elements hang off a level, so moving it moves them.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `elevation` | number | required | Height of its floor above the project's zero. |
| `height` | number | default `3.0` | Floor-to-floor height: the height walls on it take unless told otherwise. |

### GridLabels

Grid labels.

- `"numbers"`
- `"letters"`: A, B, C... (I and O are skipped), then AA, AB...

### GridGroup

A grid system: how its lines are labelled and coloured.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `prefix` | string | default `""` | Put before every label: "A-" gives A-1, A-2... |
| `vertical` | [GridLabels](#gridlabels) | default `"letters"` | How lines running north-south are labelled, west to east. |
| `horizontal` | [GridLabels](#gridlabels) | default `"numbers"` | How lines running east-west are labelled, south to north. |
| `colour` | colour |  |  |

### Grid

A grid line.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `label` | string |  | Its label after its group's prefix; the id when left out. |
| `start` | [x, y] | required |  |
| `end` | [x, y] | required |  |
| `group` | string |  | A grid group's id. |

### Wall

A straight wall along its centreline on a level.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `mark` | string |  | Its mark, shown by tags and schedules; none numbers it automatically. |
| `level` | string | required | A level's id. |
| `start` | [x, y] | required |  |
| `end` | [x, y] | required |  |
| `type` | string |  | A wall type's id: the wall takes its thickness, assembly, material, location line and height. |
| `thickness` | number |  | Needed unless the wall has a type or an assembly. |
| `height` | number |  | Height above its base. Left out: up to `top_level` when given, else the type's height, else its level's height. |
| `base_offset` | number | default `0.0` | Its base above its level. |
| `top_level` | string |  | A level's id: the top stays `top_offset` above that level as levels move. |
| `top_offset` | number | default `0.0` |  |
| `material` | string |  |  |
| `assembly` | string |  |  |
| `location` | [Location](#location) |  |  |
| `boundary` | integer |  | With location `layer-boundary`: which boundary, counted from the exterior face (0) to the interior face (the layer count). |
| `flip` | boolean | default `false` | The exterior face on the left instead of the right. |
| `attach_top` | array of string |  | Roofs and slabs (ids) the top follows the underside of. |

### OpeningKind

What an opening is.

- `"door"`
- `"window"`
- `"hole"`

### Opening

A door, window or bare hole in a wall, placed by its distance along the wall from the wall's start.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `mark` | string |  |  |
| `kind` | [OpeningKind](#openingkind) | required |  |
| `wall` | string | required | The host wall's id. |
| `offset` | number |  | From the wall's start to the opening's near edge. Give this or `centre`. |
| `centre` | number |  | From the wall's start to the opening's middle. |
| `type` | string |  | A door or window type's id: the opening takes its width, height, sill, frame and material. |
| `width` | number |  | Needed unless the opening has a type. |
| `height` | number |  | Needed unless the opening has a type. |
| `sill` | number |  | Its bottom above the wall's base. Left out: 0 for doors and holes, 0.9 m for windows (or the type's). |
| `material` | string |  |  |
| `frame` | [Frame](#frame) |  |  |
| `flip_hand` | boolean | default `false` | A door hinged on the jamb at the wall's end side. |
| `flip_facing` | boolean | default `false` | A door opening to the wall's exterior; a window facing its interior. |

### Slab

A floor slab: its outline pushed down from its top face.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `level` | string | required |  |
| `outline` | array of [x, y] |  | The corners in plan, either way round. |
| `bulges` | array of number |  | How each edge bends (corner i to i + 1): the tangent of a quarter of the arc's angle, positive bulging left. 0 or left out is straight. |
| `holes` | array of array of [x, y] |  | Holes through it, each an outline inside it. |
| `thickness` | number |  | Needed unless the slab has a type or an assembly. |
| `top_offset` | number | default `0.0` | Its top face above its level; negative for below. |
| `type` | string |  |  |
| `material` | string |  |  |
| `assembly` | string |  |  |

### RoofShape

A roof's shape.

- `"flat"`
- `"shed"`: One slope, rising from the `edge`.
- `"gable"`: Two slopes meeting at a ridge, with upright gable ends.
- `"hip"`: A slope from every edge.

### Roof

A roof over a wall-plate outline.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `level` | string | required |  |
| `outline` | array of [x, y] |  | Where the slopes start, in plan. |
| `shape` | [RoofShape](#roofshape) | default `"hip"` |  |
| `pitch` | number |  | Slope in degrees. Needed unless the roof is flat. |
| `overhang` | number | default `0.0` | How far it reaches past the outline, in plan. |
| `thickness` | number |  | Square to the slope. Needed unless the roof has a type or an assembly. |
| `base_offset` | number | default `0.0` |  |
| `edge` | integer | default `0` | The outline edge a shed roof rises from, or a gable roof's ridge runs along (edge i runs from corner i to i + 1). |
| `type` | string |  |  |
| `material` | string |  |  |
| `assembly` | string |  |  |
| `holes` | array of array of [x, y] |  | Holes cut upright through it (roof lights, a chimney). |

### Column

A column: a rectangular (`width` × `depth`) or round (`diameter`) profile standing on a level.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `mark` | string |  |  |
| `level` | string | required |  |
| `at` | [x, y] | required | The profile's centre in plan. |
| `type` | string |  |  |
| `width` | number |  |  |
| `depth` | number |  |  |
| `diameter` | number |  |  |
| `rotation` | number | default `0.0` | Turn of the profile, degrees anticlockwise. |
| `base_offset` | number | default `0.0` |  |
| `height` | number |  | A fixed height. Left out: up to `top_level`, else to the next level up. |
| `top_level` | string |  |  |
| `top_offset` | number | default `0.0` |  |
| `material` | string |  |  |

### Beam

A beam, its top `top_offset` below the top of its storey.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `mark` | string |  |  |
| `level` | string | required |  |
| `start` | [x, y] | required |  |
| `end` | [x, y] | required |  |
| `type` | string |  |  |
| `width` | number |  |  |
| `depth` | number |  | Height of the section. |
| `top_offset` | number | default `0.0` |  |
| `material` | string |  |  |

### Side

Left or right.

- `"left"`
- `"right"`

### Landing

A landing that turns a stair.

| Field | Type | | Description |
|---|---|---|---|
| `after` | integer | required | The number of risers before it. |
| `turn` | [Side](#side) | required |  |

### Stair

A stair: one flight, or two with a landing between.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `level` | string | required |  |
| `start` | [x, y] | required | The middle of the first riser, in plan. |
| `direction` | number | default `0.0` | Which way the first flight climbs, degrees anticlockwise from x. |
| `width` | number | required |  |
| `max_riser` | number | default `0.18` | The tallest riser allowed; the risers share the height equally. |
| `tread` | number | default `0.28` | The depth of each tread. |
| `height` | number |  | A fixed rise. Left out: up to the next level. |
| `base_offset` | number | default `0.0` |  |
| `waist` | number | default `0.15` | Thickness of the sloping slab under the steps. |
| `landing` | [Landing](#landing) |  |  |
| `material` | string |  |  |

### Railing

A railing along a path, or along one side of a stair.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `level` | string | required |  |
| `path` | array of [x, y, z] |  | Points of its path, heights above the level. Not used when it follows a stair. |
| `height` | number | default `1.0` |  |
| `post_spacing` | number | default `1.0` |  |
| `stair` | string |  | A stair's id: the railing follows its `side`. |
| `side` | [Side](#side) |  |  |
| `material` | string |  |  |

### Plane

A plane anywhere in the model: its origin (height above the level) and two square unit axes along it.

| Field | Type | | Description |
|---|---|---|---|
| `origin` | [x, y, z] | required |  |
| `x` | [x, y, z] | required |  |
| `y` | [x, y, z] | required |  |

### ArcPath

A sweep's arc: a point on the turn axis, the turn direction, where the arc begins and how far it turns (degrees, positive by the right-hand rule about the axis). A full turn closes on itself. The profile rides the arc: its `y` runs along `axis`, its `x` to the path's left.

| Field | Type | | Description |
|---|---|---|---|
| `center` | [x, y, z] | required |  |
| `axis` | [x, y, z] | required |  |
| `start` | [x, y, z] | required |  |
| `sweep` | number | required |  |

### WallEdge

A wall's top or base.

- `"top"`
- `"base"`

### Part

An extrusion of a plan outline, or a sweep of a profile along a path. A void cuts what it meets (a firebox in a chimney, a hole through a slab) instead of being solid.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `level` | string | required |  |
| `outline` | array of [x, y] |  | An extrusion's outline in plan. |
| `holes` | array of array of [x, y] |  | An extrusion's holes. |
| `base` | number | default `0.0` | An extrusion's bottom above the level. |
| `height` | number |  | An extrusion's height. |
| `plane` | [Plane](#plane) |  | An extrusion drawn on a face: its outline lies on this plane, and `base` and `height` run along the plane's normal (`x` × `y`). |
| `profile` | array of [x, y] |  | A sweep's section: `x` across the path (positive to its left), `y` up from it. |
| `path` | array of [x, y, z] |  | A sweep's path through points, `z` above the level. |
| `closed` | boolean | default `false` | The path runs from its last point back to its first. |
| `face_normal` | [x, y, z] |  | A path drawn on a face: the face's outward normal, which the profile's `y` runs along instead of up. |
| `arc` | [ArcPath](#arcpath) |  | A sweep round an arc: the profile rides the arc, its `y` along the turn axis. A full turn (sweep 360) closes on itself. |
| `wall` | string |  | A sweep along a wall's id, at its top or base: it follows the wall (a coping, a plinth). |
| `edge` | [WallEdge](#walledge) | default `"top"` |  |
| `offset` | number | default `0.0` | Lifts a wall path off its edge. |
| `extend` | [x, y] |  | Runs a wall path on past the wall's start and end. |
| `void` | boolean | default `false` |  |
| `material` | string |  |  |
| `ifc_class` | string |  | The IFC class it exports as. |

### SiteKind

What a site element is.

- `"terrain"`
- `"area"`
- `"plant"`

### PatternKind

A surface pattern.

- `"none"`
- `"units"`: Flags, blocks, setts.
- `"lines"`: Boards, decking.
- `"stipple"`: Gravel, sand.

### SitePattern

An area's surface: units (`size`, `stagger`, `angle`), lines (`spacing`, `angle`) or stipple (`spacing`).

| Field | Type | | Description |
|---|---|---|---|
| `kind` | [PatternKind](#patternkind) | required |  |
| `size` | [x, y] |  |  |
| `stagger` | boolean | default `false` |  |
| `angle` | number | default `0.0` |  |
| `spacing` | number |  |  |

### PlantForm

A plant's form.

- `"tree"`
- `"conifer"`
- `"palm"`
- `"shrub"`

### SiteElement

A site element: terrain, a hard landscape area or a plant.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `kind` | [SiteKind](#sitekind) | required |  |
| `level` | string | required |  |
| `outline` | array of [x, y] |  | Terrain and area: the outline. |
| `holes` | array of array of [x, y] |  | An area's holes. |
| `top` | number | default `0.0` | Terrain and area: the top above the level. |
| `thickness` | number |  | Terrain and area: how deep. |
| `points` | array of [x, y, z] |  | Terrain: spot heights `[x, y, z]`, `z` above the level. |
| `contour` | number | default `0.0` | Terrain: a contour every so much height; 0 draws none. |
| `pattern` | [SitePattern](#sitepattern) |  | An area's surface pattern. |
| `at` | [x, y] |  | A plant's position. |
| `base` | number | default `0.0` | A plant's base above the level. |
| `plant` | [PlantForm](#plantform) | default `"tree"` |  |
| `canopy` | number |  | A plant's crown across. |
| `height` | number |  | A plant's height. |
| `material` | string |  |  |

### Anchor

Which point of an object `at` gives.

- `"origin"`: The middle of its back edge.
- `"centre"`: The middle of its footprint.

### Object

Furniture or a fixture from CreaBIM's catalogue.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `mark` | string |  |  |
| `level` | string | required |  |
| `catalogue` | string | required | The catalogue object's key: "starter/sofa-3" (`creabim catalogue` lists them; "starter/" may be left out). |
| `at` | [x, y] | required | Where it goes in plan: its origin (the middle of its back edge), or its middle with `anchor` "centre". |
| `anchor` | [Anchor](#anchor) | default `"origin"` |  |
| `rotation` | number | default `0.0` | Turn, degrees anticlockwise. At 0 its back is towards +y. |
| `offset` | number |  | Its floor above the level (for one hung from the ceiling, its top below the ceiling). Left out: the catalogue's. |

### Room

A room: the space the walls close off round `at`, or an outline drawn for it.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  |  |
| `number` | string | default `""` |  |
| `level` | string | required |  |
| `at` | [x, y] | required | A point inside it, where its tag sits. |
| `outline` | array of [x, y] |  | An outline of its own; left out, the walls round `at` bound it. |
| `height` | number |  | Its top above its level. Left out: to the ceiling, or the level's height. |
| `floor_finish` | string | default `""` |  |
| `wall_finish` | string | default `""` |  |
| `ceiling_finish` | string | default `""` |  |
| `occupancy` | string | default `""` |  |
| `department` | string | default `""` |  |

### ViewKind

What a view is.

- `"plan"`
- `"section"`: Cut along its line.
- `"elevation"`: Seen from its line, nothing cut.
- `"3d"`: A parallel projection of the whole model.
- `"schedule"`

### ViewStyle

How a view draws.

- `"hidden-line"`
- `"shaded"`
- `"wireframe"`

### ScheduleCategory

What a schedule lists.

- `"doors"`
- `"windows"`
- `"walls"`
- `"slabs"`
- `"structure"`: Columns and beams.
- `"furniture"`
- `"fixtures"`
- `"rooms"`
- `"materials"`: The take-off by material.

### Callout

The box of a parent view a callout enlarges, in that view's drawing coordinates.

| Field | Type | | Description |
|---|---|---|---|
| `parent` | string | required | The parent view's id. |
| `min` | [x, y] | required |  |
| `max` | [x, y] | required |  |

### View

A view: a plan of a level, a section or elevation along a line, a 3D view, or a schedule. Which fields it uses depends on its `kind`: plan: `level`, `cut_height`, `ceiling_plan`; section and elevation: `start`, `end`, `depth`; 3d: `azimuth`, `altitude`; schedule: `category`, `by_type`.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required |  |
| `name` | string |  | Its title; the id when left out. |
| `kind` | [ViewKind](#viewkind) | required |  |
| `scale` | number | default `100.0` | The scale's denominator: 100 is 1:100. |
| `style` | [ViewStyle](#viewstyle) |  | Left out: shaded for 3D views, hidden line for the rest. |
| `level` | string |  | A plan's level. |
| `cut_height` | number | default `1.2` | A plan's cut above its level. |
| `ceiling_plan` | boolean | default `false` | A reflected ceiling plan. |
| `room_colours` | boolean | default `false` | Each room washed in a colour of its own. |
| `start` | [x, y] |  | A section or elevation's line. It looks to the left of start → end: a line drawn west to east looks north. |
| `end` | [x, y] |  |  |
| `depth` | number |  | How far a section or elevation looks. |
| `azimuth` | number | default `30.0` | A 3D view's compass bearing: 0 from the south, anticlockwise. |
| `altitude` | number | default `30.0` | A 3D view's angle above the horizon. |
| `category` | [ScheduleCategory](#schedulecategory) |  | What a schedule lists. |
| `by_type` | boolean | default `false` | A schedule's rows grouped by type. |
| `callout` | [Callout](#callout) |  | Makes it a callout of another view: an enlarged part of it. Give it the same kind and line (or level) as its parent. |

### PaperSize

ISO paper sizes, landscape.

- `"A0"`
- `"A1"`
- `"A2"`
- `"A3"`
- `"A4"`

### Viewport

A view placed on a sheet.

| Field | Type | | Description |
|---|---|---|---|
| `view` | string | required | A view's id. |
| `at` | [x, y] | required | Where on the sheet, in millimetres from its lower left corner. |
| `target` | [x, y] |  | The drawing point (in the file's units, as the drawing's x and y) put at `at`. Left out: the middle of the crop, or of the whole drawing. |
| `crop` | [Rect](#rect) |  | Only this part of the drawing is shown (drawing coordinates). |

### Sheet

A drawing sheet, with views placed on it. Its id is its number.

| Field | Type | | Description |
|---|---|---|---|
| `id` | string | required | Its number: "A101". |
| `name` | string |  | Its title; the number when left out. |
| `size` | [PaperSize](#papersize) |  | An ISO paper size, landscape. Give this or `width` and `height`. |
| `width` | number |  | In millimetres. |
| `height` | number |  |  |
| `fields` | object of strings |  | Title block fields by name. |
| `viewports` | array of [Viewport](#viewport) |  |  |

### Tag

A tag in a view, showing an element's mark (a door's, a window's) or a room's name, number and area. It moves with its element.

| Field | Type | | Description |
|---|---|---|---|
| `view` | string | required | A view's id. |
| `element` | string | required | The id of the element it tags. |
| `offset` | [x, y] | default `[0.0, 0.0]` | Where it sits, from the element's middle, in drawing units. |
| `elbow` | [x, y] |  | A bend in its leader, from the element's middle. |
| `end` | [x, y] |  | Where its leader ends, from the element's middle; left out, at the middle. |

### Note

Text in a view (drawing coordinates, in the file's units) or on a sheet (paper millimetres).

| Field | Type | | Description |
|---|---|---|---|
| `view` | string | required | A view's id, or a sheet's. |
| `at` | [x, y] | required | Where its first line starts: its left end, mid-height. |
| `text` | string | required | Line breaks (`\n`) start new lines. |
| `leader` | [x, y] |  | A leader from the text to this point, in the same units as `at`. |
| `elbow` | [x, y] |  | A bend in the leader. |
| `colour` | colour |  |  |

### Model

A whole model: the file's top level. Every list may be left out. Ids are chosen by you, are unique across the whole file, and are what elements use to refer to each other (a door names its wall's id, a plan names its level's id).

| Field | Type | | Description |
|---|---|---|---|
| `$schema` | string |  | Where an editor finds this schema; ignored by CreaBIM. |
| `version` | integer | required | The format version the file is written to. This CreaBIM reads 1. |
| `units` | [LengthIn](#lengthin) | default `"m"` | The unit every length in the file is in, except the sizes and positions on sheets, which are always paper millimetres. Angles are always degrees. |
| `display_units` | [DisplayUnits](#displayunits) |  | How the project shows and takes lengths (the model is metres whatever this says). |
| `materials` | array of [Material](#material) |  |  |
| `assemblies` | array of [Assembly](#assembly) |  | Layered build-ups for walls, floors and roofs. |
| `types` | array of [Type](#type) |  | Element types: wall types, door and window types, slab, roof, column and beam types. |
| `levels` | array of [Level](#level) |  |  |
| `grid_groups` | array of [GridGroup](#gridgroup) |  |  |
| `grids` | array of [Grid](#grid) |  |  |
| `walls` | array of [Wall](#wall) |  |  |
| `openings` | array of [Opening](#opening) |  | Doors, windows and bare holes, each in a wall. |
| `slabs` | array of [Slab](#slab) |  |  |
| `roofs` | array of [Roof](#roof) |  |  |
| `columns` | array of [Column](#column) |  |  |
| `beams` | array of [Beam](#beam) |  |  |
| `stairs` | array of [Stair](#stair) |  |  |
| `railings` | array of [Railing](#railing) |  |  |
| `parts` | array of [Part](#part) |  | Extrusions and sweeps, solid or voids that cut what they meet. |
| `site` | array of [SiteElement](#siteelement) |  | Terrain, hard landscape and planting. |
| `objects` | array of [Object](#object) |  | Furniture and fixtures from the catalogue. |
| `rooms` | array of [Room](#room) |  |  |
| `views` | array of [View](#view) |  |  |
| `sheets` | array of [Sheet](#sheet) |  |  |
| `tags` | array of [Tag](#tag) |  | Tags of doors, windows, rooms and other elements, in views. |
| `notes` | array of [Note](#note) |  | Text written in a view or on a sheet. |

<!-- reference:end -->
