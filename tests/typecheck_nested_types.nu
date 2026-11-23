let matrix: list<list<int>> = [[1, 2], [3, 4]]

let config: record<name: string, options: record<retries: int>, tags: list<string>> = {
  name: "parser",
  options: {retries: 3},
  tags: ["rust", "nushell"],
}

# EXPECT-ERROR: an inner list contains string instead of int.
let bad_matrix: list<list<int>> = [[1, 2], ["three"]]

# EXPECT-ERROR: nested retries has the wrong type.
let bad_config: record<name: string, options: record<retries: int>, tags: list<string>> = {
  name: "parser",
  options: {retries: "three"},
  tags: ["rust"],
}

# EXPECT-ERROR: a required nested field is missing.
let incomplete_config: record<name: string, options: record<retries: int>> = {
  name: "parser",
  options: {},
}
