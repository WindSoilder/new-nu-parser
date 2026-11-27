
## Current judgement
Git-wise, the file is tracked and currently unmodified. Its latest file-specific commit is bc9b12d, whose message is “WIP with some improvement,” which accurately reflects its development status.

What is good:

- A coherent bidirectional design: expressions synthesize a type and are checked against an expected type (typechecker.rs:428 (src/typechecker.rs:428)).
- Fresh bounded variables support explicit generic commands.
- Subtyping includes numeric, list, record-width, union, and intersection types.
- Union/intersection construction includes useful normalization.
- The project compiles successfully.

What remains incomplete or incorrect:

- Record subtyping does not verify that all required fields were consumed (typechecker.rs:1349 (src/typechecker.rs:1349)). The missing_name test is consequently not diagnosed.
- Declared command output types are not checked against command bodies (typechecker.rs:880 (src/typechecker.rs:880)).
- Pipeline input types are ignored; all possible outputs are unioned instead (typechecker.rs:973 (src/typechecker.rs:973)).
- Call checking compares raw arguments positionally, so flags, optional parameters, defaults, and rest parameters are mishandled.
- Built-in resolved calls reach an unimplemented! branch (typechecker.rs:907 (src/typechecker.rs:907)).
- Tables are recognized as expressions but have no typechecking implementation.
- Heterogeneous lists fall back to list<any> rather than a union (typechecker.rs:437 (src/typechecker.rs:437)).
- Closures lack input/output types.
- Some errors are emitted twice through nested expression wrappers.
- constrain_subtype and is_subtype duplicate substantial logic, increasing the risk that they diverge.
- The implementation is a 1,992-line monolith with many explicit TODOs.


## Current status

| Area | Status | Evidence |
|---|---|---|
| Literals, annotations, variables | Mostly working | typecheck_bidirectional.nu, typecheck_variables.nu |
| Numeric and explicit union subtyping | Mostly working | typecheck_operators.nu, typecheck_unions.nu |
| Generic call inference | Promising | Fresh variables and union inference work in typecheck_generics.nu |
| Records | Partial | Missing age is caught, but missing trailing name is not |
| Lists and nested collections | Partial | Mixed lists fall back to any; expected element types are not propagated fully |
| Function calls | Partial | Simple positional arguments work; flags/defaults/rest/optional arguments do not |
| Function output contracts | Missing | Incorrect command bodies are accepted |
| Pipelines and overloads | Missing | All declared outputs are unioned instead of selecting by pipeline input |
| Mutation | Missing | Invalid assignments in typecheck_mutation.nu produce no type errors |
| Match patterns | Broken/partial | Literal patterns are sent through typecheck_node and cause spurious errors |
| Tables | Missing | AstNode::Table is recognized as an expression but not typechecked |
| Closures | Partial | Typed parameters affect bodies, but closures have only the coarse closure type |

## Recommended priorities

### P0 — Establish a trustworthy test oracle

Do this before accepting snapshots.

- Add assertions for every EXPECT-ERROR marker.
- Assert important inferred types, not only the entire compiler dump.
- Separate typechecker diagnostics from unrelated IR errors.
- Keep snapshots for inspection, but do not treat snapshot acceptance as semantic correctness.
- Add focused regression cases for:
  - no expected error,
  - missing expected error,
  - duplicate diagnostic,
  - unexpected diagnostic.

Right now, for example, bad_text produces duplicate errors while bad_items is not rejected. A snapshot can preserve that behavior without indicating that it is wrong.

### P0 — Fix basic bidirectional collection checking

Relevant tests:

- typecheck_bidirectional.nu
- typecheck_collections.nu
- typecheck_nested_types.nu

Tasks:

- Pass the expected element type to every list element, not only the first.
- Make an empty list adopt its expected type:

  let xs: list<int> = []
  should produce list<int>, not list<any>.
- Reject:

  let xs: list<int> = [1, "two"]
- Infer disjoint lists as list<oneof<int, string>>.
- Compute the correct common record type for lists of records.
- Propagate expected record field types into field values.
- Add table inference after list and record joins are correct.

The main current implementation is at typechecker.rs:435 (src/typechecker.rs:435).

### P0 — Fix record subtyping

Relevant tests:

- typecheck_records.nu
- typecheck_nested_types.nu

Both record-subtyping loops return true when either field list ends. They must verify that every required supertype field was consumed.

Conceptually, the result needs:

j == supe_fields.len()

after the loop.

This affects both:

- constrain_subtype (src/typechecker.rs:1320)
- is_subtype (src/typechecker.rs:1422)

