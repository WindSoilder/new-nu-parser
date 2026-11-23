def int-to-string [] : int -> string {
  "converted"
}

def overloaded [] : [int -> string, string -> int] {
  if true { 1 } else { "ordinary" }
}

# The selected signature determines the call's output type.
let string_output: string = 1 | int-to-string
let overloaded_string: string = 1 | overloaded
let overloaded_int: int = "text" | overloaded

# EXPECT-ERROR: int-to-string does not accept string input.
let bad_input = "not an int" | int-to-string

# EXPECT-ERROR: overloaded accepts neither bool input.
let no_overload = true | overloaded

# EXPECT-ERROR: the body output does not match the declared int output.
def wrong-body [] : nothing -> int {
  "not an int"
}
