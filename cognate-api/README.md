# cognate-api

`cognate-api` is the local HTTP service that exposes `cognate-engine` to authenticated clients.

The service binds to loopback by default. An explicit bind address can be used for a trusted
LAN or when an external reverse proxy needs to reach the service. Public HTTPS termination
and certificates belong to the reverse proxy.

## Configuration

Required:

- `COGNATE_NOTEBOOK_PATH`
- `COGNATE_API_ADMIN_TOKEN`

Optional:

- `COGNATE_API_BIND_ADDRESS` — IP address to listen on, default `127.0.0.1`
- `COGNATE_API_BIND_PORT` — TCP port, default `8787`
- `COGNATE_API_DATABASE_PATH` — SQLite path, default `cognate-api.sqlite`

Run locally:

```text
COGNATE_NOTEBOOK_PATH=/path/to/notebook \
COGNATE_API_ADMIN_TOKEN='admin-bootstrap-secret' \
cargo run -p cognate-api
```

To listen on a LAN interface:

```text
COGNATE_API_BIND_ADDRESS=192.168.10.69 \
COGNATE_API_BIND_PORT=8787 \
COGNATE_NOTEBOOK_PATH=/path/to/notebook \
COGNATE_API_ADMIN_TOKEN='admin-bootstrap-secret' \
cargo run -p cognate-api
```

Direct LAN HTTP exposes bearer tokens and note contents without transport encryption. Restrict
the port with a firewall or put the service behind a reverse proxy that terminates HTTPS.

## Client credentials

Provisioning requires the configured admin token:

```text
curl -X POST http://127.0.0.1:8787/v1/admin/clients \
  -H 'X-Admin-Token: admin-bootstrap-secret' \
  -H 'Content-Type: application/json' \
  -d '{"client_name":"macbook-tui"}'
```

The response contains one `cgnt_live_...` secret. Store it in the client’s protected credential store immediately; it is not recoverable from the server. The SQLite database stores only the BLAKE3 digest of the secret. Never place the secret in URLs or logs.

Revoke a client with its returned `id`:

```text
curl -X DELETE http://127.0.0.1:8787/v1/admin/clients/{id} \
  -H 'X-Admin-Token: admin-bootstrap-secret'
```

Authenticated client requests use:

```text
Authorization: Bearer cgnt_live_...
```

Attachment clients use these authenticated routes:

- `POST /v1/attachments?note=<note-path>` with raw image bytes to upload
- `GET /v1/attachments?note=<note-path>` to list attachments
- `GET /v1/attachments/<note-path>/images/<file>` to download an attachment
- `DELETE /v1/attachments/<note-path>/images/<file>` to delete an attachment

Uploads are limited to 16 MiB and supported image signatures. Downloads return an `ETag`; replacement requests use `If-Match`.

When exposed externally, configure the reverse proxy to terminate HTTPS and forward to the
configured address and port.
