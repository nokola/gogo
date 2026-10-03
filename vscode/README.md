# Gogo for VS Code

Syntax highlighting for `.gogo` files: a TextMate grammar plus comment
toggling, bracket matching and auto-closing pairs. No language server,
nothing runs — the grammar is a set of patterns the editor colors with.

## Install (local, no store)

Point the editor at this folder with a directory junction, then reload
the window (`Developer: Reload Window`). Edits to the grammar show up
after the next reload.

```powershell
New-Item -ItemType Junction -Path "$env:USERPROFILE\.vscode\extensions\gogo-language" -Target "<repo>\FanRust\crates\gogo\vscode"
```

Cursor reads `$env:USERPROFILE\.cursor\extensions` the same way.

To hand it to someone without the repo, pack it:

```powershell
npx @vscode/vsce package   # writes gogo-language-0.1.0.vsix
code --install-extension gogo-language-0.1.0.vsix
```

## What it colors

The grammar follows the capitalization law: a **capitalized** head word
is an addressable node and gets a type color (gogo's own kinds and
widget types differ by scope, `support.class` versus
`entity.name.type`); a **lowercase** head word is structure and gets a
keyword color (`when`/`transition`/`anim`/`repeat` as control flow, the
declarations as storage). After the head: the node name, `.Look` style
references, `key=value` properties with their values (strings with the
`/key` translation suffix colored on its own, `#RRGGBBAA` colors, `$theme`
values, numbers with units, `220x110` sizes, `/0.5` opacity and `@0.24`
stop suffixes, lists, `fill(2)`-style calls), `on:event` props, built-in
state words, `->` transition arrows and `$min..$max` ranges.

`Developer: Inspect Editor Tokens and Scopes` shows what a token got.

## Keeping it right

`tests/vscode_grammar.rs` in the gogo crate fails when the grammar's
built-in kind list differs from `page.rs` `KIND_WORDS`, with the list to
paste. Keywords beyond the kinds (`when`, `uses`, `atlas`, …) are a hand
list in the grammar's `structure-keyword` and `control-keyword` rules.
