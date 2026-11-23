# Homogeneous and numeric-widened lists.
let ints = [1, 2, 3]
let numbers = [1, 2.5]

# The common element type should be record<a: int>.
let records = [{a: 1}, {a: 2, b: "extra"}]

# The element type should eventually be oneof<int, string>, not any.
let mixed = [1, "two"]

# Roadmap: infer table<a: int, b: string>.
let table = [[a, b]; [1, "one"], [2, "two"]]
