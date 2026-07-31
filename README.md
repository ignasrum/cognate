# cognate

Cognate is a desktop personal knowledge service built with Rust and Iced for organizing Markdown notes on disk and exploring how they connect through labels.

## Features

- Markdown editor with live preview
- Notebook-style file organization
- Note and folder create/delete/move flows
- Labeling for note categorization
- Embedded image workflow for pasted images
- Visualizer for label-connected notes
- Theme and UI scale configuration via `config.json`

## Screenshots

### Editor Workspace

![Cognate editor workspace with note explorer, markdown editor, and live preview](docs/editor.png)

### Label Visualizer

![Cognate visualizer showing label-connected note graph](docs/visualizer.png)

## Quick Start

### Prerequisites

- Rust and Cargo (install from [https://rustup.rs/](https://rustup.rs/))

### Build and run

Use `make`:

- `make build` builds in release mode
- `make install` installs the final release app
- `make run` runs the app
- `make test` runs the full test suite
- `make help` lists all targets

Or use Cargo directly:

```bash
cargo build --release
cargo run --release
```

### Configuration

Cognate reads configuration from `./config.json` by default. Override with:

```bash
COGNATE_CONFIG_PATH=/path/to/config.json cargo run --release
```

Example:

```json
{
  "theme": "CatppuccinMacchiato",
  "notebook_path": "/home/{USER}/Documents/cognate/example_notebook",
  "scale": 1.0,
  "storage_backend": "local",
  "api_url": "http://127.0.0.1:8787",
  "api_key": ""
}
```

- `theme` is the UI theme name
- `notebook_path` points to your notes root directory
- `scale` is the global UI scale and must be positive
- `storage_backend` selects `local` filesystem storage or the Cognate API (`api`)
- `api_url` is the API base URL, for example `http://127.0.0.1:8787`
- `api_key` is the bearer key issued by `cognate-api`; keep this config file private and do not commit it

When `storage_backend` is `api`, note metadata, note content, create/delete/move operations, search, and embedded attachments use `cognate-api`. The API is expected to run on local HTTP; HTTPS termination belongs in an external reverse proxy. Attachment uploads and downloads are authenticated and limited to supported image signatures.

Search accepts plain text plus optional filters such as `label:work`, `path:projects/`, `updated:2026-01-01..2026-12-31`, quoted phrases, and negation (`-label:archive`). Existing clients can use `GET /v1/search?q=...` for a legacy result array; paginated clients should use `GET /v1/search/page?q=...&limit=25&cursor=...`. Limits are bounded to 100 results per page. Results include a match type, snippet, score, and character-based highlight ranges.

Search matches are rendered with highlighted text in the desktop UI. Search failures are distinguished from valid empty results, and transient API failures can be retried. Search indexes are refreshed after successful note and metadata mutations.

API note writes use content-hash revisions exposed as `ETag` values. Clients must send the revision they loaded in `If-Match`; stale writes receive `409 Conflict` and do not overwrite the server version. If the API is temporarily unreachable, API-mode note content is saved in the permissions-restricted `.cognate-api-queue.json` file beside the configuration and retried on a later save.

When a conflict is detected, the editor shows the local draft and server version together. You can keep the server version, retry the local draft against the latest server revision, or save the local draft as a separate `.conflict` note. Dismissing the dialog leaves both versions available for another resolution attempt.

## Documentation

- [Development guide](docs/DEVELOPMENT.md)
- [Architecture guide](docs/ARCHITECTURE.md)
- [Manual testing checklist](docs/MANUAL_TESTING.md)

## Project Layout

- `src/components` contains UI/editor components
- `src/notebook` implements note metadata, storage, operations, and search
- `src/configuration` handles config parsing and theme mapping
- `src/tests` contains integration-style unit tests across modules
