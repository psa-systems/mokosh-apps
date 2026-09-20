# The Markdown editor (MAPPS-592)

`crate::components::MarkdownEditor` (`src/components/markdown_editor.rs`) is
the shared Markdown source field: a `<textarea>` plus its toolbar, keyboard
shortcuts, and `@`-mention completion. It grew out of the Knowledge Base
article editor over MAPPS-579, MAPPS-580 and MAPPS-587, then was pulled out so
every other place the app takes Markdown could have the same help instead of a
bare `<textarea>`. It is used for the KB article body (`knowledge_base.rs`),
the ticket description and notes (`tickets.rs`), contact notes
(`contacts.rs`), and KB activity comments (`kb_activity.rs`).

What it deliberately does not own: a preview pane (the KB page's Write /
Preview / Split modes are layout the page controls) and required file upload
wiring (`on_file` is optional, `None` where there is no saved entity yet to
attach to). The field itself stays a plain `<textarea>` holding Markdown
source; every toolbar action and shortcut rewrites that source and hands it
back, never a rendered DOM.

## Its three guard scripts

Three scripts in `scripts/`, each wired into `just check` and CI, protect
invariants the editor depends on that a reviewer cannot reliably catch by eye:

| Script | Checks |
| --- | --- |
| `check-field-value-binding.sh` | A shared form `textarea` (`src/components/form.rs`) binds its value through the `value:` attribute, not as a text child. A text child is only the DOM's default value: the browser stops syncing it after the first keystroke, which silently breaks every toolbar action that rewrites the source (MAPPS-585). |
| `check-prose-layer.sh` | The Markdown preview's `.prose` corrections in `input.css` live in a cascade layer that sorts after `@tailwindcss/typography`'s `utilities` layer. A cascade layer beats specificity and source order both, so a correction placed in an earlier layer is silently overridden by the plugin it was meant to fix (MAPPS-584). |
| `check-hooks-before-return.sh` | Every hook call in a component or custom hook runs before every early `return`. A hook called after a return only runs on some renders, which desyncs dioxus's hook index and panics the WASM runtime on the next render (MAPPS-602). |

Each script documents its own rationale in its header comment and supports
`--self-test` to check its own detection logic against fixtures.
