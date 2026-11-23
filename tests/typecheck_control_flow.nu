# Branch joins.
let numeric_branch = if true { 1 } else { 2.5 }
let union_branch = if true { "yes" } else { 0 }
let maybe_value = if true { 1 }

# Loop conditions must be bool.
while 1 < 2 {
  let inside_while = true
}

# EXPECT-ERROR: string is not bool.
while "not a condition" {
  let unreachable = 0
}

# The item type is inferred from list<int>.
for item in [1, 2, 3] {
  let doubled = $item + $item
}

# EXPECT-ERROR: an integer is not iterable by this checker.
for item in 123 {
  let copied = $item
}

# Match result types form a union.
let matched = match 1 {
  0 => "zero",
  1 => 1,
  _ => null,
}
