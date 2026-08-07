# Cognate API key provisioning

This document describes how to provision credentials for a self-hosted
`cognate-api` instance. Cognate has two credential types:

- The admin token provisions, lists, and revokes client credentials.
- A client secret authenticates an application to the notebook API.

Use a separate client secret for every application. In particular, give AI
integrations a `read_only` client and never reuse the desktop client's
read-write credential.

## 1. Start the API

Set the notebook path and a strong admin bootstrap token before starting the
standalone server:

```sh
export COGNATE_NOTEBOOK_PATH=/srv/cognate/notebook
export COGNATE_API_ADMIN_TOKEN='replace-with-a-long-random-admin-token'
export COGNATE_API_BIND_ADDRESS=127.0.0.1
export COGNATE_API_BIND_PORT=8787

cargo run -p cognate-api --release
```

The database defaults to `cognate-api.sqlite`. Set
`COGNATE_API_DATABASE_PATH` when it should live elsewhere:

```sh
export COGNATE_API_DATABASE_PATH=/var/lib/cognate/cognate-api.sqlite
```

The API speaks HTTP directly. For a remote client, bind it to a trusted local
or private interface and terminate HTTPS in a reverse proxy. Do not expose
plain HTTP to an untrusted network because bearer secrets and note contents
would be sent without transport encryption.

Check that the process is reachable:

```sh
curl --fail http://127.0.0.1:8787/v1/health
# {"status":"ok"}
```

## 2. Provision a read-write client

Use the admin token only in the `X-Admin-Token` header. It is not a client
secret and must not be used in the `Authorization` header for normal API
requests.

```sh
curl --fail-with-body -X POST http://127.0.0.1:8787/v1/admin/clients \
  -H 'X-Admin-Token: replace-with-a-long-random-admin-token' \
  -H 'Content-Type: application/json' \
  -d '{"client_name":"macbook-tui","access_mode":"read_write"}'
```

The response contains a generated secret similar to:

```json
{
  "id": "cgnt_client_...",
  "client_name": "macbook-tui",
  "secret": "cgnt_live_8f3a...",
  "created_at": "2026-08-08T12:00:00Z",
  "access_mode": "read_write"
}
```

Copy the `secret` immediately into the application's protected credential
store or a permissions-restricted configuration file. The API never returns
the secret again. The SQLite database stores only its BLAKE3 digest, never the
plain client secret.

Example authenticated request:

```sh
export COGNATE_CLIENT_KEY='cgnt_live_...'
curl --fail http://127.0.0.1:8787/v1/notes \
  -H "Authorization: Bearer $COGNATE_CLIENT_KEY"
```

Never put a client secret in a URL, notebook metadata, shell command history,
logs, screenshots, or an AI prompt.

## 3. Provision a read-only client

Read-only clients are intended for search tools, indexing viewers, and other
integrations that must not modify the notebook:

```sh
curl --fail-with-body -X POST http://127.0.0.1:8787/v1/admin/clients \
  -H 'X-Admin-Token: replace-with-a-long-random-admin-token' \
  -H 'Content-Type: application/json' \
  -d '{"client_name":"search-integration","access_mode":"read_only"}'
```

A read-only client may:

- check health;
- read notebook metadata;
- read note content;
- search notes, including paginated search;
- list and download supported attachments.

It receives `403 read_only_client` for note, metadata, attachment, and
lifecycle writes. Each read-only integration should use a dedicated key
created with:

```json
{"client_name":"search-integration","access_mode":"read_only"}
```

## 4. List and revoke clients

List client metadata without exposing secrets:

```sh
curl --fail http://127.0.0.1:8787/v1/admin/clients \
  -H 'X-Admin-Token: replace-with-a-long-random-admin-token'
```

Revoke a credential using its returned client `id`:

```sh
curl --fail -X DELETE \
  http://127.0.0.1:8787/v1/admin/clients/cgnt_client_... \
  -H 'X-Admin-Token: replace-with-a-long-random-admin-token'
```

Revocation takes effect immediately. The old secret cannot be recovered or
reactivated; provision a new client if the application needs access again.

## 5. Reverse-proxy deployment

Keep Cognate bound to loopback or a private interface and expose only the
reverse proxy publicly. The proxy should:

1. terminate HTTPS with a valid certificate;
2. forward requests to the configured Cognate HTTP address and port;
3. preserve the `Authorization` header;
4. restrict access with firewall or network policy as appropriate;
5. avoid logging authorization headers and request bodies.

Example Nginx shape:

```nginx
server {
    listen 443 ssl;
    server_name notes.example.com;

    ssl_certificate     /etc/letsencrypt/live/notes.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/notes.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8787;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header Authorization $http_authorization;
    }
}
```

Do not configure the Cognate API itself with certificates; TLS termination is
intentionally external to the service.

## 6. Rotation and incident response

When a key may have leaked:

1. identify the client from the provisioning record or client list;
2. revoke it immediately;
3. inspect proxy and API logs for accidental credential exposure;
4. provision a replacement client with the smallest required access mode;
5. update the application secret without committing it to source control.

Rotate the admin token by stopping the service, changing
`COGNATE_API_ADMIN_TOKEN`, and restarting it. The admin token is compared from
the environment and is not used as a client credential.

## 7. Embedded/local mode

The desktop application's `local` storage mode starts an embedded API on an
ephemeral loopback port. It uses an in-memory client store, generates a fresh
client secret on each launch, and does not create the standalone API SQLite
database. There are no admin provisioning routes in embedded mode. The
notebook filesystem remains the durable local state.

## Troubleshooting

- `401 Unauthorized`: the bearer header is missing, malformed, revoked, or the
  secret is incorrect. Use `Authorization: Bearer <client-secret>` exactly.
- `403 read_only_client`: the request attempted a write with a read-only key.
  Provision a separate read-write client only for applications that genuinely
  need mutation access.
- Connection refused: verify `COGNATE_API_BIND_ADDRESS`,
  `COGNATE_API_BIND_PORT`, firewall rules, and reverse-proxy upstream settings.
- Timeout: check the proxy timeout and API health endpoint; do not solve this by
  placing credentials in URLs or disabling TLS at the public boundary.
