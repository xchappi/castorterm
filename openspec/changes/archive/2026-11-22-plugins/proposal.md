## Why

A team wants castorterm to use its own task tracker and run project actions against its own systems
(a wiki instead of local Markdown) without each person editing their config, and without the open
source defaults knowing anything about that team.

## What Changes

- Every `*.toml` in a `plugins` folder next to `config.toml` is read at start (sorted, at most 20,
  each at most 256 KiB). A plugin may only define `[[providers]]` and `[[actions]]`.
- A provider in the user's config wins over a plugin provider of the same name; plugin providers can
  be used by boards like any other.
- Actions from the config and plugins together replace castorterm's built-in (local) actions.
- Plugin errors are reported like config errors and skip only that plugin.

## Capabilities

### New Capabilities
- `plugins`: configuration fragments that add task providers and project actions.

### Modified Capabilities

## Impact

`src/config.rs` (`read_plugins`, `parse_with_plugins`); README. castorlabs-stack ships the CastorLabs
plugin and installs it with its adapter.
