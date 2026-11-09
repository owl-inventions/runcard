# runcard

runcard turns a GPX track into a poster-style PNG card. Every kilometre
becomes one coloured stripe, from deep blue for the slowest to orange for the
fastest. The card also shows the event, the runner's name, the route outline
and the distance, time and average pace. The GPX file needs timestamps, since
the paces are worked out from them.

<img src="examples/sample.png" alt="Sample card rendered from examples/sample.gpx" width="360">

## Usage

```
runcard <INPUT.gpx> --runner <NAME> [--event <NAME>] [--city <CITY>] [--date <YYYY-MM-DD>] [-o <OUT.png>] [--scale <F>] [--variant <NAME>] [--svg <OUT.svg>] [--timing]
```

| Flag | Meaning |
| --- | --- |
| `--runner <NAME>` | Runner's name, printed in capitals. Required. |
| `--event <NAME>` | Event title. Default: the GPX track name, else `Run`. |
| `--city <CITY>` | City in the header line (`RUN · BERLIN`). |
| `--date <YYYY-MM-DD>` | Date of the run. Default: the UTC date of the first timestamp, else today. |
| `-o, --output <OUT.png>` | PNG to write. Default: the input path with a `.png` extension. |
| `--scale <F>` | Raster scale over the 720x1000 card, must be above 0. Default: `2` (1440x2000). |
| `--variant <NAME>` | Card look: `classic`, `noir`, `rings`, `route`, `bars` or `mosaic` (`--help` describes each). Default: `classic`. |
| `--svg <OUT.svg>` | Also write the card as SVG. |
| `--timing` | Print the time spent in each stage to stderr. |

On success runcard prints `wrote <OUT.png>` and nothing else (the `--timing`
lines go to stderr). Errors go to stderr with exit code 1; invalid or missing
arguments exit with code 2.

```
runcard examples/sample.gpx --runner "Julia Brandt" --city Berlin
```

## Variants

Six looks for the same run, chosen with `--variant` or the `variant` part.
All six are rendered from `examples/sample.gpx`.

| `classic` | `noir` | `rings` |
| --- | --- | --- |
| <img src="examples/classic.png" alt="classic" width="200"> | <img src="examples/noir.png" alt="noir" width="200"> | <img src="examples/rings.png" alt="rings" width="200"> |
| One stripe per kilometre, coloured by pace. The default. | The classic layout on a dark ground, the stripes glowing. | The kilometres as a ring around the finish time. |

| `route` | `bars` | `mosaic` |
| --- | --- | --- |
| <img src="examples/route.png" alt="route" width="200"> | <img src="examples/bars.png" alt="bars" width="200"> | <img src="examples/mosaic.png" alt="mosaic" width="200"> |
| The route drawn large, each kilometre in its pace colour. | A speed chart: taller bars for faster kilometres, the average dashed. | One tile per kilometre with its pace, like a calendar. |

## Server

`runcard-server` serves the same pipeline over HTTP; the `runcard` CLI is
unchanged.

```
runcard-server [--bind <ADDR>] [--allow-origin <ORIGIN>]... [--max-body-mib <N>]
```

| Flag | Environment | Meaning |
| --- | --- | --- |
| `--bind <ADDR>` | `RUNCARD_BIND` | Address to listen on. Default: `0.0.0.0:8080`. |
| `--allow-origin <ORIGIN>` | `RUNCARD_ALLOW_ORIGIN` | Origin a browser may call the API from, repeatable (comma separated in the environment); `*` allows any. Default: none, and no CORS headers. |
| `--max-body-mib <N>` | `RUNCARD_MAX_BODY_MIB` | Largest request body in MiB. Default: `20`. |

The server logs to stderr, starting with `listening on <ADDR>`. `RUST_LOG`
sets the log filter (default `info`; `RUST_LOG=info,tower_http=debug` adds the
requests). Ctrl-C or SIGTERM stops it once the requests in flight are answered. If it
cannot start, for example because the port is taken, it exits with code 1.

### `POST /cards`

A `multipart/form-data` form with the parts below. Other parts are ignored,
and a blank optional part counts as not given.

| Part | Meaning |
| --- | --- |
| `gpx` | The GPX file. Required. |
| `runner` | Runner's name, printed in capitals. Required. |
| `event` | Event title. Default: the GPX track name, else `Run`. |
| `city` | City in the header line. |
| `date` | Date of the run as `YYYY-MM-DD`. Default: the UTC date of the first timestamp, else today. |
| `scale` | Raster scale over the 720x1000 card, above 0 and at most 8. Default: `2`. |
| `format` | `png` or `svg`. Default: `png`. |
| `variant` | Card look, one of the names `GET /variants` lists. Default: `classic`. |

The answer is the card with status 200, `Content-Type: image/png` (or
`image/svg+xml; charset=utf-8`) and
`Content-Disposition: inline; filename="card.png"` (or `card.svg`). Errors
come as JSON, `{"error":"<message>"}`:

| Status | When |
| --- | --- |
| 400 | `gpx` or `runner` is missing or empty; `date`, `scale`, `format` or `variant` is invalid; the multipart body is malformed; the GPX cannot be parsed or analysed (the message says why, e.g. `track has no timestamps, pace cannot be derived`). |
| 413 | The body is larger than `--max-body-mib`. |
| 415 | The body is not `multipart/form-data`. |
| 500 | Rendering failed. The message is `rendering failed`; the details go to the log. |

```
curl -F gpx=@run.gpx -F runner="Julia Brandt" -F city=Berlin -o card.png http://localhost:8080/cards
```

### `GET /variants`

Answers 200 with the card looks in display order, the default first, each
with one sentence on how it looks:
`[{"name":"classic","description":"..."},{"name":"noir","description":"..."},...]`.

### `GET /health`

Answers 200 with `{"status":"ok"}`.

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

The binaries are `target/release/runcard` (the CLI) and
`target/release/runcard-server` (the HTTP server). The fonts and the SVG
templates are compiled into both, so they run without any other files.
`cargo test` runs the test suite.

### Docker

```
docker build -t runcard-server .
docker run --rm -p 8080:8080 -e RUNCARD_ALLOW_ORIGIN=https://example.com runcard-server
```

The `Dockerfile` builds `runcard-server` in a `rust:1.99-bookworm` stage and
copies the binary into `gcr.io/distroless/cc-debian12:nonroot`, about 12 MB
in all, running as `nonroot` on port 8080. The cargo registry and the target
directory are BuildKit cache mounts, so rebuilding after a code change does
not recompile the dependencies. The server stops cleanly on the SIGTERM that
`docker stop` sends.

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
svg 0.1 ms
png 11.7 ms
total 13.5 ms
```

`layout` includes measuring the names against the bundled fonts. `svg` is
filling the template, `png` rasterising and PNG encoding. Loading the fonts
and the templates before `parse` and writing the files after `png` are not
counted; `total` is the sum of the stages.
