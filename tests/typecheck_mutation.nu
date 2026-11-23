mut count: int = 0

# Valid simple and compound assignments.
$count = 1
$count += 2
$count *= 3

# EXPECT-ERROR: assignment must preserve the variable's int type.
$count = "one"

# EXPECT-ERROR: += must apply numeric addition for an int variable.
$count += "two"

mut names: list<string> = ["alice"]
$names ++= ["bob"]

# EXPECT-ERROR: append assignment must preserve list<string>.
$names ++= [1]
