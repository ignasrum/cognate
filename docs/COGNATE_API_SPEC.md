# Cognate API Specification

## Scope

`cognate-api` is a standalone, authenticated HTTP service that exposes
`cognate-engine` to the desktop UI, TUI clients, and automation such as AI
assistants. The API process owns the notebook path and all filesystem operations;
clients never write the notebook directly.

The service speaks plain HTTP. It binds to loopback by default, and HTTPS is provided
externally by a reverse proxy when required. The API must not be treated as a public
internet-facing TLS terminator.

## Configuration

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `COGNATE_NOTEBOOK_PATH` | yes | — | Canonical notebook directory |
| `COGNATE_API_ADMIN_TOKEN` | yes | — | Bootstrap credential for client administration |
| `COGNATE_API_BIND_ADDRESS` | no | `127.0.0.1` | Listen address |
| `COGNATE_API_BIND_PORT` | no | `8787` | Non-zero TCP port |
| `COGNATE_API_DATABASE_PATH` | no | `cognate-api.sqlite` | Client credential database |

The standalone service uses SQLite for client metadata. Embedded local mode is
different: the desktop starts an in-process API on an ephemeral loopback port, uses an
in-memory client store, generates a secret for that process only, and creates no SQLite
database or secret file.

## Authentication

`GET /v1/health` is public and returns `{"status":"ok"}`. All other ordinary routes
require exactly one header:

```http
Authorization: Bearer cgnt_live_<high-entropy-secret>
```

Client secrets are generated from 32 random bytes. The server stores only the BLAKE3
digest, never the plaintext secret. Authentication hashes the presented secret and
compares digests in constant time. Revocation takes effect immediately. Secrets must
not appear in URLs, logs, or API responses after provisioning.

Admin routes use the separate `X-Admin-Token` bootstrap credential and are available
only to standalone SQLite-backed services:

- `POST /v1/admin/clients` provisions a named client and returns its secret once.
- `GET /v1/admin/clients` lists client metadata without secrets.
- `DELETE /v1/admin/clients/{id}` revokes a client idempotently.

## Endpoint contract

All protected endpoints use the authenticated client key.

| Method | Path | Purpose | Preconditions |
| --- | --- | --- | --- |
| `GET` | `/v1/notes` | Load metadata | — |
| `GET` | `/v1/notes/{rel_path}` | Read note content | —; returns `ETag` |
| `PUT` | `/v1/notes/{rel_path}` | Replace note content | `If-Match` required |
| `POST` | `/v1/notes` | Create a note | Valid relative path |
| `POST` | `/v1/notes/move` | Move a note | Valid paths and lifecycle checks |
| `DELETE` | `/v1/notes/{rel_path}` | Delete a note | Lifecycle validation |
| `PUT` | `/v1/metadata` | Update labels/timestamps | `If-Match` required; exact path set |
| `GET` | `/v1/search` | Legacy unpaginated search | Query validation |
| `GET` | `/v1/search/page` | Cursor-paginated search | `limit <= 100`; valid cursor |
| `GET` | `/v1/attachments?note=...` | List attachments | Existing/valid note path |
| `POST` | `/v1/attachments?note=...` | Upload image | Raw supported image bytes |
| `GET` | `/v1/attachments/{note}/images/{file}` | Download image | Valid attachment path |
| `PUT` | `/v1/attachments/{note}/images/{file}` | Replace image | `If-Match` attachment revision |
| `DELETE` | `/v1/attachments/{note}/images/{file}` | Delete image | `If-Match` attachment revision |

Note reads and writes use BLAKE3 content revisions as `ETag`. A stale or missing
`If-Match` is rejected with `409 Conflict` (or a precondition error as appropriate)
without overwriting canonical data. Metadata responses expose a metadata ETag, and
metadata writes cannot create, remove, or rename note paths. Structural changes must
use create, move, or delete.

Attachment downloads set `Content-Type`, `Content-Length`, and `ETag`. Uploads and
replacements accept supported image signatures only. Request bodies, note writes, and
attachment writes are limited to 48 MiB. Attachment deletion is conditional, so a
client cannot delete a newer version observed by another client.

## HTTP status and failure behavior

- `200`/`201`: successful read or mutation.
- `400`: malformed paths, query, cursor, payload, or image signature.
- `401`: missing, malformed, unknown, or revoked client key.
- `404`: missing note, attachment, or client.
- `409`: stale revision, structural metadata violation, or concurrent conflict.
- `413`: request exceeds the 48 MiB body limit.
- `500`: unexpected service or engine failure.

The API delegates locking, atomic writes, rollback, revisions, and search updates to
the engine. Search is derived state: a search-index repair failure is logged and can be
rebuilt without claiming that a successful canonical mutation was rolled back.

## Deployment requirements

Bind to loopback unless a trusted LAN interface is explicitly required. If binding to
a non-loopback address, restrict the port with a firewall and place a reverse proxy in
front when transport encryption or public access is needed. The reverse proxy owns TLS
certificates and forwards HTTP to the configured API address and port.
