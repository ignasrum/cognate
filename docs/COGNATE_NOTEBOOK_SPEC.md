# Cognate Notebook Specification

## Notebook identity and ownership

A notebook is a directory selected by `COGNATE_NOTEBOOK_PATH` for the API or
`notebook_path` for the desktop configuration. In API mode, the API host's path is
authoritative; the desktop's path is only client-side configuration state. In embedded
local mode, the in-process API and UI share the local notebook directory.

Only the API/engine layer reads or mutates notebook files. This keeps local and remote
clients on the same locking, validation, revision, attachment, and search behavior.

## Filesystem format

The notebook root contains:

- `metadata.json`: canonical note metadata.
- `metadata.json.bak`: last-known-good metadata recovery copy.
- `.cognate_index.bin`: derived search/task/metrics state, when persisted.
- One directory per note, with `note.md` and optional `images/` attachments.

Note paths are relative to the notebook root. A note directory must contain a regular
`note.md`; metadata paths cannot point through symlinks or outside the notebook.

## Metadata schema

```json
{
  "version": 1,
  "notes": [
    {
      "rel_path": "projects/example",
      "labels": ["work", "draft"],
      "last_updated": "2026-07-31T08:27:38Z"
    }
  ]
}
```

`rel_path` is unique and normalized. Labels are user-defined strings. `last_updated`
is optional for compatibility with older notebooks and is repaired from note-file
timestamps when necessary.

Metadata writes are non-structural: the submitted path set must exactly match the
existing valid notebook path set. An unconditional wildcard is allowed only while the
notebook is empty. Use dedicated create, move, and delete operations to change the
path set.

## Revisions and concurrency

Note content revisions are BLAKE3 hashes of canonical note content. Metadata and
attachment revisions are also exposed as BLAKE3-derived ETags. Clients must send the
revision they read in `If-Match` for conditional updates. The server validates the
precondition while holding the relevant locks; stale updates return a conflict and do
not overwrite newer data.

The lock order is always:

```text
notebook lock -> note lock -> filesystem/index mutation
```

This order applies to note writes, metadata changes, create/delete/move operations,
and attachment replacement/deletion. Lock waits are bounded and return a typed error.

## Transaction and recovery rules

- Metadata is written atomically and the previous valid copy is retained as a backup.
- A missing or corrupt primary is recovered from a valid backup; if both are invalid,
  loading fails rather than silently returning an empty notebook.
- Create, move, and delete operations stage filesystem changes and roll back on
  metadata failure. Rollback failure is surfaced as a recovery error.
- Note writes do not create metadata-orphaned notes through ordinary update routes.
- Search state is derived and may be rebuilt from note files and metadata.
- Atomic replacement and directory synchronization reduce the risk of partial writes;
  locks provide the serialization required for multiple processes.

## Attachments

Attachments live at `<note>/images/<generated-name>.<extension>`. Supported formats
are PNG, JPEG, GIF, and WebP, detected from magic bytes rather than a client-provided
extension. Their content revision is the BLAKE3 hash of the bytes. API upload and
replacement requests are bounded to 48 MiB, and conditional replacement/deletion
prevents stale clients from destroying newer files.

## Search and derived state

The engine indexes note paths, labels, content, and update timestamps. It supports
plain terms, phrases, negation, label/path filters, and updated-date ranges. Search
results contain scores, match types, snippets, and highlight ranges. The API refreshes
the index after successful mutations and periodically checks the filesystem for
external changes. Index corruption or staleness is repairable and must never replace
the canonical notebook as the source of truth.

## Client and offline behavior

The desktop API client caches the latest revisions and queues retryable note writes in
the permissions-restricted `.cognate-api-queue.json` beside the client configuration.
The queue is atomically replaced, coalesces newer writes for the same note, records
remote commit acknowledgement before removing entries, and preserves conflicts for
visual resolution. Closing the application does not discard a persisted queue.

Embedded local mode keeps its API client secret in memory only and does not create the
standalone API's SQLite database. Remote deployments store only hashed client secrets
and client metadata in the API database.
