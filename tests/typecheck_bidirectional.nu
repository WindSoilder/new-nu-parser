# Literals synthesize types.
let inferred_int = 42
let inferred_string = "hello"

# An annotation flows down as the expected type.
let widened_number: number = 42
let typed_items: list<int> = [1, 2, 3]
let empty_items: list<int> = []

# EXPECT-ERROR: int is not a subtype of string.
let bad_text: string = 42

# EXPECT-ERROR: every item must satisfy list<int>.
let bad_items: list<int> = [1, "two"]
