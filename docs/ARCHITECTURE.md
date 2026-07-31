# Architecture Guide

This document explains how Cognate is structured and where to implement changes.

## High-level Shape

Cognate is a desktop app with a message-driven UI and an API-backed notebook domain:

- UI layer: Iced components (`editor`, `note_explorer`, `visualizer`)
- Client layer: HTTP notebook operations, revisions, offline queue, and embedded API lifecycle
- Service layer: `cognate-api` authentication, routing, and notebook API
- Engine layer: `cognate-engine` filesystem persistence, locks, attachments, and search

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

## UI Surfaces

Editor workspace:

![Editor workspace](editor.png)

Visualizer:

![Visualizer](visualizer.png)

### `src/notebook`

- `backend.rs`: backend selection, lifecycle, revisions, and public notebook operation facade
- `api_client.rs`: authenticated HTTP requests, URL construction, response decoding, and retry classification
- `attachment_ops.rs`: attachment upload, download, and conditional deletion operations
- `embedded_api.rs`: embedded loopback API lifecycle for local mode
- `operations.rs` and `storage.rs`: API-facing notebook operation adapters
- `search.rs`: search request metadata/result types used by the API client

### `cognate-engine/src/storage`

- `notebook.rs`: shared async notebook persistence and transactional note mutations
- `metadata.rs`: metadata models, timestamp normalization, and metadata validation helpers
- `metadata_persistence.rs`: atomic metadata writes, recovery snapshots, and index synchronization
- `index_sync.rs`: persisted engine-index loading, saving, and metadata synchronization
- `notebook_content.rs`: note content reads and filesystem modification-time lookup
- `notebook_lifecycle.rs`: create, delete, move, and rollback-safe note mutations
- `transactions.rs`: transaction staging-path generation used by rollback-safe mutations
- `concurrency.rs`: cross-process advisory notebook and note locks
- `attachments.rs`: embedded attachment persistence with lock coordination
- `fs_utils.rs`: path validation and atomic filesystem writes

All desktop, API, and TUI writers should use these engine APIs. Notebook-wide
transactions protect metadata and the shared persisted index; note-content writes hold
the notebook lock before the note lock so the index read-modify-write cannot overwrite
another client's update. Lock acquisition waits up to five seconds before returning a
typed lock-unavailable error.

The lock order is always notebook lock first, then note lock. Attachment uploads,
replacements, and conditional deletes follow this order too, preventing them from
racing note moves or deletes. Filesystem writes resolve the nearest existing parent
through canonical paths and reject symlink escapes; atomic writes flush the temporary
file before replacement and synchronize the parent directory on Unix.

### `cognate-api`

`cognate-api` is the HTTP boundary for external clients. It binds to a configurable address
and port (loopback by default), authenticates Bearer client keys, stores only BLAKE3 digests
and client metadata in SQLite, and delegates notebook mutations to `cognate-engine`.
Public HTTPS termination is provided by an external reverse proxy; the API process does
not manage certificates.

Route composition lives in `src/api.rs`. Attachment and search handlers are separated into
`src/routes/admin.rs`, `src/routes/attachments.rs`, and `src/routes/search.rs`; route handlers still delegate all
filesystem access, locking, revisions, and indexing to `AppState` and `cognate-engine`.

In local mode, Cognate embeds this service on an ephemeral loopback port with an
in-memory authentication store. No SQLite database or persistent client secret is
created. In remote mode, the standalone service uses SQLite for client metadata and
hashed secrets.

## Data Model

Primary persisted metadata shape (`NoteMetadata`):

- `rel_path`: note directory path relative to notebook root
- `labels`: user-defined tags
- `last_updated`: RFC3339 timestamp (optional for backward compatibility)

Notebook metadata is stored in `metadata.json` under notebook root. For details, see [STORAGE.md](STORAGE.md).

## Message and State Flow

1. UI emits a `Message` from user interaction.
2. Editor dispatches the message to a domain-specific handler.
3. Handler updates in-memory state and may schedule async tasks.
4. Task completion emits follow-up messages.
5. View rendering reflects current state.

This keeps UI behavior deterministic and testable through message transitions.

## Persistence and Consistency

- Note content and metadata writes are explicit operations.
- Cross-process advisory locks serialize mutations across cooperating Cognate clients;
  atomic replacement prevents partial files but does not replace locking.
- Metadata writes can be debounced in edit flows, but the API validates metadata
  revisions while holding the notebook lock, so stale snapshots cannot overwrite
  newer metadata.
- The API-mode note queue is a locked, atomically replaced, permissions-restricted
  file. It coalesces writes per note, preserves conflicts for explicit resolution,
  and recovers a valid orphaned temporary file only when the primary queue is absent.
- Shutdown attempts a bounded final flush before window close. If it fails, the
  window remains open and reports the failure rather than silently discarding changes.
- A transient API failure is considered safely recoverable during shutdown only after
  the note queue write succeeds. Queue persistence failures keep the window open.
- Search indexes are owned by the API/engine process and refreshed from the filesystem
  on interval with targeted mutation hooks. They are derived state: a stale or failed
  index update must not be treated as a failed canonical note or metadata commit.
- Attachment replacement uses atomic byte replacement, and deletion validates its
  `If-Match` revision under the same note lock as the delete.

## Where to Add Features

- New editor commands: `components/editor` message + handler + layout control
- New notebook mutations: `notebook/operations.rs` and related tests
- New visualization behavior: `components/visualizer` graph/canvas modules
- New config fields: `configuration/reader.rs` and config tests
