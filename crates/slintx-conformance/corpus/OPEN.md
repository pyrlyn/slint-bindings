# OPEN items

Behaviour the Slint 1.18.0 docs leave open, from `research.md` §8.4.1 and
§8.4.2, in the order they appear there. Every id is named by at least one corpus
file (`// OPEN Ox` in its header); `cases_cover_every_open_item` enforces it.
Stage 1 records whether Slint accepts the construct; the runtime half of an item
(values, timing, order) is settled by the later stages on the same case.

- O1: unterminated block comment
- O2: lexing of `a-b` and `1-2`
- O3: a number with an unknown unit suffix (`10foo`)
- O4: imports placed after their use
- O5: exporting an imported name
- O6: cyclic imports and `std-widgets.slint` resolution
- O7: `int` width and overflow
- O8: `property<int>` without a space
- O9: the `grad` angle unit
- O10: brush to color conversion
- O11: chains of one logical operator (`a && b && c`)
- O12: division by zero
- O13: int overflow, float equality, NaN, unit products (`px*px`)
- O14: sign of `mod`
- O15: string literals spanning lines
- O16: interpolating types other than numbers and strings
- O17: trailing comma in array literals
- O18: `-=`, `*=`, `/=` on plain numbers
- O19: `if` as an expression
- O20: `init` order
- O21: named arguments at the call site
- O22: float index; mutating a constant array
- O23: the CSS named color list
- O24: type of `.red`, `.green`, `.blue`, `.alpha`
- O25: color methods on a gradient brush
- O26: brush default, equality and conversion to color
- O27: `LogicalPosition`
- O28: per-property defaults of built-in enums
- O29: `round(-0.5)`
- O30: `Math.PI`, `Math.E`
- O31: inheritance: rebinding, redeclaring, shadowing
- O32: `parent` of a root; `self`, `parent`, `root` in repeaters and states
- O33: `interface` syntax
- O34: who may assign `in`, `out` and private properties
- O35: binding loops
- O36: constant folding
- O37: evaluation order; notification on an equal value
- O38: `%` on properties other than sizes
- O39: two-way links: own binding, convertible types, cycles
- O40: side-effecting built-ins
- O41: reentrancy, assigning `self`, recursion
- O42: `init` order across siblings
- O43: both the internal and the use-site `init` run
- O44: order and passes of `changed` handlers
- O45: global initialization order
- O46: re-instantiation when a model changes
- O47: `if` flipped twice, negative or float models, out-of-range index
- O48: lazy instantiation in ListView
- O49: `@children` with `inherits`
- O50: a bare `0` for a length
- O51: default size, negative sizes, `opacity` outside 0..1
- O52: transition from state A straight to state B
- O53: retargeting mid-transition; state change against `animate`
- O54: eager evaluation of `when`
- O55: animation fields re-read mid-animation
- O56: the easing list
- O57: animation sampling, `delay` with `reverse`, zero iterations
- O58: queries during an animation; `changed` per frame
- O59: Timer: first tick, 0 ms interval, `restart`, missed ticks
- O60: notifications from native models
