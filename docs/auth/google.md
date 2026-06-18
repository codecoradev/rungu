# Google OAuth Setup

## 1. Create OAuth Client in Google Console

1. Go to [Google Cloud Console → Credentials](https://console.cloud.google.com/apis/credentials)
2. Click **Create Credentials → OAuth client ID**
3. Application type: **Web application**
4. Name: `Rungu Feedback Board`
5. Authorized redirect URI: `https://your-domain.com/auth/google/callback`

## 2. Get Credentials

After creation, you'll get:
- **Client ID**: `xxxx.apps.googleusercontent.com`
- **Client Secret**: `GOCSPX-xxxx`

## 3. Configure Rungu

```env
APP_URL=https://your-domain.com
GOOGLE_CLIENT_ID=xxxx.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=GOCSPX-xxxx
```

## Redirect URI

Default: `{APP_URL}/auth/google/callback`

There is no `GOOGLE_REDIRECT_URI` override — the redirect URI is always derived from `APP_URL`.
