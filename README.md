# runcard

runcard turns a GPX track into a poster-style PNG card. Every kilometre
becomes one coloured stripe, from deep blue for the slowest to orange for the
fastest. The card also shows the event, the runner's name, the route outline
and the distance, time and average pace. The GPX file needs timestamps, since
the paces are worked out from them.

<img src="examples/sample.png" alt="Sample card rendered from examples/sample.gpx" width="360">

## Usage

```
runcard <INPUT.gpx> --runner <NAME> [--event <NAME>] [--city <CITY>] [--date <YYYY-MM-DD>] [-o <OUT.png>] [--scale <F>] [--svg <OUT.svg>] [--timing]
```

| Flag | Meaning |
| --- | --- |
| `--runner <NAME>` | Runner's name, printed in capitals. Required. |
| `--event <NAME>` | Event title. Default: the GPX track name, else `Run`. |
| `--city <CITY>` | City in the header line (`RUN · BERLIN`). |
| `--date <YYYY-MM-DD>` | Date of the run. Default: the UTC date of the first timestamp, else today. |
| `-o, --output <OUT.png>` | PNG to write. Default: the input path with a `.png` extension. |
| `--scale <F>` | Raster scale over the 720x1000 card, must be above 0. Default: `2` (1440x2000). |
| `--svg <OUT.svg>` | Also write the card as SVG. |
| `--timing` | Print the time spent in each stage to stderr. |

On success runcard prints `wrote <OUT.png>` and nothing else (the `--timing`
lines go to stderr). Errors go to stderr with exit code 1; invalid or missing
arguments exit with code 2.

```
runcard examples/sample.gpx --runner "Julia Brandt" --city Berlin
```

## Limits

A long runner's name or event title is set smaller until it fits (the name
down to 40 px, the title down to 28 px) and cut with an ellipsis if it does
not fit even then; a long city is cut so the header does not run into the
date. Every track point needs a timestamp, and no timestamp may be earlier
than the one before it. Runs of one metre or less are rejected.

## Build

```
cargo build --release
```

The binary is `target/release/runcard`. The fonts and the SVG template are
compiled into it, so it runs without any other files. `cargo test` runs the
test suite.

## Fonts

The card uses IBM Plex Mono, Fraunces and Bebas Neue, all under the SIL Open
Font License 1.1. The font files and their licence texts are in `fonts/`, and
`fonts/fetch.sh` downloads them again from the google/fonts repository.

## Timing

Rendering `examples/sample.gpx` (a marathon, 423 points) at the default scale
with `--timing`, release build, second of two consecutive runs on an Apple M4
Pro:

```
parse 1.2 ms
analyze 0.0 ms
layout 0.5 ms
svg 0.3 ms
png 13.6 ms
total 15.6 ms
```

`layout` includes measuring the names against the bundled fonts. `svg`
includes loading the fonts for rendering and filling the template. `png`
covers rasterising, PNG encoding and writing the file.
