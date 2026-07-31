# Note Storage and Metadata Specification

This document details how Cognate stores notes, tags, and metadata on the filesystem, as well as the mechanisms it employs to guarantee path safety, write atomicity, and crash recovery.

---

## 1. Directory Structure

A Cognate **Notebook** is represented by a single root directory on disk. Within this directory, notes are organized as follows:

```
<notebook_root>/
├── metadata.json
├── metadata.json.bak
├── .cognate_index.bin
├── .cognate_locks/
└── <relative_note_path>/
    ├── note.md
    └── images/
        └── <image_id>.<ext>
```

### Components:
1. **`metadata.json`**: The central register of all note files, their labels, and modification times.
2. **`metadata.json.bak`**: A snapshot of the last known-good metadata file, used for automatic recovery if the main metadata file becomes corrupted.
3. **Note Folders (`<relative_note_path>/`)**: Every note resides inside its own subdirectory. Folders can be nested (e.g., `recipes/desert/pie`).
4. **`note.md`**: The actual text content of the note is stored in a file named exactly `note.md` inside its corresponding note folder.
5. **`images/`**: A subdirectory within each note folder dedicated to storing embedded media/images pasted or inserted into that specific note.
6. **`.cognate_index.bin`**: Persisted search, task, and metrics state maintained by `cognate-engine`.
7. **`.cognate_locks/`**: Stable advisory lock files used to coordinate cooperating Cognate processes. These files may remain after a process exits.
8. **`<image_id>.<ext>`**: Automatically generated unique image files.

---

## 2. Metadata Data Schema

The metadata is serialized as a JSON object matching the `NotebookMetadata` structure in the codebase.

### Example `metadata.json`
```json
{
  "notes": [
    {
      "rel_path": "recipes/dessert/apple-pie",
      "labels": [
        "baking",
        "sweet"
      ],
      "last_updated": "2026-07-05T18:00:00Z"
    },
    {
      "rel_path": "todos/work",
      "labels": [
        "urgent"
      ],
      "last_updated": "2026-07-05T18:30:15Z"
    }
  ]
}
```

### Field Definitions:
- **`notes`** (`Vec<NoteMetadata>`): List of all tracked notes.
  - **`rel_path`** (`String`): The path to the note directory relative to the notebook root.
  - **`labels`** (`Vec<String>`): User-defined tags associated with the note.
  - **`last_updated`** (`Option<String>`): Optional RFC3339 formatted UTC timestamp tracking the note's last modification time.

---

## 3. Storage Operations & Consistency Guarantees

Cognate prioritizes data integrity and implements several layers of reliability mechanisms in `cognate-engine/src/storage/notebook.rs`, `cognate-engine/src/storage/concurrency.rs`, and the desktop adapters under `src/notebook/`.

### Concurrency and Cross-Process Writes

`cognate-engine` coordinates cooperating Cognate clients with advisory lock files under
`.cognate_locks/` in the notebook root. A notebook-wide lock protects metadata,
structural note operations, and the shared `.cognate_index.bin` read-modify-write
transaction. Note locks identify individual note paths. Current note-content saves
take the notebook lock first and then the note lock because they update the shared
index; attachment creation takes the note lock, while attachment deletion takes the
notebook lock. Operations that need both locks always use notebook-then-note order.

The lock is held for the complete logical mutation, including reads, atomic replacement,
index synchronization, and rollback. Atomic replacement prevents partial files, while
the advisory lock prevents a desktop, API, or TUI writer from silently overwriting a
concurrent update. Lock files can remain after a crash; the OS releases the lock and a
future client may reuse the same file. Clients should treat a lock timeout as a temporary
conflict and retry or ask the user to resolve the competing edit.

All writers must use `NotebookManager` (or an API built on it) rather than writing
`note.md`, `metadata.json`, or `.cognate_index.bin` directly. These locks are advisory:
they coordinate Cognate clients that honor this contract, but cannot stop unrelated
programs from changing notebook files.

### A. Atomic Writes
To prevent file corruption caused by partial writes (e.g., due to sudden application crashes or power loss), note, metadata, and engine-index writes are performed atomically:
1. Write the payload to a temporary file in the target parent directory:
   `.{filename}.cognate_tmp_{process_id}_{timestamp_nanos}`
2. Atomically rename the temporary file to the final destination file (using OS-level atomic rename capabilities via `fs::rename`).
3. If renaming fails, clean up the temporary file and abort.

### B. Metadata Backups and Auto-Recovery
Before writing an updated `metadata.json`, Cognate checks and backs up the existing metadata:
1. It validates that the current `metadata.json` file is syntactically correct by attempting to parse it.
2. If parse validation succeeds, it copies the valid metadata to `metadata.json.bak`.
3. If Cognate subsequently fails to parse `metadata.json` upon startup (e.g., due to manual tampering or external corruption), it automatically attempts to load and restore metadata from `metadata.json.bak` and posts a recovery warning.

### C. Staged Deletions (Transactions)
Deleting a note is a dangerous operation. Cognate handles it via a staged transactional flow to prevent loss of consistency between the filesystem and the metadata:
1. **Stage**: The note directory is moved (renamed) to a unique transaction path in the notebook root:
   `.cognate_txn_delete_<sanitized_relative_path>_<timestamp_nanos>`
2. **Metadata Commit**: The note metadata is removed from the in-memory registry, and the updated metadata is written atomically to `metadata.json`.
3. **Commit / Finalize**: If the metadata save succeeds, the staged folder is recursively deleted from disk.
4. **Rollback**: If the metadata save fails, the transaction is rolled back by renaming the staged folder back to its original location.

