# Setting up the demo

<!-- Plan 01-07 Task 2 writes the Raspberry Pi section; Task 3 completes the
     rest of this guide. -->

## Running on the Raspberry Pi 5 (arm64)

The demo has two reference machines — an amd64 laptop and a Raspberry Pi 5 (8 GB,
64-bit Raspberry Pi OS) — and they run **the same compose file, the same images
and the same single command** (PLAT-02, D-06). The only thing that differs
between them is the generated per-host environment file. There is no Pi override
file, and there must never be one: the moment the two machines run different
topologies, the Pi run stops proving anything about the laptop run.

### Prerequisites on the Pi

| Need | Why | Check |
|---|---|---|
| 64-bit Raspberry Pi OS | The images are `linux/arm64`. A 32-bit OS cannot run them. | `uname -m` → `aarch64` |
| Docker Engine with the compose v2 plugin | Every recipe calls `docker compose`. | `docker compose version` |
| `just` | The task runner. A single static binary; `cargo install just` or the release tarball. | `just --version` |
| `openssl` | The offline PKI. | `openssl version` |
| ~8 GB free disk | Asserted by preflight, and it refuses below it. | `df -h /` |
| **No Rust toolchain** | Deliberate. The bootstrap runs as a one-shot container on the compose network (D-35), so the Pi never compiles anything — *unless* you take the native build route below. | — |

`just preflight` checks every one of these and prints the version it found. Run
it first; it refuses rather than failing halfway.

### The per-host environment file

`.env` holds the five keys an operator sets by hand. It is **not** committed —
it is per machine, and `DOMO_LAN_IP` in particular is different on every host.

Either copy `.env.example` and fill it in on the Pi:

```bash
cp .env.example .env
$EDITOR .env          # DOMO_LAN_IP must be the Pi's own LAN address
```

…or copy the laptop's `.env` across and change `DOMO_LAN_IP`. Do **not** copy
`.secrets/generated.env`: those values are minted per machine and are applied
only on the first boot of an empty data volume.

`hostname -I | awk '{print $1}'` prints the address to use.

### Getting the arm64 images onto the Pi

Two routes. The first is fast; the second needs nothing but the Pi.

**Route A — build on the laptop, load on the Pi (recommended).**
`just images` cross-compiles both images for both architectures on the amd64
machine and writes a transferable archive per image:

```bash
# on the laptop
just images
just images-verify              # asserts every image in the stack resolves arm64
scp dist/images/domo-*-arm64.tar pi@raspberrypi.local:~/
```

```bash
# on the Pi
docker load -i domo-twin-arm64.tar
docker load -i domo-tools-arm64.tar
docker tag domo-twin:dev-arm64  domo-twin:dev
docker tag domo-tools:dev-arm64 domo-tools:dev
```

The tag step matters: the compose file references `domo-twin:dev` and
`domo-tools:dev` on both machines, because there is only one compose file.

**Route B — build natively on the Pi.** `just up` builds the two images itself
when they are absent or older than the source, so this route needs no extra
step at all — just run `just up` and wait. It is considerably slower (the Pi
compiles the whole Rust workspace), but it needs no second machine, and
`deploy/docker/Dockerfile.rust` compiles natively there with no cross toolchain
involved: on the Pi the build platform and the target platform are the same.

This is the documented fallback if the cross-build route ever stops working. It
is not needed today — the cross route is the one this phase actually used, and
it produced a genuine aarch64 binary in 2–3 minutes per image.

### Trusting the organization root

Every certificate in the demo chains to one offline root, and the Pi's browser
(and the Pi itself, for `curl`) must trust it. Full per-target steps, including
how to confirm it worked and how to remove it again, are in
[`docs/trust.md`](trust.md). The Raspberry Pi OS step is:

```bash
just export-trust                                   # writes dist/trust/
sudo cp dist/trust/domo-root.pem /usr/local/share/ca-certificates/domo-root.crt
sudo update-ca-certificates
```

**Compare the fingerprint before you trust it.** `just export-trust` prints it,
`dist/trust/domo-root.sha256` holds it, and the demo's own front door shows it.
All three must agree; if they do not, stop.

If the Pi generates its own root (it will, on a fresh checkout — the root is
generated once per machine and never rotated by a reset, D-10), that root is
*different* from the laptop's. A client that has trusted one will not trust the
other. For a demo driven from the Pi, trust the Pi's root.

### First run

```bash
just up
```

One command. It prints one line per stage and ends in `✓ up`, and it writes a
demo card with everything a presenter needs. If it stops, it names the stage,
the cause and the last lines of the relevant container's log, and re-running it
resumes at that stage rather than starting over.

### Confirming the Pi really pulled arm64

A wrong-architecture image fails at run time on the Pi, not at build time on the
laptop — so check it explicitly:

```bash
for i in $(docker compose -f deploy/compose.yml config --images | sort -u); do
  printf '%-50s %s\n' "$i" "$(docker image inspect --format '{{.Architecture}}' "$i" 2>/dev/null)"
done
```

Every line must read `arm64`. A line reading `amd64` is an image that was
copied across rather than pulled or built for this machine; remove it and let
`just up` fetch or build it again.

The stronger check — that the *binary inside* our own images is really
`aarch64`, not merely that the image config claims to be — is `just
images-verify`, on the build machine. It exists because an image whose config
said `arm64` around an x86-64 binary is exactly the defect this phase hit and
fixed: it looks correct on the laptop and fails only on the Pi.
