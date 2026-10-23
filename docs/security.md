# Security

## What is sent

Every included file is sent to OpenRouter as part of the state document. Komply
has no extension filter: if a UTF-8 file is under the target and not ignored, it
is sent.

Use [`.complyignore`](ignores.md) or `[files].exclude` to keep secrets,
generated files, and large assets out of the document. Check the dry run's file
summary before a live run.

Non-UTF-8 files are skipped and reported. They are not sent.

## The API key

- `OPENROUTER_API_KEY` is read only when a live run constructs the decisions
  repository.
- The key is never put in the config file, the request body, or the output.
- The key is never written to disk.
- A dry run needs no key and makes no network call.

`COMPLY_DECISIONS_URL` can override the endpoint for development and tests. It
is also read only when a live run constructs the decisions client.

## Limits

The final state document is capped by `max_document_bytes`. A single file over
the same limit is an error before its bytes are read. An oversized document is
an error; Komply never truncates silently. See [Config](config.md).

## What komply does not do

- It does not write to the checked project.
- It does not open a database.
- It does not cache answers.
- It sends one logical request per run. The decisions repository may retry a
  transient HTTP failure.
