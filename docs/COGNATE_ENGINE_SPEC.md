# Cognate Engine Specification

## Purpose

`cognate-engine` is the storage and domain layer shared by the desktop application
and `cognate-api`. It owns the canonical notebook filesystem, derived search state,
attachments, and the invariants that keep concurrent clients from corrupting data.
Callers must use the public engine managers rather than manipulating notebook files
directly.

## Canonical notebook layout

```text
<notebook>/
├── metadata.json
├── metadata.json.bak
├── .cognate_index.bin
└── <note path>/
    ├── note.md
    └── images/
        └── <generated image id>.<png|jpg|gif|webp>
```

`NoteMetadata` contains `rel_path`, `labels`, and an optional RFC3339
`last_updated` value. A note is canonical only when its metadata entry and its
directory containing `note.md` agree. Attachments are filesystem children of the
note's `images/` directory and are not separate metadata entries.

## Public boundaries

- `NotebookManager` loads and saves metadata and engine state, reads and writes note
  content, and performs create, delete, and move transactions.
- `AttachmentManager` validates image signatures and manages attachment reads,
  uploads, replacements, and conditional deletes.
- `SearchIndexManager` owns the derived search index, cache, query parser, snippets,
  highlights, and incremental mutation hooks.
- `ConcurrencyManager` provides advisory notebook and note locks for cooperating
  processes.

## Safety and consistency invariants

1. Relative paths are validated before filesystem access. Absolute paths, traversal,
   invalid components, and symlink escapes outside the notebook are rejected.
2. Notebook-wide mutations acquire the notebook lock. Note-specific mutations acquire
   the notebook lock first and the note lock second. This order is mandatory.
3. Writes use temporary files, flush the file, atomically replace the destination,
   and synchronize the parent directory on Unix where supported.
4. Metadata writes preserve a valid backup and recover from it when the primary is
   missing or corrupt. A metadata update cannot add or remove note paths; lifecycle
   operations must be used for structural changes.
5. Note writes update note content and `last_updated` consistently. The engine returns
   content and metadata revisions so an API can enforce conditional writes.
6. Attachment replacement and deletion validate the expected BLAKE3 content revision
   while holding the notebook and note locks.
7. The persisted engine index is derived data. A failed index repair must be reported
   and recoverable without pretending that a canonical filesystem commit failed.
8. Lock acquisition is bounded. A held lock produces a typed `LockUnavailable` error
   rather than waiting forever.

## Search contract

Search indexes note paths, labels, and content. Queries support terms, quoted phrases,
negation, `label:`, `path:`, and `updated:FROM..TO` filters. Results include ranking,
match type, snippets, and character-based highlight ranges. The index is refreshed
from disk periodically and receives targeted updates after successful note, metadata,
move, and delete operations. It is always rebuildable from canonical files.

## Attachments

Supported image signatures are PNG, JPEG, GIF, and WebP. Attachment revisions are
BLAKE3 hashes of the bytes. Engine-level size enforcement is complemented by the API's
48 MiB request/body limit. Attachment paths must remain below `images/`; arbitrary
filesystem paths are never accepted.

## Error model and testing

Operations return `EngineError` variants for validation, storage, recovery, lock
availability, and conflicts. Tests cover traversal and symlink attacks, atomic-write
failures, rollback failures, backup recovery, concurrent writers, stale revisions,
attachment lifecycle behavior, and search-index recovery. New storage behavior must
include both success and failure-path tests.
