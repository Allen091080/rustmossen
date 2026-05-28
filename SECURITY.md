# Security Policy

## Secrets

Do not publish real model provider credentials, API keys, auth tokens, local settings, transcripts, or `.env` files.

Mossen model profiles are stored locally in:

```text
~/.mossen/settings.json
```

This file can contain API keys and must not be committed. Project-local `.mossen/` directories are ignored because they may contain local settings and render/session artifacts.

## Reporting Issues

For public security reports, open a private advisory if the hosting platform supports it. If not, contact the maintainers privately before publishing exploit details.

## Supported Version

The current public source target is V1.0.0.
