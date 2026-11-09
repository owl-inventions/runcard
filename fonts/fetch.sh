#!/usr/bin/env bash
# Downloads the card fonts (SIL Open Font License) from the google/fonts repository.
set -euo pipefail
cd "$(dirname "$0")"

base="https://raw.githubusercontent.com/google/fonts/main/ofl"

fetch() {
  local url="$1" out="$2"
  echo "fetching $out"
  curl -fsSL "$url" -o "$out"
}

fetch "$base/archivo/Archivo%5Bwdth%2Cwght%5D.ttf" Archivo-Variable.ttf
fetch "$base/jetbrainsmono/JetBrainsMono%5Bwght%5D.ttf" JetBrainsMono-Variable.ttf
fetch "$base/archivo/OFL.txt" OFL-Archivo.txt
fetch "$base/jetbrainsmono/OFL.txt" OFL-JetBrainsMono.txt