This is why {age: 4} is incorrectly accepted where {name: string, age: int} is required.

### P0 — Check declared function output types

Relevant tests:

- typecheck_functions.nu
- typecheck_pipelines.nu

Currently command bodies are typechecked with top, and the declared output is stored without checking the body against it. This is already marked at typechecker.rs:880 (src/typechecker.rs:880).

Reject cases such as:

def wrong [] : nothing -> int {
  "not an int"
}

This should be addressed before pipeline overload work because pipelines rely on trustworthy command signatures.

### P0 — Consume the resolver’s argument binding

Relevant test:

- typecheck_commands.nu

The resolver already distinguishes:

- required positional arguments,
- optional positional arguments,
- flags,
- switches,
- rest arguments,
- omitted/defaulted arguments.

The typechecker ignores that information and compares raw call parts positionally at typechecker.rs:936 (src/typechecker.rs:936). That causes valid calls such as configure --count 3 --verbose to produce errors.

Tasks:

- Expose a safe read-only ResolvedArg interface.
- Typecheck each bound value against its selected parameter.
- Do not require omitted optional/default parameters.
- Check each rest argument against the rest element type.
- Give switches a boolean type.
- Add builtin-command handling instead of the current unimplemented!.

### P1 — Implement pipeline input/output selection

Relevant test:

- typecheck_pipelines.nu

Current pipelines typecheck every element independently with top, and calls union every declared output type. This makes valid overloads fail and invalid pipeline inputs pass.

Required flow:

previous output
    ↓
find compatible command input signatures
    ↓
select matching input/output pairs
    ↓
infer union of only matching outputs

For example:

def overloaded [] : [int -> string, string -> int]

should infer:

1 | overloaded       → string
"text" | overloaded  → int
true | overloaded    → error

The relevant current shortcuts are at typechecker.rs:482 (src/typechecker.rs:482) and typechecker.rs:973 (src/typechecker.rs:973).

### P1 — Enforce mutation and compound assignment

Relevant test:

- typecheck_mutation.nu

Assignments currently traverse both operands but perform no checking at typechecker.rs:811 (src/typechecker.rs:811).

Tasks:

- Require the left side to resolve to a mutable variable.
- Check ordinary assignment against the variable’s declared/inferred type.
- Reuse normal operator rules for +=, *=, and similar operators.
- Check the result remains assignable to the variable.
- For ++=, preserve the list element type.

### P1 — Add a dedicated match-pattern checker

Relevant test:

- typecheck_control_flow.nu

Match patterns should not go through typecheck_node, which only understands declarations and structural nodes. Currently literal patterns generate “unsupported AST node” errors.

Introduce something like:

typecheck_pattern(pattern, target_type)

It should handle literals, wildcard patterns, bindings, and structural patterns separately from expressions.

### P1 — Harden generic constraint solving

Generic inference works for the basic tests, but the solver still has fragile areas:

- Constraining a value against oneof<...> does not update inference variables.
- Failed compound constraints can leave earlier mutations applied.
- Variable elimination and solving mutate the shared type arena.
- Diagnostics sometimes expose unresolved forms such as:

  oneof<int, string> <: '6 <: top
- is_subtype and constrain_subtype duplicate substantial logic.

Add solver-level tests for lower bounds, upper bounds, unions, nested lists, conflicting constraints, and rollback after failure.

### P2 — Closures and remaining language coverage

- Represent closure parameter and output types instead of a single Type::Closure.
- Infer untyped closure parameters where possible.
- Handle parameter defaults.
- Handle optional record fields.
- Validate break and continue placement.
- Add the remaining Nushell scalar and collection types.
- Improve error locations and eliminate wrapper-level duplicate diagnostics.

### P2 — Refactor after correctness coverage

At roughly 2,000 lines, typechecker.rs (src/typechecker.rs) mixes:

- AST traversal,
- type representation,
- constraint solving,
- subtype relations,
- joins/intersections,
- formatting,
- diagnostics.

Once the P0 cases have focused tests, split it into modules such as:

typechecker/
  mod.rs
  check.rs
  subtype.rs
  constraints.rs
  normalize.rs
  diagnostics.rs

I would implement in this order:

test oracle
→ lists/records
→ function output contracts
→ resolved command arguments
→ pipeline signature selection
→ mutation
→ match patterns
→ solver hardening
→ closures/tables/refactoring

That order fixes foundational correctness first and avoids building pipeline and closure behavior on top of unreliable collection, call, and signature checking.

