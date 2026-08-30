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

Or use Cargo directly from the repository root. The desktop application is the `cognate` package in `cognate-ui/`, configured as the workspace default member:

```bash
cargo build --release
cargo run --release
cargo run -p cognate --release
```

Use `cargo test --workspace` to test the desktop UI, API, and engine packages together.

### Configuration

Cognate reads configuration from `./config.json` by default. Override with:

```bash
COGNATE_CONFIG_PATH=/path/to/config.json cargo run --release
```

Choose one of these configurations. The desktop app always needs a
`notebook_path`. In API mode, the server's `COGNATE_NOTEBOOK_PATH` is the
authoritative notes directory; the desktop path does not need to exist on the
API host.

#### Local mode (embedded API, recommended for one desktop)

```json
{
  "theme": "CatppuccinMacchiato",
  "notebook_path": "/home/ignasr/Documents/cognate/notebook",
  "scale": 1.0,
  "storage_backend": "local"
}
```

Run it with:

```bash
COGNATE_CONFIG_PATH=./config.local.json cargo run --release
```

Local mode silently starts an in-process API on an ephemeral loopback port.
Do not add `api_url` or `api_key`; they are ignored in this mode.

#### API mode (desktop connects to a self-hosted `cognate-api`)

```json
{
  "theme": "CatppuccinMacchiato",
  "notebook_path": "/home/ignasr/Documents/cognate/client-state",
  "scale": 1.0,
  "storage_backend": "api",
  "api_url": "http://127.0.0.1:8787",
  "api_key": "cgnt_live_replace_with_client_secret"
}
```

Start the API on the same machine, then run the UI:

```bash
COGNATE_NOTEBOOK_PATH=/home/ignasr/Documents/cognate/notebook \
COGNATE_API_ADMIN_TOKEN='replace-with-admin-token' \
COGNATE_API_BIND_ADDRESS=127.0.0.1 \
COGNATE_API_BIND_PORT=8787 \
cargo run -p cognate-api --release

COGNATE_CONFIG_PATH=./config.api.json cargo run --release
```

#### API mode (desktop connects over a LAN)

On the API host, bind to its LAN address:

```bash
COGNATE_NOTEBOOK_PATH=/srv/cognate/notebook \
COGNATE_API_ADMIN_TOKEN='replace-with-admin-token' \
COGNATE_API_BIND_ADDRESS=192.168.10.69 \
COGNATE_API_BIND_PORT=8787 \
cargo run -p cognate-api --release
```

Use the host address in the desktop config:

```json
{
  "theme": "Dark",
  "notebook_path": "/home/ignasr/Documents/cognate/client-state",
  "scale": 1.0,
  "storage_backend": "api",
  "api_url": "http://192.168.10.69:8787",
  "api_key": "cgnt_live_replace_with_client_secret"
}
```

Direct LAN HTTP is unencrypted. Restrict port `8787` with a firewall or place
the API behind a reverse proxy that terminates HTTPS; the API itself serves
plain HTTP only.

- `theme` is the UI theme name
- `notebook_path` points to your notes root directory
- `scale` is the global UI scale and must be positive
- `storage_backend` selects embedded local API mode (`local`) or a remote Cognate API (`api`)
- `api_url` is the API base URL, for example `http://127.0.0.1:8787`
- `api_key` is the bearer key issued by a remote `cognate-api`; keep this config file private and do not commit it. It is ignored in embedded local mode.

For search and reflection integrations, provision a separate `read_only` API client.
That key can search and read the permitted notebook data but cannot write notes,
metadata, or attachments. Do not reuse the desktop client's read-write key.

When `storage_backend` is `local`, Cognate silently starts an embedded `cognate-api` on an ephemeral `127.0.0.1` HTTP port. The UI keeps a newly generated client secret only in memory; no API SQLite database or secret file is created. The notebook filesystem remains the durable local state.

When `storage_backend` is `api`, note metadata, note content, create/delete/move operations, search, and embedded attachments use the configured remote `cognate-api`. The API is expected to run on local HTTP; HTTPS termination belongs in an external reverse proxy. Attachment uploads and downloads are authenticated and limited to supported image signatures.

Search accepts plain text plus optional filters such as `label:work`, `path:projects/`, `updated:2026-01-01..2026-12-31`, quoted phrases, and negation (`-label:archive`). Existing clients can use `GET /v1/search?q=...` for a legacy result array; paginated clients should use `GET /v1/search/page?q=...&limit=25&cursor=...`. Limits are bounded to 100 results per page. Results include a match type, snippet, score, and character-based highlight ranges.

Search matches are rendered with highlighted text in the desktop UI. Search failures are distinguished from valid empty results, and transient API failures can be retried. Search indexes are refreshed after successful note and metadata mutations.

The paginated search API returns stable error codes such as `invalid_query`, `invalid_cursor`, and `search_internal` for validation and index failures. Successful note, move, delete, and metadata operations update the active search index incrementally.

API note writes use content-hash revisions exposed as `ETag` values. Clients must send the revision they loaded in `If-Match`; stale writes receive `409 Conflict` and do not overwrite the server version. If the API is temporarily unreachable, API-mode note content is saved in the permissions-restricted `.cognate-api-queue.json` file beside the configuration and retried on a later save.

Metadata updates are non-structural and must preserve the existing set of valid note paths. Wildcard metadata writes are limited to an empty notebook; use the note create/delete/move endpoints for structural changes. This prevents metadata updates from creating dangling entries or hiding note directories on disk.

When a conflict is detected, the editor shows the local draft and server version together. You can keep the server version, retry the local draft against the latest server revision, or save the local draft as a separate `.conflict` note. Dismissing the dialog leaves both versions available for another resolution attempt.

## Documentation

- [Development guide](docs/DEVELOPMENT.md)
- [Architecture guide](docs/ARCHITECTURE.md)
- [Manual testing checklist](docs/MANUAL_TESTING.md)

## Project Layout

- `cognate-ui/src/components` contains UI/editor components
- `cognate-ui/src/notebook` implements the desktop API client and runtime storage integration
- `cognate-ui/src/configuration` handles config parsing and theme mapping
- `cognate-ui/src/tests` contains desktop package tests
- `cognate-api` contains the standalone HTTP service
- `cognate-engine` contains shared notebook persistence, locking, attachments, and search
