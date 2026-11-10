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

fetch "$base/instrumentserif/InstrumentSerif-Regular.ttf" InstrumentSerif-Regular.ttf
fetch "$base/instrumentserif/InstrumentSerif-Italic.ttf" InstrumentSerif-Italic.ttf
fetch "$base/bricolagegrotesque/BricolageGrotesque%5Bopsz%2Cwdth%2Cwght%5D.ttf" BricolageGrotesque-Variable.ttf
fetch "$base/jetbrainsmono/JetBrainsMono%5Bwght%5D.ttf" JetBrainsMono-Variable.ttf
fetch "$base/instrumentserif/OFL.txt" OFL-InstrumentSerif.txt
fetch "$base/bricolagegrotesque/OFL.txt" OFL-BricolageGrotesque.txt
fetch "$base/jetbrainsmono/OFL.txt" OFL-JetBrainsMono.txt
