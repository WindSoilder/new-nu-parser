# Arithmetic and numeric promotion.
let int_sum = 1 + 2
let float_sum = 1 + 2.5
let quotient = 5 / 2
let remainder = 5 mod 2

# String, comparison, equality, boolean, membership, regex, and append operators.
let string_sum = "nu" + "shell"
let comparison = 1 < 2.5
let equality = 1 == 1.0
let boolean_result = true and false
let membership = 2 in [1, 2, 3]
let regex_result = "nushell" =~ "nu.*"
let appended = [1, 2] ++ [3.5]

# EXPECT-ERROR: unsupported operand combinations.
let bad_add = "value" + 1
let bad_boolean = 1 and true
let bad_membership = "two" in [1, 2, 3]
let bad_comparison = 1 < "two"
let bad_append = [1, 2] ++ "three"
