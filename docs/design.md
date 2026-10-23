# Design

Komply has four parts:

```text
comply
  └── testing service
        ├── rules repository
        ├── files repository
        └── decisions repository
```

The CLI parses the command, resolves the working directory and target, loads
config, and builds the service. The service owns the decisions:

1. Resolve the rule files that apply to the target.
2. Merge their file filters.
3. Collect the target's UTF-8 files.
4. Build one state document from the files.
5. Build one `noul` question per enabled rule.
6. Send one request and map every probability to a verdict.

The repositories only read data:

- The rules repository reads TOML files.
- The files repository reads the target directory and `.complyignore`.
- The decisions repository sends one HTTP request and returns a `Decision`.

The decisions repository owns the HTTP details, including its fixed retry
budget. The service never reads a status code or a response body.

One run sends one logical request. The decisions repository may retry a
transient HTTP failure inside that request. There is no database, no cache, and
no write to the checked project. Every run reads rules from `.architecture/` in
the current working directory.
