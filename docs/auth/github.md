# GitHub OAuth Setup

## 1. Create OAuth App in GitHub

1. Go to [GitHub → Settings → Developer settings → OAuth Apps](https://github.com/settings/developers)
2. Click **New OAuth App**
3. Application name: `Rungu Feedback Board`
4. Homepage URL: `https://your-domain.com`
5. Authorization callback URL: `https://your-domain.com/auth/github/callback`

## 2. Get Credentials

After creation, you'll get:
- **Client ID**: `Ov23xxxx`
- **Client Secret**: Generate one from the app settings

## 3. Configure Rungu

```env
APP_URL=https://your-domain.com
GITHUB_CLIENT_ID=Ov23xxxx
GITHUB_CLIENT_SECRET=your-github-secret
```

## Redirect URI

Default: `{APP_URL}/auth/github/callback`

There is no `GITHUB_REDIRECT_URI` override — the redirect URI is always derived from `APP_URL`.
