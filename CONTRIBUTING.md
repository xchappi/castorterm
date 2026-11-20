# Contributing to castorterm

Thanks for your interest! The project is in a very early stage, so the
best way to help right now is opening issues with ideas, use cases and bugs.

## Workflow

1. Open an issue (or comment on an existing one) before large changes.
2. Behaviour changes go through an [OpenSpec](https://github.com/Fission-AI/OpenSpec)
   change in `openspec/changes/<id>/` (proposal, specs, tasks).
3. Keep PRs focused: one cohesive change per PR.
4. Before pushing:

   ```sh
   cargo fmt --all
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/):
`feat(sidebar): ...`, `fix(pty): ...`, `docs: ...`.

## License

By contributing you agree that your contributions are dual-licensed under
MIT OR Apache-2.0, without any additional terms or conditions.
