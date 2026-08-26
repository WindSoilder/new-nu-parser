# Nushell Grammar — Reader-Friendly EBNF

This is a descriptive, context-free grammar for the Nushell surface syntax targeted
by this repository. It is intentionally written in the reader-friendly EBNF style
used in *Crafting Interpreters*, rather than in executable PEG syntax.

`grammar.peg` remains the normative PEG-style target grammar. This document is a
companion: it makes precedence, repetition, and the boundary between syntax and
parse-time semantics easier to review.

## Notation

- `a → b` means “`a` is composed of `b`”.
- `a | b` means “choose one of `a` or `b`”.
- `x*`, `x+`, and `x?` mean zero-or-more, one-or-more, and optional `x`.
- Parentheses group alternatives.
- Quoted text is literal source text; uppercase names are lexer tokens.
- This grammar describes syntax only unless a rule is marked **parse-time semantic
  requirement** below.

## Program structure

```text
program        → shebang? terminator* statementSequence? terminator* EOF ;
statementSequence
               → statement (terminator+ statement)* ;
terminator     → NEWLINE | ";" ;

statement      → declaration
               | loopStatement
               | flowStatement
               | assignment
               | pipeline ;

pipeline       → pipeElement (pipe pipeElement)* ;
pipeElement    → expressionCommand redirection* ;
pipe           → "|" | "e>|" | "err>|" | "out+err>|" | "err+out>|" ;
redirection    → fileRedirection expression ;
fileRedirection
               → ">" | "o>" | ">>" | "o>>"
               | "e>" | "err>" | "e>>" | "err>>"
               | "out+err>" | "err+out>" | "o+e>" | "e+o>"
               | "out+err>>" | "err+out>>" | "o+e>>" | "e+o>>" ;
```

## Declarations, modules, and overlays

```text
declaration    → letDecl | mutDecl | constDecl | defDecl | externDecl
               | aliasDecl | moduleDecl | useDecl | sourceDecl
               | exportDecl | exportEnvDecl | hideDecl | overlayDecl
               | pluginUseDecl ;

letDecl        → "let" binding "=" pipeline ;
mutDecl        → "mut" binding "=" pipeline ;
constDecl      → "const" binding "=" expression ;
binding        → variableDecl typeAnnotation? ;
variableDecl   → "$"? IDENTIFIER ;

defDecl        → "def" defOption* commandName typeParams? signature
                 ioSignature? block ;
defOption      → "--env" | "--wrapped" ;
externDecl     → "extern" commandName signature ;
aliasDecl       → "alias" commandName "=" pipeline ;

moduleDecl     → "module" moduleName block | "module" modulePath ;
useDecl        → "use" importPattern ;
sourceDecl     → ("source" | "source-env") expression ;
exportDecl     → "export" exportableDeclaration ;
exportableDeclaration
               → defDecl | externDecl | aliasDecl | constDecl | moduleDecl
               | useDecl ;
exportEnvDecl  → "export-env" block ;
hideDecl       → "hide" importPattern ;

pluginUseDecl  → "plugin" "use" expression ;
overlayDecl    → "overlay" overlayAction ;
overlayAction  → "use" "--prefix"* importPattern ("as" commandName)?
               | "hide" overlayHideOption* commandName? overlayHideOption*
               | "new" commandName
               | "list" ;
overlayHideOption
               → "--keep-custom" | "--keep-env" list ;
```

## Statements and command calls

```text
assignment     → cellRef assignOp pipeline ;
assignOp       → "=" | "+=" | "++=" | "-=" | "*=" | "/=" ;

loopStatement  → forStatement | whileStatement | loopForever ;
forStatement   → "for" variableDecl "in" expression block ;
whileStatement → "while" expression block ;
loopForever    → "loop" block ;

flowStatement  → "return" expression? | "break" | "continue" ;

expressionCommand
               → environmentAssignment* command ;
environmentAssignment
               → ENV_NAME "=" (string | variable | bareWord) ;
command        → externalCall | internalCall | expression ;
externalCall   → "^" externalName externalArgument* ;
externalName   → commandName | variable | string ;
externalArgument
               → spread | expression | bareWord | string ;
internalCall   → commandName argument* ;
argument       → flag | spread | expression | bareWord ;
flag           → longFlag (("=" expression) | expression)?
               | shortFlag (("=" expression) | expression)? ;
longFlag       → "--" FLAG_NAME ;
shortFlag      → "-" SHORT_FLAGS ;
spread         → "..." expression ;

commandName    → string | knownCommandName | IDENTIFIER ;
knownCommandName
               → IDENTIFIER IDENTIFIER+ ;
```

**Command-name semantic requirement:** command heads are scope-sensitive. The
parser/resolver must select the longest name that is a command known in the current
parse-time scope, then parse the remaining words as arguments. The EBNF rule above
only admits the possible word sequence; it does not perform name resolution.

## Expressions and precedence

