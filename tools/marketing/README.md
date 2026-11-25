# Marketing captures

Makes the screenshots and animated GIFs used by the website (`site/`) and the wiki.

The real xpando front end runs in a headless browser against a made-up phrase library
(`mock.js` stands in for the Tauri backend), so nothing touches your own phrases, your
running xpando, or your desktop. `expand.html` is a staged illustration of an expansion.

```powershell
# 1. start the front end (separate terminal)
cd xpando-app
npx vite --port 1421 --strictPort

# 2. record raw captures (needs Node 22+ and Chrome or Edge; no npm packages)
cd ..\tools\marketing
node scenes.mjs stills        # all screenshots, light and dark
node scenes.mjs gifs          # all animations
node scenes.mjs gif:create    # one scene (still:library, gif:expand, ...)

# 3. frame them and write the final files (needs Python with Pillow)
python build.py               # site/assets/img, site/assets/gif and out/wiki
```

`out/` holds raw frames and the wiki-ready copies and is not committed. To change what a
screenshot shows, edit the demo data in `mock.js` or the steps in `scenes.mjs`.
