# Contributing to CreaBIM

Thank you for wanting to help! CreaBIM is a free, open-source BIM modeller, designed and built by an architect, in the open.

CreaBIM grows from a design direction: **modelled like Blender, made for architects**. It offers direct modelling for design work, not only documentation. It's one model that every drawing comes from, and it runs offline at no cost. Contributions are very welcome. They come in two kinds.

## 1. Welcome any time: make it work, everywhere

This is help that makes CreaBIM sturdier without changing its design. Open a pull request whenever you like, after an issue for anything bigger than a typo.

- **Other systems:** macOS, Linux distributions, 4K and high-DPI screens, packaging.
- **Bugs:** report them with the *Something went wrong* form. In CreaBIM, *Help › Report a problem…* saves one `.zip` to attach. Fix them with a test that shows the bug.
- **Speed, tests and file exchange:** IFC, DXF, glTF and the rest.
- **The guide:** every page is a Markdown file in [`crates/creabim-app/help`](crates/creabim-app/help). Open one on GitHub, click the pencil, change it and *Propose changes*. No tools needed. Keep the guide's voice: short, plain sentences that say what to do.
- **Translations, sample projects and objects:** start with an issue or a post in [Discussions](https://github.com/chopinregis/CreaBIM/discussions).

## 2. Proposal first: change how it works

New tools, workflows and changes to the interface go through the design, so every piece fits the whole. Before you write the change into CreaBIM:

1. **Explore in your fork.** Clone it and try the idea. Anything goes there.
2. **Show it.** Open an issue with the *An idea or a wish* form and show the idea working: a mockup, screenshots or a short video of your prototype.
3. **Say why.** The task from real work that it solves, and your background (architect, BIM user, developer, student). Every background is welcome; it helps the conversation.
4. **Shape it together.** We'll see how it fits the direction and what it should become.
5. **Bring it in:** a small, checked pull request, once the design is agreed.

Like Blender's add-ons, not every idea joins the app, even good ones. That keeps CreaBIM coherent. Your fork stays free to go wherever you like.

## Using AI

AI is welcome here, from any good model; CreaBIM is built with it too. What matters is the result. Building a tool like this takes design judgement as well as code, so:

- **Experiment freely in your fork.** That's where fast exploration belongs.
- **Audit before you send.** Ask a strong model to review your change against this guide and CreaBIM's direction (the *About CreaBIM* page in the guide), and fix what it finds. It's also a good way to learn the codebase.
- **Understand what you send.** Read the change, run the app and run the checks below. A pull request you haven't tried yourself will be closed.
- **Say it's AI-assisted** in the pull request. It's not a mark against it; it helps the review.
- **Keep it small.** Large AI-written changes are hard to review and won't be merged.
- **Use your own accounts.** AI subscriptions are personal.

Every submission is reviewed, by people and by AI, against where CreaBIM is going.

## Building and checking

CreaBIM is written in Rust. You need [Rust stable](https://rustup.rs).

```bash
cargo run --release -p creabim-app
```

Before a pull request, run the same checks as CI:

```bash
cargo fmt --all --check
```

```bash
cargo clippy --workspace --lib --bins --tests -- -D warnings
```

```bash
cargo test --workspace --lib --bins --tests
```

In the pull request, say what you checked: the tests you ran and what you tried in the app. Add a screenshot for anything visible.

**No new paid services.** CreaBIM runs offline and costs nothing to use, so a dependency must be free, open source and work without a network.

## Funding

You can sponsor the work on [GitHub Sponsors](https://github.com/sponsors/chopinregis), monthly or once. Sponsoring never unlocks features.

## Licence

CreaBIM is free software under the [GNU GPL, version 3 or later](LICENSE). By contributing, you agree that your contribution is licensed under the same terms.

## Be kind

Be patient and respectful: people come from many countries, languages and levels of experience. Critique the work, never the person, and help newcomers get better.
