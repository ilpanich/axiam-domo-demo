//! The connect matrix, declared as data (D-25).
//!
//! # Why data and not a script
//!
//! Phase 3 reuses this harness as the 114-connection load driver, so the cases
//! have to be a list something else can iterate rather than a sequence of
//! hand-written steps. It also makes the matrix's own shape reviewable: the
//! table below *is* the claim this plan makes about the device identity chain,
//! and a case that is missing is visible as a missing row.
//!
//! # Expected outcomes are specific on purpose
//!
//! Not "fails" but *which layer refuses, and with what signal*. Those two are
//! genuinely different facts:
//!
//! - [`Expect::HandshakeFailed`] versus [`Expect::AuthorizationDenied`] is the
//!   difference between the broker's peer-certificate requirement being in
//!   force and it being off entirely. A harness that only checked "connect
//!   failed" would pass either way, which is the whole of T-06-02.
//! - [`Expect::BrokerRejectedIdentity`] says the broker's
//!   `ssl_cert_client_id_from = distinguished_name` check fired *before* the
//!   Twin was ever consulted. If that stopped being true, the case would still
//!   fail — just differently — and the change would be visible rather than
//!   absorbed.
//!
//! # Never weaken a case to make it pass
//!
//! If a refusal does not happen, the answer is a finding and a stop. Relaxing
//! the broker configuration, the certificate or a check in the Twin to get a
//! green run defeats the only reason this file exists.

/// The probe itself: a device of the Lakeside tenant, under the smoke
/// apartment's device resource.
pub const PROBE: &str = "smoke-probe-device";
/// A second Lakeside device in the same apartment.
pub const PEER: &str = "smoke-peer-device";
/// A Lakeside account whose certificate was issued but deliberately never
/// bound — "authenticates as nobody", made concrete.
pub const UNBOUND: &str = "smoke-unbound-device";
/// A device of the OTHER tenant, with a certificate from that tenant's CA.
pub const OTHER: &str = "smoke-summit-device";

/// Which certificate and key the attempt presents at the TLS handshake.
#[derive(Debug, Clone, Copy)]
pub enum Credential {
    /// The probe's own leaf, chained to its tenant's signing CA.
    Own,
    /// Another fixture's leaf — the certificate half of the identity swapped
    /// out while the credential half stays the probe's.
    CertOf(&'static str),
    /// A leaf carrying the PROBE's own common name, signed by the OTHER
    /// tenant's authority.
    ///
    /// Only producible because DF-017 is real; when AXIAM refuses to mint it,
    /// the case falls back to the other tenant's own device certificate, which
    /// tests the same boundary less sharply. See [`Expect::IssuanceRefused`].
    ForgedCommonName,
    /// No client certificate at all.
    None,
}

/// What rides in the CONNECT password field.
#[derive(Debug, Clone, Copy)]
pub enum Password {
    /// The token the probe's own mTLS device login returned.
    OwnToken,
    /// A string that is not a token at all.
    Malformed,
    /// The probe's own token with its signature segment corrupted: the shape
    /// stays valid, the signature cannot verify.
    CorruptSignature,
    /// A valid token belonging to another fixture — and, for [`OTHER`], to
    /// another tenant.
    TokenOf(&'static str),
}

/// Where the attempt publishes once it is connected.
#[derive(Debug, Clone, Copy)]
pub enum Target {
    /// The probe's own namespace: subscribe, publish, expect the round trip.
    Own,
    /// A sibling device's namespace in the same tenant.
    Sibling,
    /// The probe's own identifier, in the other tenant's namespace.
    OtherTenant,
    /// A namespace whose name *extends* the probe's own. A prefix comparison
    /// in the Twin would pass this one; a separator-aware one does not.
    Adjacent,
}

/// The outcome a case asserts, naming the layer that must refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// CONNACK accepted, subscribe acknowledged, publish acknowledged, and the
    /// message came back.
    Connected,
    /// Refused at the TLS handshake, before any CONNACK exists — which is what
    /// `fail_if_no_peer_cert` being genuinely in force looks like.
    HandshakeFailed,
    /// Refused by the broker at CONNECT because the client identifier is not
    /// the certificate's subject. Fires before the Twin is consulted.
    BrokerRejectedIdentity,
    /// Refused by the authorization backend: a CONNACK carrying a refusal.
    AuthorizationDenied,
    /// Connected, then refused when it published outside its own namespace.
    PublishRefused,
    /// The REST device login refused the certificate: 401 or 403.
    ///
    /// 401 was the predicted status. AXIAM answers **403** for a certificate
    /// that is valid TLS material bound to no account — recorded rather than
    /// silently substituted. A 200 still fails the case.
    LoginUnauthorized,
    /// AXIAM refused to sign a request under another tenant's signing CA.
    ///
    /// The one case whose answer was genuinely unknown from reading the code
    /// (P-11). A refusal closes DF-017; an acceptance confirms it, and must
    /// fail this matrix rather than be absorbed into a green log.
    IssuanceRefused,
}

impl Expect {
    #[must_use]
    pub fn describe(self) -> &'static str {
        match self {
            Self::Connected => "connect, subscribe, publish and a round trip",
            Self::HandshakeFailed => "a TLS handshake failure, before any CONNACK",
            Self::BrokerRejectedIdentity => "a CONNACK refusing the client identifier",
            Self::AuthorizationDenied => "a CONNACK refusing the credentials",
            Self::PublishRefused => "the publish refused after a successful connect",
            Self::LoginUnauthorized => "the device login to refuse (HTTP 401 or 403)",
            Self::IssuanceRefused => {
                "AXIAM to refuse a certificate request naming another tenant's signing CA"
            }
        }
    }
}

