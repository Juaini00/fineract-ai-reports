# Project Setup: 7. Initial run() Target

Source: `docs-old/project-setup.md`

## 7. Initial run() Target

The first real `run()` should do only this:

```text
load .env
load config
init tracing
start HTTP server
serve /health
```

Do not connect databases yet in the first implementation if the HTTP server is not running.

After `/health` works, add database pools and `/ready`.
