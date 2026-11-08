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

fetch "$base/ibmplexmono/IBMPlexMono-Medium.ttf" IBMPlexMono-Medium.ttf
fetch "$base/fraunces/Fraunces%5BSOFT%2CWONK%2Copsz%2Cwght%5D.ttf" Fraunces-Variable.ttf
fetch "$base/bebasneue/BebasNeue-Regular.ttf" BebasNeue-Regular.ttf
fetch "$base/ibmplexmono/OFL.txt" OFL-IBMPlexMono.txt
fetch "$base/fraunces/OFL.txt" OFL-Fraunces.txt
fetch "$base/bebasneue/OFL.txt" OFL-BebasNeue.txt
