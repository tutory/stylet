#!/bin/sh
# Assembles the GitHub Pages site into _site: the landing page at /, the
# playground at /playground/.
set -e
cd "$(dirname "$0")/.."
playground/build.sh
rm -rf _site
mkdir -p _site/playground
cargo run --release -q -p stylet-cli -- build site/site.styl \
  -o _site/site.css --minify --resolve-custom-media --source-map
cp site/index.html site/site.js _site/
cp -R playground/index.html playground/playground.css playground/playground.css.map playground/playground.js playground/stylet-hljs.js playground/pkg _site/playground/
echo "Built _site. Serve with: python3 -m http.server -d _site"
