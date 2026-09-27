# Traced website

The landing page: what Traced does, a phone playing the app's Live screen on a simulated lap of
Mondercange, and the APK download. A plain client-side page, no server.

```bash
dx serve --package web --platform web
dx bundle --package web --platform web   # static files to host anywhere
```

The download buttons point at `releases/latest/download/traced.apk` on GitHub, so each release
needs its APK attached under that name.
