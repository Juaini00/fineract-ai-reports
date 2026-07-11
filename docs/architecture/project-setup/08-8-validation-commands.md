# Project Setup: 8. Validation Commands

Source: `docs-old/project-setup.md`

## 8. Validation Commands

Check workspace:

```bash
cargo metadata --verbose --format-version 1 --all-features --filter-platform aarch64-apple-darwin
```

Check compile:

```bash
cargo check
```

Run app:

```bash
cargo run -p app
```

Test health endpoint after server exists:

```bash
curl http://127.0.0.1:3007/health
```
