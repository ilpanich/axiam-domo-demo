# Trusting the demo root certificate

Everything in this demo — the portals, the AXIAM console, the broker, the
Device Twin — presents a certificate issued by **one** certificate authority:
the AXIAM Domo Demo organization root. Until your machine trusts that root,
every browser will show a warning, because from its point of view the root is
a stranger.

This document is how you make that stranger known, how you confirm it worked,
and — just as importantly — how you undo it again afterwards.

> **This grants real authority.** A CA you trust can vouch for *any* host name,
> not just this demo's. Only import a root whose fingerprint you have checked
> (below), and remove it when you are done evaluating. Every section here has a
> matching removal section for exactly that reason.

---

## 1. Get the files

```bash
just export-trust
```

That writes three files and prints the fingerprint:

| File | What it is |
|---|---|
| `dist/trust/domo-root.pem` | The root certificate, PEM. This is what every step below imports. |
| `dist/trust/domo-root.der` | The same certificate, DER. Byte-identical content, for tools that insist on DER. |
| `dist/trust/domo-root.sha256` | Its SHA-256 fingerprint, as `openssl` prints it. |

The **private** key is not in `dist/` and never will be. It stays in
`.secrets/pki/root.key`, git-ignored, docker-ignored, mode 0600, and
`just guard-secrets` fails loudly if any of that stops being true.

The root is deliberately **not** served over HTTP or HTTPS for a one-click
download. Distribution is this recipe plus the steps below — nothing else.

---

## 2. Check the fingerprint before you trust anything

There is no fingerprint printed in this document, and that is on purpose:
every installation generates its own root, so a value hardcoded here would be
wrong for yours and would train you to skip the check.

Your installation's fingerprint appears in **three** places, and all three must
agree:

1. `dist/trust/domo-root.sha256`
2. The output of `just export-trust`
3. The demo landing page at `https://{DOMO_HOST}/`

Read it from the certificate yourself and compare:

```bash
openssl x509 -noout -fingerprint -sha256 -in dist/trust/domo-root.pem
cat dist/trust/domo-root.sha256
```

If those two disagree, or if either disagrees with the landing page, **stop**.
Do not import. Something has substituted a different certificate between the
CA and you, which is precisely the attack the fingerprint exists to catch.

You can also read the subject and validity, which should say
`CN=Domo Demo Root CA, O=AXIAM Domo Demo`:

```bash
openssl x509 -noout -subject -dates -in dist/trust/domo-root.pem
```

---

## 3. ArchLinux (the presenting laptop)

### Import

```bash
sudo trust anchor --store dist/trust/domo-root.pem
```

### Confirm

```bash
trust list | grep -A2 "Domo Demo Root"
```

You should see the root listed as a pinned/anchored certificate.

> **Unverified, and worth knowing.** On Arch, NSS's trust module is p11-kit, so
> Chrome and Firefox are *expected* to pick up a `trust anchor --store` root
> automatically. This has not been confirmed on this project's own machine
> (research marks it `[ASSUMED]`). If a browser still warns after this step,
> use its own import path in §5 or §6 — that always works — and please record
> which one was actually needed.

### Remove

```bash
sudo trust anchor --remove dist/trust/domo-root.pem
trust list | grep -c "Domo Demo Root"   # expect 0
```

---

## 4. Debian / Raspberry Pi OS (the Pi, and the simulator PC)

### Import

The `.crt` extension is required — `update-ca-certificates` ignores files that
do not end in `.crt`, even when they contain a perfectly good PEM certificate.

```bash
sudo cp dist/trust/domo-root.pem /usr/local/share/ca-certificates/domo-root.crt
sudo update-ca-certificates
```

### Confirm

The command reports its own result; look for a line reading **`1 added`**:

```
Updating certificates in /etc/ssl/certs...
1 added, 0 removed; done.
```

And directly:

```bash
ls -l /etc/ssl/certs | grep -i domo-root
openssl verify -CAfile /etc/ssl/certs/ca-certificates.crt dist/trust/domo-root.pem
```

### Remove

```bash
sudo rm /usr/local/share/ca-certificates/domo-root.crt
sudo update-ca-certificates --fresh
```

`--fresh` rebuilds the bundle from scratch; without it the removed certificate
can linger in `/etc/ssl/certs/ca-certificates.crt`.

### The simulator PC

The simulator host is Linux x86_64 and needs nothing more than the root, so use
exactly the Debian steps above. Phase 4 adds a `just` recipe that bundles the
per-host service credentials; until then the root is the whole story.

---

## 5. Chromium / Chrome on Linux (the NSS user database)

Chrome on Linux keeps its own certificate database per user, separate from the
OS trust store. This path works whether or not §3 did.

### Import

