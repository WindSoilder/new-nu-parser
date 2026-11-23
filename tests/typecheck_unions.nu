# Explicit union annotations.
let int_or_string: oneof<int, string> = 42
let another_int_or_string: oneof<int, string> = "forty-two"
let int_or_nothing: oneof<int, nothing> = null

# A single-member union is the member itself.
let just_int: oneof<int> = 42

# Numeric subtyping still applies inside a union.
let number_or_string: oneof<number, string> = 2.5

# `any` accepts any statically known value.
let dynamic_scalar: any = true
let dynamic_record: any = {name: "Nu", version: 1}

# Inferred branch union.
let inferred_union = if true { 1 } else { "one" }

# EXPECT-ERROR: bool is neither int nor string.
let invalid_union: oneof<int, string> = false
