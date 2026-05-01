# `gt api` 403 errors should hint at the auth/scope fix

## Summary

When a request fails with 403 because the token lacks a required scope, Gitea's response body already names the missing scope (e.g. `required=[read:admin]`). `gt` surfaces the raw JSON but does not translate that into actionable next steps.

## Example

```
$ gt api admin/actions/runners
Error:
   0: 403 Forbidden
      {"message":"token does not have at least one of required scope(s), required=[read:admin], token scope=write:organization,write:package,write:issue,write:repository,write:user","url":"https://gt.example/api/swagger"}
Location:
   gt/src/api.rs:202
```

A user new to Gitea tokens has to read the message carefully to figure out the fix. An ops user knows immediately. Both would benefit from a one-line hint.

## Proposed fix

When `api.rs:202` parses a 403 with `code/required` in the message, add a follow-up line:

```
hint: token is missing required scope(s): read:admin
hint: re-run `gt auth login` with a token that includes those scopes,
      or use a different token (admin tokens cover read:admin).
```

Keep the raw JSON for users who want it — only add the hint, don't replace.

Same shape on 401 (`Token rejected by server. Run \`gt auth login\` to refresh.`) — see bug 001.

## Acceptance

- 403 with a `required=[...]` body emits a one-line hint listing the missing scopes.
- 401 emits a one-line hint to re-run `gt auth login`.
- Other 4xx responses are unchanged.

---

## Resolution (2026-05-01)

Hints added in `gt/src/api.rs::send_request` via `auth_hint_for(status, body)` and `extract_required_scopes(message)`. They print to stderr (`hint: ...`) before the existing `bail!`, so the raw JSON response is preserved.

- 401 → `hint: token rejected by server. Run \`gt auth login\` to refresh.`
- 403 with `required=[<scopes>]` → names the scopes and suggests re-login or an admin token. Generic 403s (no scope info) still print the response unchanged — relying on `main.rs`'s top-level `HTTP 403` hint would require also reformatting the bail prefix, which the bug didn't ask for.
- Other 4xx pass through unchanged.

The top-level handler in `main.rs:185` keys on `HTTP 401` / `HTTP 403`, but `gt api`'s bail uses the bare `403 Forbidden` form, so its hints don't fire here. Inlining the hint in `api.rs` keeps the scope-specific message close to the body parsing and avoids reshaping an unrelated error string.
