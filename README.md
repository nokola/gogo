# gogo

A declarative UI language for apps and games, written for humans and AI
assistants alike. A `.gogo` file describes a page: its controls, how they
are laid out, how they look in each theme, and how they move. The compiler
turns it into static data at build time; the runtime takes touches and
returns a draw list. No engine, no window, no GPU: the host program
supplies those.

Design goals, measured on every decision:

- **Easy to understand, easy to write, easy to find.** A `.gogo` file read
  six months later still makes sense.
- **Rust-inspired.** Every construct is announced by a keyword, one keyword
  per concept, hidden semantics made visible, shorthand allowed only as
  defined sugar.
- **Reliable for AI.** Every mistake is a numbered diagnostic with a
  teaching entry; every example builds.

## Layout

| Path | What |
|---|---|
| `crates/gogo-ui` | the language: lexer, parser, compiler, layout, input, draw list, hot reload. Pure Rust, no dependencies. |
| `vscode/` | the VS Code extension: grammar, comment toggling, brackets |
| `docs/gogo-spec.md` | grammar and design rules |
| `docs/REFERENCE.md` | everything, organized for lookup |
| `docs/COOKBOOK.md` | task-oriented recipes |
| `docs/gogo-errors.md` | one teaching entry per diagnostic code |
| `docs/gogo-design.md` | how the crate works inside |
| `docs/cards/` | one card per control and keyword |

## Status

Freshly extracted from the FanRust application, where it drives every page
of a shipping Android app. The docs still describe the earlier Godot addon
in places and are being rewritten for the Rust crate. Text measurement, a
wgpu renderer and a build helper are the next crates to land here.

## License

MIT. See [LICENSE](LICENSE).