Each rule accepts its own precedence level and every tighter level below it. This
makes expressions such as `1 + 2 * 3` unambiguous without precedence metadata.
Binary repetition in the following rules is left-associative unless stated otherwise.

```text
expression     → range ;
range          → logicalOr (rangeOp logicalOr? (rangeOp logicalOr?)?)?
               | rangeOp logicalOr? ;
rangeOp        → ".." | "..<" ;

logicalOr      → logicalXor ("or" logicalXor)* ;
logicalXor     → logicalAnd ("xor" logicalAnd)* ;
logicalAnd     → bitOr ("and" bitOr)* ;
bitOr          → bitXor ("bit-or" bitXor)* ;
bitXor         → bitAnd ("bit-xor" bitAnd)* ;
bitAnd         → comparison ("bit-and" comparison)* ;
comparison     → shift (compareOp shift)* ;
compareOp      → "==" | "!=" | "<" | "<=" | ">" | ">="
               | "=~" | "!~" | "in" | "not-in" | "has" | "not-has"
               | "like" | "not-like" | "starts-with" | "not-starts-with"
               | "ends-with" | "not-ends-with" | "++" ;
shift          → addition (("bit-shl" | "bit-shr") addition)* ;
addition       → multiply (("+" | "-") multiply)* ;
multiply       → power (("*" | "/" | "//" | "mod") power)* ;
power          → unary ("**" power)? ;  // right-associative
unary          → ("not" | "+" | "-") unary | postfix ;
postfix        → primary cellPath? ;

primary        → ifExpression | tryExpression | matchExpression
               | literal | variable | cellPathLiteral | table | list | closure
               | record | block | subexpression ;
subexpression  → "(" statementSequence? ")" ;
block          → "{" statementSequence? "}" ;
```

## Values, collections, and paths

```text
literal        → FILESIZE | DURATION | DATE | BINARY | interpolatedString
               | rawString | string | FLOAT | INT | BOOL | "null" ;
string         → singleQuotedString | doubleQuotedString | backtickString ;
variable       → specialVariable | "$" IDENTIFIER ;
specialVariable
               → "$env" | "$in" | "$it" | "$nu"
               | "$NU_LIB_DIRS" | "$NU_PLUGIN_DIRS" ;

cellRef        → variable cellPath? | cellPathLiteral ;
cellPathLiteral
               → "$" cellPath ;
cellPath       → cellMember+ ;
cellMember     → "." pathMember "?"? ;
pathMember     → string | INT | IDENTIFIER ;

list           → "[" listItem (separator? listItem)* separator? "]"
               | "[" "]" ;
listItem       → spread | expression | bareWord ;

table          → "[" tableHeader ";" tableRow* "]" ;
tableHeader    → "[" tableHeaderName (separator? tableHeaderName)* separator? "]"
               | "[" "]" ;
tableHeaderName
               → string | IDENTIFIER ;
tableRow       → "[" tableRowValue (separator? tableRowValue)* separator? "]"
               | "[" "]" ;
tableRowValue  → expression | bareWord ;

record         → "{" recordItem (separator? recordItem)* separator? "}"
               | "{" "}" ;
recordItem     → recordKey ":" expression ;
recordKey      → string | IDENTIFIER ;

closure        → "{" "|" closureParameter ("," closureParameter)* ","? "|"
                 statementSequence? "}"
               | "{" "|" "|" statementSequence? "}"
               | "{" statementSequence? "}" ;
closureParameter
               → IDENTIFIER typeAnnotation? ;
separator      → "," | NEWLINE | ";" ;
```

## Control expressions and patterns

```text
ifExpression   → "if" expression block elseClause? ;
elseClause     → "else" ifExpression | "else" matchExpression | "else" block ;
tryExpression  → "try" block catchClause? finallyClause? ;
catchClause    → "catch" (closure | block) ;
finallyClause  → "finally" block ;

matchExpression
               → "match" expression "{" matchArm (separator matchArm)*
                 separator? "}" ;
matchArm       → pattern guard? "=>" (expression | block) ;
guard          → "if" expression ;
pattern        → singlePattern ("|" singlePattern)* ;
singlePattern  → "_" | literal | variable | listPattern | recordPattern ;
listPattern    → "[" pattern ("," pattern)* ","? "]" ;
recordPattern  → "{" recordPatternItem ("," recordPatternItem)* ","? "}" ;
recordPatternItem
               → recordKey ":" pattern ;
```

## Signatures, types, imports

