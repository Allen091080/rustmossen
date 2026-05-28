# LLM 配置

[English](CONFIGURATION.md) | [简体中文](CONFIGURATION.zh-CN.md)

Mossen 使用本地模型 profile。默认用户级配置文件是：

```text
~/.mossen/settings.json
```

这个文件可能包含 API key，不能提交到仓库。

## Profile 结构

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

支持的 `provider` 值：

- `openai-compatible`
- `openai-responses`
- `anthropic`

## CLI 配置

添加 profile：

```bash
mossen --add-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model your-model-name \
  --apiKey "$YOUR_API_KEY"
```

启用 profile：

```bash
mossen --set-model-profile my-model
```

列出 profile：

```bash
mossen --list-model-profiles
```

查看当前 profile，密钥会被隐藏：

```bash
mossen --get-model-profile
```

测试 profile：

```bash
mossen --test-model-profile my-model --timeout 30000
```

更新 profile：

```bash
mossen --update-model-profile my-model \
  --provider openai-compatible \
  --baseURL https://api.example.com/v1 \
  --model another-model-name \
  --apiKey "$YOUR_API_KEY"
```

只更新 key：

```bash
mossen --set-model-profile-key my-model "$YOUR_API_KEY"
```

删除 profile：

```bash
mossen --delete-model-profile my-model
```

## 环境变量覆盖

推荐使用 profile。临时会话也可以通过环境变量指定运行时模型后端：

```bash
export MOSSEN_CODE_USE_CUSTOM_BACKEND=1
export MOSSEN_CODE_CUSTOM_BACKEND_PROTOCOL=openai-compatible
export MOSSEN_CODE_CUSTOM_BASE_URL=https://api.example.com/v1
export MOSSEN_CODE_CUSTOM_MODEL=your-model-name
export MOSSEN_CODE_CUSTOM_API_KEY="$YOUR_API_KEY"
mossen
```

只有当你的供应商明确要求 bearer token 流程时，才使用 `MOSSEN_CODE_CUSTOM_AUTH_TOKEN` 代替 `MOSSEN_CODE_CUSTOM_API_KEY`。

## 项目级配置

默认情况下，profile 命令写入用户级配置。项目级 scope 可用于本地实验：

```bash
mossen --add-model-profile local-dev \
  --scope project \
  --provider openai-compatible \
  --baseURL http://localhost:8000/v1 \
  --model local-model \
  --apiKey "$LOCAL_TEST_KEY"
```

不要在公开仓库中使用项目级配置，除非 `.mossen/` 已被忽略，并且你已经确认没有密钥被 Git 跟踪。
