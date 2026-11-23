# Typed parameters constrain the closure body.
let increment = {|value: int| $value + 1}
let predicate: closure = {|value: number| $value > 0}

# Multiple parameters can participate in one constraint.
let less_than = {|left: number, right: number| $left < $right}

# Roadmap: infer a useful parameter/output signature for untyped closures.
let untyped_identity = {|value| $value}

# EXPECT-ERROR: string cannot be used as the numeric operand of +.
let invalid_body = {|value: string| $value + 1}
