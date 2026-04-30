# Missing: `gt runner` subcommand for Actions runners

## Summary

`gt` has no first-class subcommand for managing Gitea Actions runners. The endpoints exist in the Gitea API (admin + per-repo + per-org) but users have to fall back to `gt api` and remember the path layout. `gh` has `gh runners` (and `gh actions runner ...`) for the same purpose.

## Use case

Drawbar (Forgejo/Gitea-Actions runner) registration on a fresh Gitea instance is the common ops flow. Today it requires:

```bash
gt api admin/actions/runners                          # list registered runners (admin scope)
gt api -X POST admin/actions/runners/registration-token  # mint a registration token
```

Both work via `gt api` but require knowing:

- the endpoints exist
- the path uses `actions/runners` not `runners`
- the registration-token endpoint is `POST` not `GET` (returns 404 on GET)
- which scope the token needs (`read:admin` or `write:admin`)

## Proposed shape

Mirror `gh runners`:

```
gt runner list                                      # current scope (depends on -R)
gt runner list --org <name>                         # org-scoped
gt runner list --admin                              # all runners on instance (admin only)
gt runner registration-token [--repo R | --org O | --admin]
gt runner delete <id>                               # remove a runner
gt runner status <id>                               # show last poll, labels, idle/busy
```

Endpoint mapping (verify against current Gitea swagger):

| `gt runner` form | Gitea endpoint |
|---|---|
| `runner list --admin` | `GET /api/v1/admin/actions/runners` |
| `runner list --org X` | `GET /api/v1/orgs/X/actions/runners` |
| `runner list -R o/r` | `GET /api/v1/repos/o/r/actions/runners` |
| `runner registration-token --admin` | `POST /api/v1/admin/actions/runners/registration-token` |
| `runner registration-token --org X` | `POST /api/v1/orgs/X/actions/runners/registration-token` |
| `runner registration-token -R o/r` | `POST /api/v1/repos/o/r/actions/runners/registration-token` |
| `runner delete <id>` (admin) | `DELETE /api/v1/admin/actions/runners/<id>` |

## Acceptance

- `gt runner list` works in a repo-detected context.
- `gt runner registration-token --admin` returns a token that can register a runner.
- 403 from a non-admin token surfaces a hint to log in with admin creds.

---

## Resolution (2026-04-30)

Implemented as `gt runner {list, view, delete, registration-token}` with `--admin`, `--org NAME`, and `-R OWNER/REPO` scope flags. User-scope runners (`/user/actions/runners/*`), `gt runner edit`, and pagination flags are deferred — see `docs/superpowers/specs/2026-04-30-gt-runner-design.md` for the full design.

Note: this bug body claimed `registration-token` is `POST` (correct) and "returns 404 on GET" — empirically the live server returns 403 with a scope-required message on GET, since the auth gate runs before verb routing. The actual route is `POST`-only, confirmed against Gitea source `routers/api/v1/api.go`.

Note: the Gitea OpenAPI spec describes the registration-token response as a *header*, but the live server returns JSON `{"token": "..."}`. This implementation uses `Gitea::raw_request` to POST and parse the JSON body, bypassing progenitor's typed builder which (correctly per the spec) discards the response body. Worth filing upstream against `go-gitea/gitea` to align the swagger annotation with actual behavior.
