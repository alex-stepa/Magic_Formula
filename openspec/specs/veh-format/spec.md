## Purpose

The custom `.veh` format (JSON) describes one car as a node-beam structure: nodes with mass,
beams between them, wheels, surface triangles, and optionally engine/gearbox/brakes/layout.
The game only **reads** it (`src/veh.rs`, serde `Deserialize`); writing is the converter's job.

## Requirements

### Requirement: Backwards-compatible format

Older `.veh` files MUST keep loading after new fields are added to the format. Every new
field MUST therefore be `Option<T>` or carry `#[serde(default)]`; only `name`, `nodes` and
`beams` stay mandatory.

#### Scenario: Old .veh without engine and gearbox

- **WHEN** a `.veh` containing only `name`, `version`, `nodes`, `beams` is loaded
- **THEN** deserialization succeeds and `engine`, `gearbox`, `brakes`, `layout` are `None`
- **AND** `wheels` and `triangles` are empty vectors
- **AND** this is pinned by the test `veh::tests::engine_gearbox_optional`

#### Scenario: A new field is added to the format

- **WHEN** a new field is introduced
- **THEN** it MUST be optional (`Option`) or have `#[serde(default)]`
- **AND** existing `.veh` files load unchanged

### Requirement: Unknown keys are ignored

The format also carries keys the game does not need (`version`, `wheels.name`, `wheels.dir`, …).
The game MUST declare only the fields it reads and silently ignore the rest; the full format
is documented in README.

#### Scenario: .veh contains keys outside the game structs

- **WHEN** a `.veh` contains keys absent from the structs in `veh.rs`
- **THEN** loading succeeds and the unknown keys are ignored without error

### Requirement: Elements reference nodes by string id

Beams, wheels and triangles MUST reference nodes by their string `id`, not by index — the
converter emits jbeam names, and the format has to stay readable and independent of node order.

#### Scenario: Beam references its nodes

- **WHEN** a beam carries `a: "n1"`, `b: "n2"`
- **THEN** the loader resolves both names to `SoftBody` node indices while building the car

#### Scenario: Wheel references two mounts

- **WHEN** a wheel carries `node1` and `node2`
- **THEN** those are its mount nodes
- **AND** when `center` is absent, the wheel centre is approximated as the average of the
  two mount positions

### Requirement: Missing physical values fall back in the game layer

Data coming from `.jbeam` is not always complete (beam materials can be missing, tire parts
often live in BeamNG common content outside the mod). The format MUST carry such values as
`0` / `None` and the **game** MUST supply the fallback — the format itself never scales or
derives values.

#### Scenario: Beam without material

- **WHEN** a beam has `stiffness: 0` and `damping: 0` (the JSON keys were absent)
- **THEN** the loader applies the game layer's fallback stiffness/damping
- **AND** values that ARE present in the `.veh` are used as-is, with no scaling

#### Scenario: Wheel without dimensions

- **WHEN** a wheel has `radius: 0` or `width: 0`
- **THEN** the game supplies a fallback tire dimension

### Requirement: Optional beam precompression and asymmetric damping

A beam MAY carry a rest length `rest` (jbeam `beamPrecompression`) and rebound damping
`rebound` (jbeam `beamDampRebound`). When they are absent the loader MUST fall back to the
spawn distance of the nodes as rest length and to symmetric damping.

#### Scenario: Beam without `rest`

- **WHEN** a beam has no `rest`
- **THEN** its rest length is the distance between its two nodes at spawn time

#### Scenario: Beam without `rebound`

- **WHEN** a beam has `rebound: 0`
- **THEN** damping is symmetric and equals `damping`