```text
signature      → "[" signatureParameter (separator? signatureParameter)*
                 separator? "]" ;
signatureParameter
               → restParameter | flagParameter | shortOnlyFlag | positionalParameter ;
positionalParameter
               → IDENTIFIER "?"? typeAnnotation? defaultValue? ;
restParameter  → "..." IDENTIFIER typeAnnotation? ;
flagParameter  → longFlag ("(" shortFlag ")")? typeAnnotation? defaultValue? ;
shortOnlyFlag  → shortFlag typeAnnotation? defaultValue? ;
defaultValue   → "=" expression ;
ioSignature    → ":" "[" inOutType (separator? inOutType)* separator? "]"
               | ":" inOutType ;
inOutType      → type "->" type ;
typeAnnotation → ":" type ("@" commandName)? ;
type           → "record" "<" recordTypeField ("," recordTypeField)* ","? ">" "?"?
               | IDENTIFIER typeArguments? "?"? ;
typeArguments  → "<" type ("," type)* ","? ">" ;
recordTypeField
               → recordKey typeAnnotation? ;
typeParams     → "<" IDENTIFIER ("," IDENTIFIER)* ","? ">" ;

importPattern  → moduleRef importMembers? ;
moduleRef      → modulePath | moduleName ;
modulePath     → PATH | string ;
moduleName     → string | commandName ;
importMembers  → "*" | commandName
               | "[" importMember (separator? importMember)* separator? "]"
               | "[" "]" ;
importMember   → "*" | string | commandName ;
```

## Lexer contracts

The lexer owns token boundaries, comments, escape decoding, and raw-string delimiter
matching. These are deliberately not expressed as ordinary context-free productions.

```text
lineComment    → "#" charactersUntilNewline ;
shebang        → "#!" charactersUntilNewline NEWLINE ;
skip           → (space | tab | lineComment)* ;

singleQuotedString
               → "'" anyCharacterUntil("'") "'" ;
doubleQuotedString
               → '"' (escape | characterExceptQuoteBackslashOrNewline)* '"' ;
backtickString → "`" anyCharacterUntil("`") "`" ;
interpolatedString
               → '$"' interpolationPart* '"'
               | "$'" interpolationPart* "'" ;
```

### Raw-string lexer algorithm

Nushell raw strings begin with one or more `#` characters and use the *same count*
in their closing delimiter:

```text
rawString       → "r" rawDelimiter rawContent rawClosingDelimiter ;
rawDelimiter    → "#"+ "'" ;
```

The equality between the number of opening and closing `#` characters is not a
regular EBNF/PEG relationship. Implement it in the lexer:

1. After reading `r`, count consecutive `#` characters as `hash_count`. Require
   `hash_count >= 1` and then require an opening `'`.
2. Scan raw content without processing escapes or interpolation.
3. At each `'`, check whether it is immediately followed by `hash_count` `#`
   characters.
4. If so, consume that quote and those hashes and emit one `RAW_STRING` token.
   Otherwise, keep scanning; that quote belongs to the content.
5. If EOF is reached first, emit an unterminated-raw-string error whose span starts
   at the opening `r`.

Examples:

```text
r#'text with 'quotes''#            // one-hash delimiter
r###'r##'nested-looking text'##'### // three-hash delimiter
r##'a '# is content; '## closes'##  // only quote + two hashes closes
```

A PEG may retain a placeholder such as `RAW_STRING ← LexerRawString`, but it should
not use independent `'#'+` expressions for the opener and closer because that admits
mismatched delimiters.

## Parse-time semantic requirements (not PEG)

Nushell parses a complete source unit before ordinary evaluation. Some declarations
and commands therefore have syntax *and* parse-time effects. The PEG/EBNF should
accept their structure; a resolver/semantic pass must enforce the following rules in
source order.

### Parse-time declarations and scope

- `const name = expression` requires a constant-evaluable expression. Store the
  resulting value in the parse-time constant scope.
- `def`, `extern`, `alias`, `module`, and their `export` variants register names in
  the parse-time declaration/module scope when their declaration is processed.
- Parse-time scope determines multiword command resolution and which imported or
  overlaid names are available to subsequent source.

### Parse-time loading and mutation

- `source`, `source-env`, `use`, `overlay use`, and `plugin use` require arguments
  that the parser can resolve without ordinary runtime evaluation. Where Nushell
  permits computed inputs, they must be constant-evaluable.
- Resolve the module/script/plugin during parsing, parse its contents or signatures,
  and merge the resulting declarations into the appropriate parse-time scope.
- `hide` and `overlay hide` operate on names/overlays that are known in that scope.
- Context checks are semantic: for example, `export` forms are valid only where the
  module/export context allows them.

### Diagnostics

The resolver should diagnose a non-constant parser-keyword argument at its argument
span, an unresolved path/name at the corresponding operand, and a source-order
failure with a note that Nushell resolves parser keywords before normal evaluation.

```nu
# Valid: `root` is known while this source unit is parsed.
const root = $nu.default-config-dir | path join "modules"
use $"($root)/tools.nu" *

# Invalid: ordinary `let` values do not exist until evaluation.
let root = $nu.default-config-dir
use $"($root)/tools.nu" *
```

This separation is required by Nushell's parse-then-evaluate execution model and its
restricted parse-time constant evaluation.

## Deliberate omissions

This document does not enumerate every built-in command. Built-ins and subcommands
are accepted by `internalCall`/`commandName`; their command-specific signatures and
semantics belong in the command registry, not the grammar.
