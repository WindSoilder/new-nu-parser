# Literal inference and propagation through references.
let integer = 42
let copied_integer = $integer
let widened_number: number = $copied_integer

# A branch joins int and float to number.
let selected_number = if true { $integer } else { 2.5 }

# Infer a loop variable from the list element type.
let integer_items = [1, 2, 3]
for item in $integer_items {
  let copied_item = $item
}

# A new binding with the same name must not change earlier bindings.
let before_shadow = $integer
let integer = "shadowed"
let after_shadow = $integer

$copied_integer
$widened_number
$selected_number
$before_shadow
$after_shadow
