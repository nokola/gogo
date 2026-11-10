# Gogo for VS Code

Syntax highlighting for `.gogo` files: a TextMate grammar plus comment
toggling, bracket matching and auto-closing pairs.

Gogo UI is high performace (~1000fps on phone from 2014.) This is achieved by pre-compiling .gogo files into Rust arrays, that results in low memory allocation and instant display for complex UIs.

Official gogo UI page: https://github.com/nokola/gogo

## What it colors

* **capitalized** type color 
* keywords (`when`/`transition`/`anim`/`repeat` 
* styles e.g. `.Look`
* references, `key=value` 
* translation suffix `/key` 
* `#RRGGBBAA` colors, `$theme` values
* numbers with units, `220x110` sizes, `/0.5` opacity and `@0.24`
* stop suffixes, lists, `fill(2)`-style calls
* `on:event` props
* built-in state words
* `->` transition arrows and `$min..$max` ranges.
