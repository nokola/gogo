# gogo

A declarative UI language for apps and games, written for people and for AI
assistants. A `.gogo` file describes a page: its controls, how they are laid
out, how they look in each theme, and how they move.

```kdl
View hello {
    Column menu halign=center, valign=center, spacing=12 {
        Label title text="Hello from gogo", font_size=24
        Button startButton text="Start", min_width=160, min_height=44
    }
}
```

gogo does the middle of a UI. It compiles a page (at build time, into
static data), lays it out for a screen size, takes touches and keys, and
answers with a list of draw commands. It owns no window and no GPU: the
program that hosts it measures the text and paints the commands.

## Try it

```text
cargo run -p gogo-ui --example first_page
```

This compiles the page above, lays it out for a 360 x 640 screen, and prints
where each control landed and the three draw commands that paint it. The
source is [crates/gogo-ui/examples/first_page.rs](crates/gogo-ui/examples/first_page.rs).

## Design goals

Every decision about the language is measured against these:

- **Easy to understand, easy to write, easy to find.** A `.gogo` file read
  six months later still makes sense.
- **Rust-inspired.** Every construct is announced by a keyword, one keyword
  per concept, hidden semantics made visible, shorthand allowed only as
  defined sugar.
- **Reliable for AI.** Every mistake is a numbered diagnostic with a
  teaching entry, and every example in the docs is compiled by the tests.

## What is here

| Path | What |
|---|---|
| `crates/gogo-ui` | the language: lexer, parser, compiler, layout, input, draw list, hot reload. Pure Rust, no dependencies. |
| `docs/gogo-spec.md` | grammar and design rules |
| `docs/REFERENCE.md` | everything, organized for lookup |
| `docs/COOKBOOK.md` | recipes: "How do I ...?" |
| `docs/cards/` | one short card per control and keyword |
| `docs/gogo-errors.md` | one teaching entry per diagnostic code |
| `docs/gogo-design.md` | how the crate works inside |
| `docs/examples/` | the sample project the docs speak about: a shared theme and a guided tour in one file |
| `docs/design/` | notes from when the language was designed; history, not reference |
| `vscode/` | the VS Code extension: syntax colors, comment toggling, brackets |

## The examples in the docs are compiled

`cargo test` runs every gogo example in this README, the spec, the
reference, the cookbook and the cards through the compiler
(`crates/gogo-ui/tests/docs_examples.rs`). A snippet that is not a whole
page is put inside a `View` first. A word after the code fence's language
marks an example that is held to a different account:

- `partial`: the example leans on something outside itself, such as a node
  from an earlier snippet. Only its grammar is checked.
- `stale`: the example was written for the earlier Godot implementation and
  is not rewritten yet. Do not copy it.
- `error=G1234`: the example shows a mistake on purpose, and the compiler
  must refuse it with that code.

## Status

gogo was taken out of the FanRust application, where it draws every page of
a shipping Android app. Parts of the docs still describe the earlier Godot
implementation: the examples marked `stale`, and the sections about a Godot
host (GDScript, engine nodes, the runtime API). They are being rewritten for
the Rust crate. A crate that measures text, a wgpu renderer and an example
app with a window are the next things to land here.

## License

MIT. See [LICENSE](LICENSE).
