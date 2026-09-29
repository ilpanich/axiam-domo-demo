# Setting up the demo

From a machine that has never seen this repository to a printed demo card. Two
reference machines are supported and they run the same compose file, the same
images and the same single command: an **amd64 laptop** and a **Raspberry Pi 5**
(8 GB, 64-bit Raspberry Pi OS). Everything up to "Running on the Raspberry Pi 5"
applies to both; that section covers only what is different about the Pi.

> **This demo is LAN-only and is not hardened for the internet.** It relaxes
> AXIAM's login rate limit to 120 attempts a minute so a presenter can log in
> repeatedly without being throttled mid-demo (`AXIAM__RATE_LIMIT__LOGIN_PER_MIN`
> in `deploy/compose.yml`). That is correct here and wrong anywhere reachable
> from outside your network. Do not copy this compose file to a public host.

**Contents**

1. [Prerequisites](#prerequisites)
2. [Configure the machine](#configure-the-machine)
3. [What preflight checks, and what to do about each refusal](#what-preflight-checks-and-what-to-do-about-each-refusal)
4. [The first run](#the-first-run)
5. [Trusting the organization root](#trusting-the-organization-root-both-machines)
6. [The demo card](#the-demo-card)
7. [Between demo runs](#between-demo-runs)
8. [Verifying the installation](#verifying-the-installation)
9. [Running on the Raspberry Pi 5 (arm64)](#running-on-the-raspberry-pi-5-arm64)
10. [Troubleshooting](#troubleshooting)

---

## Prerequisites

| Need | Why | Check |
|---|---|---|
| Docker Engine with the compose v2 plugin | Every recipe calls `docker compose`. | `docker compose version` |
| `docker buildx` | Multi-architecture image builds (PLAT-02). | `docker buildx version` |
| `just` | The task runner. A single static binary; `cargo install just` or the release tarball. | `just --version` |
| `openssl` | The offline PKI: the organization root and every server certificate. | `openssl version` |
| ~8 GB free disk | Asserted by preflight, which refuses below it. | `df -h /home /` |
| A Rust toolchain | **Only on a machine that builds the images.** The demo machine never compiles anything: `domo-bootstrap` runs as a one-shot container on the compose network (D-35). | `cargo --version` |

Network access is needed exactly once, to pull the pinned AXIAM, PostgreSQL,
RabbitMQ and Caddy images. After that the demo is LAN-only.

Clone the repository, then point git at its own hooks — git never does this
automatically, so a fresh clone has no secrets guard until you do:

```bash
just hooks-install
```

## Configure the machine

Five keys, set by hand, per machine:

```bash
cp .env.example .env
$EDITOR .env
```

`.env.example` explains each one. The two you must get right before the first
run are:

- **`DOMO_LAN_IP`** — this machine's real LAN address (`hostname -I | awk '{print $1}'`),
  not `0.0.0.0`. The server certificates are issued for it.
- **`DOMO_HOST`** — the portal host name, default `domo.local`. The AXIAM console
  is served at `axiam.<DOMO_HOST>`.

Everything else — the auth pepper, the JWT key pair, the PKI encryption key, the
database and broker passwords — is minted once by `just secrets` into
`.secrets/generated.env` and is never regenerated. Do not set those by hand: an
operator-chosen value replaces a minted 256-bit one with a weaker one (D-13), and
`.env.example` deliberately does not offer them.

If `DOMO_HOST` does not resolve on the machines you will present from, publish
it. `just preflight` prints the exact command; `mdns` or `/etc/hosts` both work:

```bash
avahi-publish -a -R domo.local        192.168.1.10
avahi-publish -a -R axiam.domo.local  192.168.1.10
```

## What preflight checks, and what to do about each refusal

`just preflight` runs first inside `just up`, and you can run it alone. **It
reports and refuses; it never fixes.** Nothing is started or changed by a
refusal.

| Refusal | What it means | What to do |
|---|---|---|
| `only NG free on /home, need 8G` | The disk floor. This machine has run out of space mid-session before, and the resulting errors look like compiler bugs rather than a full disk. | Reclaim space. `cargo clean`, `docker builder prune -f`, `docker image prune -f`. Never prune volumes — the sibling AXIAM checkout's volumes hold development data. |
| `docker is not on PATH` / `the Docker daemon is not reachable` | Docker is missing or not running. | Install it, or `systemctl start docker`; add yourself to the `docker` group. |
| `docker compose is not available as a v2 plugin` | The old `docker-compose` script is not enough. | Install the compose v2 plugin. |
| `port 443 / 8090 / 8883 / 15672 is already in use` | A bind that would fail inside `docker compose up`. The refusal names the container holding it. | If it is a previous run of this demo, `just down`. If it names an `axiam-*` container, that is the **sibling AXIAM checkout's** own stack — stop it from *that* repository, and do not `docker stop` it blindly: its volumes are not yours to touch. See Troubleshooting. |
| `<host> does NOT resolve here` | A note, not a refusal. The stack still comes up and `just edge-verify` pins the name to `DOMO_LAN_IP`. | Publish the name if a browser on another machine has to reach the demo. The command is printed. |

Preflight deliberately does **not** check memory or swap. Pi memory behaviour is
validated on real hardware in a later phase, not guessed at here.

## The first run

```bash
just up
```

One command. It prints one line per stage, in this order:

```
preflight  pki  axiam-up  org-bootstrap  tenants  catalog
service-certs  broker  platform-up  verify
```

and ends with `✓ up` and a printed demo card.

If it stops, it names the stage, the cause, and the last forty lines of the
relevant container's log — then tells you to re-run `just up`, which **resumes at
that stage** rather than starting over. A stage marker is only a hint: every
stage re-probes AXIAM for its real state before skipping, so a run that died
between a successful request and writing its marker still converges.

`just checklist` prints where this machine stands, stage by stage, at any time.

## Trusting the organization root (both machines)

Every certificate in the demo chains to one offline root. A browser must trust it
or every page shows a warning.

```bash
just export-trust
```

writes `dist/trust/domo-root.pem`, `.der` and `.sha256`, and prints the per-target
import steps. The full instructions — per OS, per browser, how to confirm it
worked, and how to remove it again — are in [`docs/trust.md`](trust.md).

**Compare the fingerprint before you trust it.** `just export-trust` prints it,
`dist/trust/domo-root.sha256` holds it, the demo card carries it and the portal's
front door shows it. All four must agree; if they do not, stop.

The root is generated once per machine and is **never rotated by a reset** (D-10),
so a machine you have trusted stays trusted across every `just demo-reset`. Only
`just pki-rotate-root` changes it, it asks for confirmation, and it appears in no
automated path.

## The demo card

The last thing a successful run produces. It is printed to your terminal and
written to `.secrets/demo-card.txt` at owner-only mode, because the terminal it
was printed to is usually not the one open when the demo starts — and because it
carries the super-admin login.

It holds the portal and console URLs, the super-admin email and password, the
root fingerprint, the trust-export hint, and the next steps. `just demo-card`
reprints it at any time.

## Between demo runs

```bash
just demo-reset
```

Wipes this project's state and rebuilds it — every named volume, one by one and
by explicit name, plus every stage marker and all the certificate material issued
beneath the root — then runs `just up` again. Running it twice in a row is safe;
the second run reports every stage as done rather than erroring.

Two things it deliberately does **not** do:

- **It never prunes.** A blanket volume prune would also destroy the sibling AXIAM
  checkout's development volumes, which are not ours to destroy (D-09). `just
  verify` asserts that no prune appears anywhere in the orchestration.
- **It never touches the organization root** (D-10). No browser has to be
  re-trusted between demo runs.

## Verifying the installation

```bash
just verify        # the fast gate — minutes
just phase-verify  # the phase gate: reset → verify → smoke → reset → smoke
```

`just verify` prints one line per group and maps them to the phase's own four
success criteria, so you can check a run against the definition of done rather
than against a list of commands.

**`just smoke` is red on purpose, and `just phase-verify` therefore exits
non-zero.** Two of twelve device-connect cases fail against the pinned AXIAM
build, both confirmed defects in AXIAM rather than in this demo:

| Case | Finding | What it means |
|---|---|---|
| `cross-tenant-ca-issuance` | DF-017 | AXIAM signs a leaf under *another* tenant's signing CA at this tenant's request |
| `other-tenant-ca` | DF-025 | that forged leaf then connects, publishes and subscribes, end to end |

Together: **certificate issuance is not a tenant boundary on the pinned build.**

AXIAM 1.0.0-beta17 fixes DF-017 upstream as T22.1, and DF-025 had no cause of its
own beyond it — but **none of that is verified here**, because the beta17
container images were never published. The demo stays pinned at beta16, server
and SDK together. `just smoke-gate` prints the whole explanation and distinguishes
these two from any third failure, which would be news;
[`docs/dogfooding-upstream-status.md`](dogfooding-upstream-status.md) lists exactly
what to re-run the day the images exist.

Do not make this gate green by pinning the behaviour, excluding the cases or
weakening an assertion. A negative case that is made to pass tests nothing.

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

---

## Troubleshooting

Every entry below is something this phase actually hit. They are ordered by how
likely you are to meet them on a first run.

### `docker compose up` dies with a bind error on port 15672

Something else already publishes RabbitMQ's management port. On a developer
machine the likeliest culprit is the **sibling AXIAM checkout's own development
stack** (`axiam-rabbitmq`), which publishes `15672` on `0.0.0.0` — which occupies
this demo's loopback bind too.

```bash
ss -ltnp | grep 15672
docker ps --format '{{.Names}}\t{{.Ports}}' | grep 15672
```

Stop that stack **from its own repository**. Do not `docker stop` it blindly and
never prune its volumes: they hold development data that is not this demo's to
destroy.

`just preflight` catches this before anything starts and names the container
holding the port. It did not always — the port list covered only the three
LAN-facing ports, and a loopback-only publication is still a publication.

### A command fails with `unrecognized subcommand`, and the source clearly has it

The container image is older than the source it was built from, so it is running
a previous binary. clap's error reads exactly like a source bug and cost this
phase an hour.

```bash
just build          # rebuild both images
```

`just up` now checks image-vs-source freshness itself (`_images-fresh`) and
rebuilds when the image is older than anything under `crates/`, `tools/`,
`services/`, `Cargo.toml`, `Cargo.lock` or the Dockerfile. If you invoke a `just`
recipe that runs a tool directly, without going through `just up`, rebuild first.

### The organization bootstrap fails and the output says `reset required`

AXIAM emits its one-time setup token **once per SurrealDB volume** (DF-019). If
the token was consumed by a previous run and the saved copy in
`.secrets/state/setup-token` is gone, that volume can no longer be bootstrapped
by anything.

```bash
just demo-reset     # wipes the volume; a fresh token is minted
```

There is no recovery short of that on the pinned build. AXIAM 1.0.0-beta17 adds
`axiam-server setup-token --remint` (T22.7) — unverified here, see
[`docs/dogfooding-upstream-status.md`](dogfooding-upstream-status.md).

### The AXIAM container is up but reports unhealthy, or has no health status

The image's own `healthcheck` subcommand **cannot probe a TLS listener behind a
private certificate authority** (DF-016): it speaks plain HTTP by default, and
over HTTPS it trusts only webpki roots with no way to point it at a custom trust
anchor. Plan 01-01 dropped the compose health check for this reason; readiness is
a polling loop in the task runner instead (`just _wait-axiam`), which probes the
TLS listener with the demo's own root.

An unhealthy or absent health status on `axiam-server` is therefore not a
symptom. Judge it by `curl --cacert .secrets/pki/root.pem https://127.0.0.1:8090/health`.

### Signing a certificate fails with a decryption error after changing a secret

AXIAM's PKI encryption key has **two spellings, and only one is honoured**.
`AXIAM__AUTH__PKI_ENCRYPTION_KEY` is the one that works on `1.0.0-beta16`;
`AXIAM__PKI__ENCRYPTION_KEY` is ignored entirely (P-9, DF-018). `just secrets`
sets both to the same value so a future image that switches spellings does not
break — if you ever edit one by hand, edit both.

More generally: the key is applied when the tenant CA's private key is
*encrypted*, so changing it after provisioning makes every existing CA
undecryptable. `just demo-reset` is the way out.

### A service starts but cannot read its key, or fails closed with a confusing reason

A file written at `0600` under the host uid is unreadable to a container running
as a different user, and `.secrets/` is `0700` because it holds the root key. The
Device Twin runs as uid 65532; when the tenant map was written into
`.secrets/state` under the host uid, every tenant lookup failed closed with a
message that pointed at the tenant rather than at the file mode.

Material a container must read is published into per-service named volumes by the
`tls-init` service, with the ownership that service needs — Compose ignores a
`secrets:` block's uid/mode outside swarm mode, which is why it is done that way.
If you add a file a container reads, route it through `tls-init`, not through
`.secrets/` directly. `just _dc exec <service> ls -l <path>` tells you what the
container actually sees.

### `just verify-pki` reports a listener as "no port answered a TLS handshake"

Check whether the row is `caddy`. The edge runs with `auto_https off` and only
host-keyed site blocks, so it has **no default site** and correctly refuses a
handshake carrying no SNI. The probe sends `-servername <first DNS SAN>` for
exactly this reason; the message names the SNI it used.

For the same reason the `caddy` row carries **names only and no IP SAN**: a client
reaching the edge by IP literal sends no SNI, so an IP SAN would be a name that
listener can never present. Reach the demo by name. If a later phase wants an IP
URL to work, that is a catch-all site block in the Caddyfile first, and the SAN
back afterwards — in that order.

### `just authz` fails saying a tenant does not exist

`just authz` deliberately does **not** create tenants. It depends on
`tenant-admins`, `catalog-apply-all`, `tree` and `service-certs`, all of which are
tenant-scoped; the `tenants` and `pki` stages that create the tenants and their
signing CAs run inside `just up`.

On a half-provisioned instance the order matters and is recoverable by hand:

```bash
just _stage tenants
just _stage pki
just authz
```

`just checklist` shows which stages this machine has actually completed. In the
normal case just run `just up`, which does all of it in the right order and
resumes where it stopped.

### The portal or console is unreachable from another machine

Three separate causes, in the order worth checking:

1. **The name does not resolve there.** `getent hosts domo.local` on the *client*
   machine. Publish it with `avahi-publish`, or add it to that machine's
   `/etc/hosts`. `just preflight` prints the command.
2. **The client does not trust the root.** Every page shows a certificate
   warning. Follow [`docs/trust.md`](trust.md) on that machine and compare the
   fingerprint against the demo card.
3. **`DOMO_LAN_IP` is stale.** Changing networks changes the address, and the
   certificates were issued for the old one. Update `.env` and re-run
   `just demo-reset` — the root survives, so trusted clients stay trusted.

### The smoke suite is red

Expected, if and only if the failures are `cross-tenant-ca-issuance` and
`other-tenant-ca`. Run `just smoke-gate`: it attributes those two to DF-017 and
DF-025, states that AXIAM beta17 fixes the root cause as T22.1 and that the fix is
**not verified here**, and calls out any *third* failure as a regression in its own
right. See [Verifying the installation](#verifying-the-installation) above.

---

## Notes for the phases that come next

Small facts that are expensive to rediscover.

- **`smoke-` is a reserved fixture prefix.** Applied to the *slug*, after the type
  prefix: `site:smoke-park`, `apartment:smoke-tower-a1`, service accounts
  `smoke-probe-device` and friends, the user `smoke-resident`. `just
  smoke-teardown` deletes by that prefix alone, so Phase 2's seed data must never
  use it.
- **`AXIAM__RATE_LIMIT__TRUSTED_HOPS` stays `0`**, and that is deliberate rather
  than an oversight. AXIAM's listener on `:8090` is reached *both* through Caddy
  *and* directly from the LAN by devices (D-05), so any non-zero value would let a
  LAN device forge `X-Forwarded-For`. Do not raise it.
- **Service accounts are for authorization checks and device-style authentication
  only.** The Management Platform needs each tenant's admin *user*, not the
  `mgmt@<slug>` service account. (AXIAM beta17's T22.13 may change this — DF-013,
  unverified here.)
- **T-06-04, from plan 01-06's threat register, is recorded as `mitigate` and that
  disposition is wrong.** It reads "attempted explicitly; a refusal passes, an
  acceptance fails the matrix" — the acceptance happened, and the compensating
  control it assumed (`other-tenant-ca` being refused) does not hold. It is an
  **accepted-and-demonstrated** risk, not a mitigated one. Plan 01-06's completed
  plan is not rewritten; the correction is recorded here, where the next security
  review will meet it.
