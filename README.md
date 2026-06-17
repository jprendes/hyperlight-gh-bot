# hyperlight-gh-bot

A GitHub App webhook server that automatically posts benchmark results as PR comments on the [hyperlight](https://github.com/hyperlight-dev/hyperlight) repository.

## How it works

1. The hyperlight CI runs benchmarks and uploads `benchmark-report_*` artifacts.
2. GitHub sends a `workflow_job` webhook event when each benchmark job completes.
3. The bot downloads the benchmark report artifacts from the workflow run.
4. It posts (or updates) a combined comment on the associated PR with collapsible sections per platform/hypervisor/CPU.

## Prerequisites

- A registered [GitHub App](https://docs.github.com/en/apps/creating-github-apps) with:
  - **Webhook URL** pointing to this server's `/webhook` endpoint
  - **Webhook secret** configured
  - **Permissions**: `actions: read`, `pull_requests: write`
  - **Events**: `workflow_job`
- The App installed on the target repository
- Rust toolchain (for building locally)

## Configuration

The bot is configured via environment variables:

| Variable | Description |
|----------|-------------|
| `GITHUB_APP_ID` | The numeric App ID |
| `GITHUB_APP_KEY` | The App's private key (PEM format, including `-----BEGIN...` markers) |
| `GITHUB_WEBHOOK_SECRET` | The webhook secret configured in the App settings |
| `GITHUB_INSTALLATION_ID` | The installation ID for the target repository |

## Running locally

```bash
export GITHUB_APP_ID=123456
export GITHUB_APP_KEY="$(cat private-key.pem)"
export GITHUB_WEBHOOK_SECRET="your-secret"
export GITHUB_INSTALLATION_ID=789012

cargo run
```

The server listens on port 8080. For local development, use a tunnel (e.g. `ngrok http 8080`) to expose it to GitHub.

## Building the Docker image

```bash
docker build -t hyperlight-gh-bot .
docker run -p 8080:8080 \
  -e GITHUB_APP_ID=... \
  -e GITHUB_APP_KEY="$(cat private-key.pem)" \
  -e GITHUB_WEBHOOK_SECRET=... \
  -e GITHUB_INSTALLATION_ID=... \
  hyperlight-gh-bot
```

## Deployment

See [DEPLOY.md](DEPLOY.md) for full GitHub App setup and Azure Container Apps deployment instructions.

## Logging

Set the `RUST_LOG` environment variable to control log verbosity:

```bash
RUST_LOG=info cargo run        # default recommended level
RUST_LOG=debug cargo run       # verbose for troubleshooting
```