/// How a case exercises the chain.
#[derive(Debug, Clone, Copy)]
pub enum Probe {
    /// A full MQTT attempt.
    Connect {
        credential: Credential,
        password: Password,
        target: Target,
    },
    /// The REST device login only — the hop that happens before MQTT exists.
    DeviceLogin { fixture: &'static str },
    /// The outcome of the cross-tenant issuance experiment, which only an
    /// admin client can produce. `smoke-certs` records it; this reports it,
    /// because the plan requires an acceptance to fail *the matrix*.
    CrossTenantIssuance,
}

/// One named case.
#[derive(Debug, Clone, Copy)]
pub struct Case {
    pub name: &'static str,
    pub probe: Probe,
    pub expect: Expect,
    /// What this case would let through if it stopped failing. Printed on a
    /// failure, so the consequence is in front of whoever is reading.
    pub why: &'static str,
}

const fn connect(credential: Credential, password: Password, target: Target) -> Probe {
    Probe::Connect {
        credential,
        password,
        target,
    }
}

/// Every case, in the order they run.
#[must_use]
pub fn all() -> Vec<Case> {
    vec![
        Case {
            name: "positive",
            probe: connect(Credential::Own, Password::OwnToken, Target::Own),
            expect: Expect::Connected,
            why: "the whole chain — certificate, client identifier, user name, token subject \
                  and topic namespace — agreeing end to end",
        },
        Case {
            name: "mismatched-cert",
            probe: connect(
                Credential::CertOf(PEER),
                Password::OwnToken,
                Target::Own,
            ),
            expect: Expect::BrokerRejectedIdentity,
            // The broker fires first: `ssl_cert_client_id_from =
            // distinguished_name` compares the client identifier against the
            // certificate's subject before the backend is asked anything. The
            // Twin's own `client_id == "CN=" + username` check is the second
            // barrier, and is proven at unit level in plan 01-05.
            why: "a stolen token to be replayed with any certificate the holder happens to own",
        },
        Case {
            name: "malformed-token",
            probe: connect(Credential::Own, Password::Malformed, Target::Own),
            expect: Expect::AuthorizationDenied,
            why: "anything at all in the password field to authenticate a device",
        },
        Case {
            name: "bad-signature",
            probe: connect(Credential::Own, Password::CorruptSignature, Target::Own),
            expect: Expect::AuthorizationDenied,
            why: "a token forged by anyone who can read a real one's claims",
        },
        Case {
            name: "wrong-tenant",
            probe: connect(
                Credential::Own,
                Password::TokenOf(OTHER),
                Target::Own,
            ),
            expect: Expect::AuthorizationDenied,
            // Both tenants' tokens are signed by the same organization key, so
            // the signature alone separates nothing. Two independent barriers
            // stand here: the subject-to-user-name equality, which refuses
            // first, and the verifier's per-tenant assertion, which plan
            // 01-05 proves separately because no live case can isolate it —
            // a token whose subject matched would have to belong to an
            // account that exists in both tenants, and none does.
            why: "one tenant's device credentials to be used against another tenant's",
        },
        Case {
            name: "no-client-cert",
            probe: connect(Credential::None, Password::OwnToken, Target::Own),
            expect: Expect::HandshakeFailed,
            why: "the broker's peer-certificate requirement to be silently off, which looks \
                  identical to a working setup from any harness that only checks 'connect failed'",
        },
        Case {
            name: "other-tenant-ca",
            probe: connect(
                Credential::ForgedCommonName,
                Password::OwnToken,
                Target::Own,
            ),
            // The STRICT form of this case, and the sharpest negative in the
            // matrix. The certificate CHAINS fine — the broker's bundle is the
            // organization root (D-26), above both tenant CAs, so TLS trust
            // separates no tenants here and was never meant to. It also
            // carries the probe's OWN common name, so the broker's
            // client-identifier check has nothing to object to either. What is
            // left is exactly the residual risk D-24 names: a forged-common-name
            // certificate from another tenant's authority, combined with a
            // token for the account it impersonates. If anything refuses this,
            // that refusal IS the compensating control.
            expect: Expect::BrokerRejectedIdentity,
            why: "a certificate minted under another tenant's authority to speak for this one",
        },
        Case {
            name: "cross-tenant-ca-issuance",
            probe: Probe::CrossTenantIssuance,
            expect: Expect::IssuanceRefused,
            why: "any tenant administrator to mint certificates under any other tenant's \
                  signing CA, which is the whole of what the per-tenant CA tier is for",
        },
        Case {
            name: "namespace-sibling",
            probe: connect(Credential::Own, Password::OwnToken, Target::Sibling),
            expect: Expect::PublishRefused,
            why: "a device to write into the topics of the device next door",
        },
        Case {
            name: "namespace-other-tenant",
            probe: connect(Credential::Own, Password::OwnToken, Target::OtherTenant),
            expect: Expect::PublishRefused,
            why: "a device to write across the tenant boundary",
        },
        Case {
            name: "namespace-adjacent",
            probe: connect(Credential::Own, Password::OwnToken, Target::Adjacent),
            expect: Expect::PublishRefused,
            why: "a device whose identifier merely extends another's to write into it — the \
                  one case a prefix comparison in the Twin would pass",
        },
        Case {
            name: "empty-cert-binding",
            probe: Probe::DeviceLogin { fixture: UNBOUND },
            expect: Expect::LoginUnauthorized,
            why: "a certificate that was issued but never bound to authenticate as somebody",
        },
    ]
}
