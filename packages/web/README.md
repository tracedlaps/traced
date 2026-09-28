# Traced website

The landing page: what Traced does, a phone playing the app's Live screen on a simulated lap of
Mondercange, and the beta APK download. A plain client-side page, no server.

```bash
dx serve --package web --platform web
dx bundle --package web --platform web   # static files to host anywhere
```

Published with `scripts/publish-web.sh` to the public `tracedlaps/tracedlaps` repo, which GitHub
Pages serves at tracedlaps.com. The download button points at that repo's
`releases/latest/download/traced.apk`, so each release needs its APK attached under that name.
