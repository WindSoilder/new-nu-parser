def identity<T> [value: T] : nothing -> T {
  $value
}

def singleton<T> [value: T] : nothing -> list<T> {
  [$value]
}

def same-type<T> [left: T, right: T] : nothing -> T {
  $left
}

# Each call gets fresh inference variables.
let int_result = identity 42
let string_result = identity "hello"
let int_list = singleton 42
let string_list = singleton "hello"

# Both arguments constrain the same type variable.
let two_ints = same-type 1 2

# A union is a valid solution when the context permits it.
let union_result = same-type 1 "two"

# EXPECT-ERROR: the arguments require int|string, but the context requires int.
let incompatible: int = same-type 1 "two"