```bash
certutil -d sql:$HOME/.pki/nssdb -A -t "C,," -n "Domo Demo Root" \
         -i dist/trust/domo-root.pem
```

`-t "C,,"` means "trusted CA for TLS server authentication, and nothing else" —
the narrowest trust flag that makes the demo work. Do not widen it.

If `$HOME/.pki/nssdb` does not exist yet (a profile that has never stored a
certificate), create it first:

```bash
mkdir -p $HOME/.pki/nssdb
certutil -d sql:$HOME/.pki/nssdb -N --empty-password
```

### Confirm

```bash
certutil -d sql:$HOME/.pki/nssdb -L
```

`Domo Demo Root` should appear with trust attributes `C,,`. Then restart
Chrome — it reads the database at startup — and open `https://{DOMO_HOST}/`.

### Remove

```bash
certutil -d sql:$HOME/.pki/nssdb -D -n "Domo Demo Root"
certutil -d sql:$HOME/.pki/nssdb -L | grep -c "Domo Demo Root"   # expect 0
```

---

## 6. Firefox

Firefox keeps its own store too, per profile.

### Import — the click path

1. **Settings → Privacy & Security**
2. Scroll to **Certificates** → **View Certificates…**
3. **Authorities** tab → **Import…**
4. Choose `dist/trust/domo-root.pem`
5. Tick **"Trust this CA to identify websites."** Leave the email and software
   boxes unticked — the demo only needs TLS server identity.
6. **OK**

### Import — the command line

Equivalent, against the profile directory. Find the profile first:

```bash
ls -d ~/.mozilla/firefox/*.default* ~/.mozilla/firefox/*.default-release 2>/dev/null
```

Then, with Firefox **closed** (it holds the database open):

```bash
certutil -d sql:$HOME/.mozilla/firefox/<your-profile> \
         -A -t "C,," -n "Domo Demo Root" -i dist/trust/domo-root.pem
```

### Confirm

Re-open **Authorities** and look for `AXIAM Domo Demo` → `Domo Demo Root CA`,
or:

```bash
certutil -d sql:$HOME/.mozilla/firefox/<your-profile> -L | grep "Domo Demo Root"
```

Then load `https://{DOMO_HOST}/` and click the padlock → **Connection secure**
→ **More information** → **View Certificate**. The chain's anchor should read
`Domo Demo Root CA`.

> **Unverified, and worth knowing.** Some Firefox builds only consult the OS
> trust store when `security.enterprise_roots.enabled` is `true`
> (`about:config`). That setting's behaviour on Linux is `[ASSUMED]` in this
> project's research. If §3 alone did not satisfy Firefox, the import above is
> the reliable path and needs no `about:config` change at all.

### Remove

Settings → Privacy & Security → View Certificates… → **Authorities** → select
`Domo Demo Root CA` → **Delete or Distrust…**

Or:

```bash
certutil -d sql:$HOME/.mozilla/firefox/<your-profile> -D -n "Domo Demo Root"
```

---

## 7. Rotating the root

```bash
just pki-rotate-root
```

**This is the one destructive thing in this document.** It destroys the current
root and generates a new one, which means every machine listed above starts
showing certificate warnings again until it repeats its import step — and its
*removal* step first, or it will be left trusting a root that no longer exists.

The order that works:

1. On every machine: run that machine's **removal** step for the old root.
2. On the demo host: `just pki-rotate-root`, then `just export-trust`.
3. Redistribute `dist/trust/domo-root.pem` and run each machine's **import**
   step again.
4. Re-check the fingerprint (§2) — it *must* have changed. If it has not, the
   rotation did not happen.

This is why rotation is a separate, explicitly named recipe and is **not** part
of `just demo-reset`. A reset wipes the databases and re-issues everything
*beneath* the root, re-importing the same root into AXIAM, so trust you have
already installed keeps working across any number of resets.

---

## 8. If a browser still warns

Work down this list; each step rules something out.

| Symptom | What it means | What to do |
|---|---|---|
| "Certificate authority is invalid or unknown" | The root is not in the store the browser actually reads | Do §5 (Chrome) or §6 (Firefox) explicitly; the OS store alone may not be consulted |
| "Certificate is not valid for this name" | You reached the host by a name that is not in the certificate's SAN list | Use `{DOMO_HOST}` or `axiam.{DOMO_HOST}`; `just verify-pki` prints every name each listener is valid for |
| The fingerprint on the landing page differs from yours | The root was rotated since you imported | Follow §7 |
| Everything looks right but the warning persists | The browser cached the old chain | Fully quit and reopen the browser — both read their certificate database only at startup |

`just verify-pki` asserts the whole chain — extensions, SANs, lifetimes, and a
live TLS handshake against each published listener — and names the listener and
the property whenever something is wrong. Run it before assuming the problem is
on the client side.
