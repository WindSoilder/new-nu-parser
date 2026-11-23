def add [x: int, y: int] : nothing -> int {
  $x + $y
}

def configure [--count: int = 1, --verbose] {
  [$count, $verbose]
}

def optional-value [value?: string] {
  $value
}

def collect-names [...names: string] {
  $names
}

let sum = add 1 2
let defaults = configure
let configured = configure --count 3 --verbose
let absent = optional-value
let present = optional-value "hello"
let names = collect-names "alice" "bob"

# EXPECT-ERROR: x must be int.
let bad_add = add "one" 2

# EXPECT-ERROR: --count must be int.
let bad_flag = configure --count "many"

# EXPECT-ERROR: every rest argument must be string.
let bad_rest = collect-names "alice" 2
