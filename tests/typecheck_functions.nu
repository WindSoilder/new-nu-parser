def square [value: number] : nothing -> number {
  $value * $value
}

def singleton [value: int] : nothing -> list<int> {
  [$value]
}

let squared_int: number = square 4
let squared_float: number = square 2.5
let one_item: list<int> = singleton 1

# EXPECT-ERROR: square requires number.
let bad_argument = square "four"

# EXPECT-ERROR: singleton requires int.
let bad_singleton = singleton 1.5

# EXPECT-ERROR: the body does not satisfy the declared output.
def wrong_scalar_output [] : nothing -> int {
  "not an int"
}

# EXPECT-ERROR: the body does not satisfy the nested declared output.
def wrong_list_output [] : nothing -> list<int> {
  ["not an int"]
}