#### Orphan Cleanup
If the application crashes during a delete transaction, staged delete directories might remain on disk. Upon loading a notebook, Cognate runs a cleanup task that scans for folders prefixed with `.cognate_txn_delete_` and permanently removes any that are older than the grace period of **5 minutes** (`STAGED_DELETE_CLEANUP_GRACE_NANOS`).

### D. Timestamp Reconciliation
During notebook load, Cognate reconciles differences between the persisted metadata timestamps and actual filesystem modified times:
- For each note, Cognate queries the filesystem modification time (`mtime`) of `note.md`.
- If the filesystem modified time is newer than the recorded `last_updated` value (or if `last_updated` is absent), the metadata timestamp is updated to match the filesystem's `mtime`.
- If changes are detected, the reconciled metadata is written back to `metadata.json` to keep the file in sync.

---

## 4. Path Validation and Safety

To prevent directory traversal attacks and unauthorized filesystem modifications:
1. **Relative Path Constraints**: Engine paths are validated by `validate_relative_path`, which enforces:
   - Paths cannot be empty.
   - Absolute paths (starting with root or prefix) are rejected.
   - Component validation: Path components containing `.` (current directory) or `..` (parent directory) are strictly forbidden.
2. **Containment Verification**: Prior to performing mutations (creating, deleting, or moving notes), paths are canonicalized and checked to ensure they reside strictly within the canonical notebook root.

---

## 5. Search Index Ownership

For high-performance text searches across note contents, Cognate maintains an in-memory cache layer in `cognate-engine/src/search/manager.rs`:
- Note contents are cached locally alongside their filesystem modified time (`mtime`).
- The search index automatically invalidates or refreshes entries if the note file is modified externally (validated on an interval).
- Staging and renaming operations sync automatically with this index to ensure search results are up to date.
- Both embedded-local and remote UI searches use the same API endpoint, engine query parser, and result contract. Supported filters include `label:`, `path:`, `updated:FROM..TO`, quoted phrases, and negated terms/filters.
- API search managers are reused between requests and return bounded pages from `/v1/search/page` with a cursor, match type, snippet, score, and character-based highlight ranges. `/v1/search` remains an array-response compatibility endpoint.
- Search manager entries are bounded and evicted by idle time/LRU order. Successful note and metadata mutations invalidate or update the active search index; filesystem refresh remains a safety net for external writers.
- Note writes, metadata changes, moves, and deletes use targeted index mutations. A failed targeted update falls back to a safe rebuild on a later search.

The desktop UI does not own a filesystem search index and does not directly read or
write note files or attachments. In `storage_backend: "local"`, the UI starts an
embedded loopback API; in `storage_backend: "api"`, it connects to the configured
self-hosted API. The API process is the only storage boundary used by the UI.

---

## 6. Embedded Image Storage & Lifecycle

Cognate supports embedding images directly into Markdown notes via pastes or file paths. Images are stored inside the `images/` directory within the corresponding note folder.

### A. Paste and Clipboard Processing
Pasting imagery (handled in `src/components/editor/core/clipboard.rs`) supports two methods:
1. **File Copy-Paste**: If the clipboard contains path text or `file://` URIs, Cognate parses and checks if the target file represents a supported image type. If so, it reads the image binary off disk and encodes it to base64.
2. **Raw Bitmap Paste**: If the clipboard contains direct image data (e.g., from taking a screenshot), Cognate uses the `png` crate to encode the raw pixel bytes into a standard PNG byte vector, then base64-encodes it.

### B. Persistent Storage and Magic Byte Detection
Once base64 image payload data is retrieved, it is written to the note's storage directory (handled in `src/components/editor/core/embedded_images.rs`):
1. **Format/Extension Resolution**: The raw bytes are decoded from base64. Cognate inspects the magic bytes at the beginning of the binary payload to determine the image format/extension:
   - `\x89PNG\r\n\x1a\n` -> `.png`
   - `\xFF\xD8\xFF` -> `.jpg`
   - `GIF87a` / `GIF89a` -> `.gif`
   - `RIFF` ... `WEBP` -> `.webp`
   - Default fallback: `.png`
2. **Unique ID Generation**: A unique identifier is generated based on the system timestamp in hexadecimal format:
   `img_<timestamp_nanoseconds_hex>.<extension>` (e.g., `img_17b5e40e2cf082a0.png`).
3. **Write**: The directory `<notebook_root>/<relative_note_path>/images/` is created if it does not exist, and the image bytes are written to file.
4. **Markdown Insertion**: A Markdown image tag referencing the relative path is pasted into the editor text surface:
   `![image](images/img_<id>.<ext>)`

### C. Resolution and UI Rendering
During text rendering or markdown preview (handled in `src/components/editor/core/embedded_image_service.rs`):
- Cognate parses the current Markdown text for image reference IDs (e.g. `images/img_<id>.<ext>`) using regular expressions (`extract_embedded_image_ids`).
- The `EmbeddedImageWorkflow` struct maps these IDs to their absolute paths:
  `<notebook_root>/<relative_note_path>/images/img_<id>.<ext>`.
- The service loads these files from disk and produces Iced UI image handles (`iced::widget::image::Handle::from_bytes`) to render them in the preview canvas.

### D. Dereferencing and Cleanup
To prevent unused image files from consuming disk space, Cognate detects when an image is deleted/replaced:
1. When a user edit occurs, `EmbeddedImageWorkflow` compares the previous markdown image reference IDs against the new markdown text.
2. If any image reference has been deleted/edited out, the image ID is added to a `pending_deletion_ids` queue.
3. Upon confirming/committing the edit state change, Cognate removes the files through `AttachmentManager`, which coordinates the deletion with the notebook lock.
