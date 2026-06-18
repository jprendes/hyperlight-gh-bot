# Deployment Guide

## GitHub App setup

### 1. Create the App

Go to **GitHub → Settings → Developer settings → GitHub Apps → New GitHub App** and configure:

| Field | Value |
|-------|-------|
| App name | `hyperlight-benchmark-bot` (or similar) |
| Homepage URL | Repository URL or deployment URL |
| Webhook URL | `https://example.com` (placeholder — you'll update this after deployment) |
| Webhook secret | A random string (save it — you'll need it for deployment) |

### 2. Permissions

Under **Repository permissions**, grant:

| Permission | Access |
|------------|--------|
| Actions | Read |
| Pull requests | Read & Write |
| Metadata | Read (auto-granted) |

### 3. Events

Subscribe to:

- [x] Workflow job

### 4. Generate a private key

After creating the App, click **Generate a private key**. Save the `.pem` file.

### 5. Install the App

Go to **Install App** in the sidebar and install it on the `hyperlight-dev/hyperlight` repository (or the target org/repo).

### 6. Note the App ID

- **App ID**: shown on the App's General page

## Azure Container Apps deployment

### Prerequisites

```bash
az login
az extension add --name containerapp --upgrade
az provider register -n Microsoft.App --wait
az provider register -n Microsoft.OperationalInsights --wait
```

### Set variables

```bash
RESOURCE_GROUP="hyperlight-gh-bot-rg"
LOCATION="eastus"
GHCR_IMAGE="ghcr.io/<owner>/hyperlight-gh-bot:latest"
ENVIRONMENT="hyperlight-gh-bot-env"
APP_NAME="hyperlight-gh-bot"
KEY_VAULT="hyperlight-gh-bot-kv"
```

### Create resource group

```bash
az group create --name $RESOURCE_GROUP --location $LOCATION
```

### Build and push the image

The container image is built and pushed to GHCR automatically by the **Publish image to GHCR** GitHub Actions workflow on every push to `main`.

You can also trigger it manually from the Actions tab.

### Create Key Vault and store secrets

```bash
az keyvault create \
  --resource-group $RESOURCE_GROUP \
  --name $KEY_VAULT \
  --location $LOCATION \
  --enable-rbac-authorization false

az keyvault secret set --vault-name $KEY_VAULT \
  --name github-app-key \
  --file private-key.pem

az keyvault secret set --vault-name $KEY_VAULT \
  --name github-webhook-secret \
  --value "your-webhook-secret"
```

### Create Container Apps environment

```bash
az containerapp env create \
  --resource-group $RESOURCE_GROUP \
  --name $ENVIRONMENT \
  --location $LOCATION
```

### Deploy the Container App

```bash
APP_KEY=$(az keyvault secret show --vault-name $KEY_VAULT --name github-app-key --query value -o tsv)
WEBHOOK_SECRET=$(az keyvault secret show --vault-name $KEY_VAULT --name github-webhook-secret --query value -o tsv)

az containerapp create \
  --resource-group $RESOURCE_GROUP \
  --name $APP_NAME \
  --environment $ENVIRONMENT \
  --image "$GHCR_IMAGE" \
  --target-port 8080 \
  --ingress external \
  --min-replicas 0 \
  --max-replicas 1 \
  --secrets \
    github-app-key="$APP_KEY" \
    github-webhook-secret="$WEBHOOK_SECRET" \
  --env-vars \
    GITHUB_APP_ID=<your-app-id> \
    GITHUB_APP_KEY=secretref:github-app-key \
    GITHUB_WEBHOOK_SECRET=secretref:github-webhook-secret \
    RUST_LOG=info
```

### Get the ingress URL and update the GitHub App

```bash
az containerapp show \
  --resource-group $RESOURCE_GROUP \
  --name $APP_NAME \
  --query "properties.configuration.ingress.fqdn" -o tsv
```

Go back to your GitHub App settings and update the **Webhook URL** to `https://<fqdn>/webhook`.

### Update the deployment

After the GitHub Actions workflow pushes a new image:

```bash
az containerapp update \
  --resource-group $RESOURCE_GROUP \
  --name $APP_NAME \
  --image "$GHCR_IMAGE"
```

### View logs

```bash
az containerapp logs show \
  --resource-group $RESOURCE_GROUP \
  --name $APP_NAME \
  --type console \
  --follow
```
