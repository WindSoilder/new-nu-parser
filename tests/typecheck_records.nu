def accept-person [person: record<name: string, age: int>] {
  $person
}

# Exact record type.
let exact = accept-person {name: "Nu", age: 4}

# Width subtyping: extra fields are allowed.
let with_extra = accept-person {name: "Nu", age: 4, height: "5-9"}

# EXPECT-ERROR: the required age field is missing.
let missing_age = accept-person {name: "Nu"}

# EXPECT-ERROR: the required name field is missing. This exercises end-of-field-list handling.
let missing_name = accept-person {age: 4}

# EXPECT-ERROR: age has the wrong type.
let wrong_age = accept-person {name: "Nu", age: "four"}
