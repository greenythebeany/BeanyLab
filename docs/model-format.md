# BeanyLab model format (`.bmdl`), version 1

A model is a UTF-8 JSON document. It's plain text on purpose: models diff and merge well in version control, and other tools can generate them.

## Top level

```json
{
  "version": 1,
  "name": "pid_control",
  "solver": { "solver": "ode45", "stop_time": "10", "rel_tol": "1e-6" },
  "blocks": [ ... ],
  "lines": [ ... ]
}
```

| Field | Type | Meaning |
|---|---|---|
| `version` | integer | Format version. Readers refuse files with a newer version than they support. |
| `name` | string | Model name; the editor sets it from the file name on save. |
| `solver` | object | Simulation settings (below). Every field is optional. |
| `blocks` | array | Blocks at this level. |
| `lines` | array | Signal connections at this level. |

### Solver settings

All values are strings, so they can hold expressions evaluated in the workspace (for example `"Tend"`).

| Field | Default | Meaning |
|---|---|---|
| `solver` | `auto` | `auto`, `ode45`, `ode23` (variable step), `ode23s` (variable step, stiff), `ode4`, `ode2`, `ode1` (fixed step), `discrete`. `auto` picks `ode45` when the model has continuous states, else `discrete`. |
| `stop_time` | `10` | Simulation end time in seconds; `inf` runs until a Stop Simulation block or the Stop button. |
| `max_step` | `auto` | Largest variable step. `auto` = stop time / 200. |
| `fixed_step` | `auto` | Step for fixed-step solvers. `auto` = smallest discrete sample time, at most stop time / 50. |
| `rel_tol` | `1e-3` | Relative error tolerance (variable step). |
| `abs_tol` | `1e-6` | Absolute error tolerance (variable step). |

## Blocks

```json
{ "id": "plant", "type": "TransferFcn", "name": "Motor", "x": 400, "y": 100, "w": 90, "h": 50,
  "params": { "num": "1", "den": "[1 1]" } }
```

| Field | Type | Meaning |
|---|---|---|
| `id` | string | Unique within its level; lines refer to it. Not shown to users. |
| `type` | string | Block type, one of the library types (see `block_library` in the session API, or the editor's palette). |
| `name` | string | Display name, unique within its level. `get_param`/`set_param` paths use names: `model/Subsystem/Block`. |
| `x`, `y`, `w`, `h` | number | Position and size in editor units (pixels at 100% zoom). Defaults: `w` 70, `h` 50. |
| `params` | object | Parameter values as strings, exactly as typed in the dialog. Missing parameters use the library defaults. Parameters of kind `expr` are evaluated in the base workspace when the simulation starts; `text` and `choice` parameters are used as written. |
| `system` | object | Only for `Subsystem` and `EnabledSubsystem`: a nested model with `blocks` and `lines` (its `solver` is ignored). |

### Subsystems

A subsystem's input ports are its `Inport` blocks and its output ports are its `Outport` blocks, numbered by their `port` parameter. An `EnabledSubsystem` has one extra input after the data inputs: while that enable signal is not positive, the blocks inside keep their outputs and states.

At the top level, `Inport` blocks read 0 and `Outport` blocks are logged to `out.yout`.

## Lines

```json
{ "src": { "block": "pid", "port": 1 }, "dst": { "block": "plant", "port": 1 }, "points": [[300, 120]] }
```

| Field | Meaning |
|---|---|
| `src` | Source block id and 1-based output port. |
| `dst` | Destination block id and 1-based input port. Each input port takes at most one line. |
| `points` | Optional routing waypoints; the editor draws orthogonal routes when absent. |

One output can feed many inputs (fan-out): write one line per destination.

`Goto` and `From` blocks with the same `tag` are connected without a line. Tags are global to the model and each tag has one Goto.

A `BusSelector` reads a bus made by a `BusCreator`, possibly through Goto/From and subsystem ports. It picks elements by the names in the creator's `names` parameter and has one output per name in its `select` parameter.

## Results

`sim('model')` returns (or assigns to `out` when called without an output):

- `out.tout`: column of logged times (every major solver step).
- `out.<variable>`: one matrix per To Workspace block, one row per logged time.
- `out.yout`: top-level Outport signals, side by side in port order.

Each Scope block draws a figure named after the block.

## Compatibility promise

Version 1 readers ignore unknown fields, so later versions can add optional data without breaking older BeanyLab releases. A change that older readers would misread increments `version`.
