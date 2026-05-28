# LLM Configuration

Mossen uses local model profiles. The default user-level config file is:

```text
~/.mossen/settings.json
```

This file can contain API keys and must not be committed.

## Profile Schema

```json
{
  "mossen.activeProfile": "my-model",
  "mossen.profiles": {
    "my-model": {
      "provider": "openai-compatible",
      "baseURL": "https://api.example.com/v1",
      "model": "your-model-name",
      "apiKey": "<your-api-key>"
    }
  }
}
```

Supported `provider` values:

- `openai-compatible`
- `openai-responses`
- `anthropic`

## CLI Setup

Add a profile:

```bash
mossen --add-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model your-model-name \
  --apiKey "$YOUR_API_KEY"
```

Activate it:

```bash
mossen --set-model-profile my-model
```

List profiles:

```bash
mossen --list-model-profiles
```

Show the active profile with secrets redacted:

```bash
mossen --get-model-profile
```

Test a profile:

```bash
mossen --test-model-profile my-model --timeout 30000
```

Update a profile:

```bash
mossen --update-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model another-model-name \
  --apiKey "$YOUR_API_KEY"
```

Set only the key:

```bash
mossen --set-model-profile-key my-model "$YOUR_API_KEY"
```

Delete a profile:

```bash
mossen --delete-model-profile my-model
```

## Environment Override

Profiles are the recommended path. For temporary sessions, Mossen also accepts runtime environment variables:

```bash
export MOSSEN_CODE_USE_CUSTOM_BACKEND=1
export MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL=openai-compatible
export MOSSEN_CODE_CUSTOM_BASE_URL=https://api.example.com/v1
export MOSSEN_CODE_CUSTOM_MODEL=your-model-name
export MOSSEN_CODE_CUSTOM_API_KEY="$YOUR_API_KEY"
mossen
```

Use `MOSSEN_CODE_CUSTOM_AUTH_TOKEN` instead of `MOSSEN_CODE_CUSTOM_API_KEY` only when your provider expects a bearer token flow.

## Project Scope

By default, profile commands write user-level settings. A project scope exists for local experiments:

```bash
mossen --add-model-profile local-dev \
  --scope project \
  --provider openai-compatible \
  --baseURL http://localhost:8000/v1 \
  --model local-model \
  --apiKey "$LOCAL_TEST_KEY"
```

Do not use project-scoped config in a public repository unless `.mossen/` is ignored and you have verified no secrets are tracked.
