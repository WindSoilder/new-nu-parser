const root = "modules"
export const answer = 42
extern git status [--porcelain (-p): string = "v1", ...rest: string]
module tools {
    export def greet [name: string] { $"hi ($name)" }
}
use tools *
hide tools greet
source-env "env.nu"
plugin use "plugin.nu"
overlay new demo
let mask = 1 bit-or 2 bit-xor 3 bit-and 4 bit-shl 1 bit-shr 1
^echo ...[1 2] o> "out.txt"
