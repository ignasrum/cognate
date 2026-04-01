# Architecture Guide

This document explains how Cognate is structured and where to implement changes.

## High-level Shape

Cognate is a desktop app with a message-driven UI and a file-backed notebook domain:

- UI layer: Iced components (`editor`, `note_explorer`, `visualizer`)
- Domain layer: notebook operations (`create`, `delete`, `move`, `search`)
- Infrastructure layer: JSON/config parsing and disk persistence

## Core Modules

### `src/components/editor`

- Main application state holder and update dispatcher
- Coordinates text editing, note lifecycle flows, labels, search, and shutdown flushes
- Renders the main workspace through `ui/layout.rs`

### `src/components/note_explorer`

- Loads note metadata from notebook storage
- Maintains expanded/collapsed folder state
- Renders a tree view and emits selection/rename-intent messages

### `src/components/visualizer`

- Builds a graph from notes and labels
- Handles camera focus and canvas interactions
- Emits note selection and focus events back to the editor

### `src/notebook`

- `operations.rs`: create/delete/move with path safety and metadata updates
- `storage.rs`: metadata and note file persistence
- `search.rs`: search index cache and query matching

## Data Model

Primary persisted metadata shape (`NoteMetadata`):

- `rel_path`: note directory path relative to notebook root
- `labels`: user-defined tags
- `last_updated`: RFC3339 timestamp (optional for backward compatibility)

Notebook metadata is stored in `metadata.json` under notebook root.

## Message and State Flow

1. UI emits a `Message` from user interaction.
2. Editor dispatches the message to a domain-specific handler.
3. Handler updates in-memory state and may schedule async tasks.
4. Task completion emits follow-up messages.
5. View rendering reflects current state.

This keeps UI behavior deterministic and testable through message transitions.

## Persistence and Consistency

- Note content and metadata writes are explicit operations.
- Metadata writes can be debounced in edit flows.
- Shutdown path attempts a final flush before window close.
- Search cache is refreshed from filesystem on interval and mutation hooks.

## Where to Add Features

- New editor commands: `components/editor` message + handler + layout control
- New notebook mutations: `notebook/operations.rs` and related tests
- New visualization behavior: `components/visualizer` graph/canvas modules
- New config fields: `configuration/reader.rs` and config tests
