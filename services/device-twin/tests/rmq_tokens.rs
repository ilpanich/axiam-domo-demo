//! Per-tenant token verification and the whole denial surface around it.
//!
//! Every test here runs offline against a `wiremock` stand-in for the
//! organization key-set endpoint. Nothing reaches the real AXIAM, and nothing
//! waits out a real token lifetime.

mod common;

use common::{
    CLOCK_SKEW_SECS, FakeAxiam, FixtureKey, HARBOUR_ID, HARBOUR_SLUG, LAKESIDE_ID, LAKESIDE_SLUG,
    M2M_AUDIENCE, OTHER_SA, SA, TokenClaims, Twin, cn, hs256_token, now, registry,
};
use device_twin::tenants::peek_tenant_id;

const UNREGISTERED_TENANT: &str = "99999999-9999-9999-9999-999999999999";

/// Verify a token the way the CONNECT path does: route by the claimed tenant,
/// then let that tenant's own verifier decide.
async fn verify_as(
    axiam: &FakeAxiam,
    tenants: &[(&str, &str)],
    tenant_id: &str,
    token: &str,
) -> Result<String, String> {
    let registry = registry(&axiam.url(), tenants);
    let verifier = registry.verifier(tenant_id).map_err(|e| e.to_string())?;
    verifier
        .verify(token)
        .await
        .map(|c| c.sub)
        .map_err(|e| e.to_string())
}

// --- the happy path ---------------------------------------------------------

#[actix_web::test]
async fn a_device_token_verifies_and_yields_its_subject() {
    let axiam = FakeAxiam::start().await;
    let token = axiam.key.sign(&TokenClaims::device(SA, LAKESIDE_ID));
    let sub = verify_as(&axiam, &[(LAKESIDE_ID, LAKESIDE_SLUG)], LAKESIDE_ID, &token)
        .await
        .expect("a well-formed device token must verify");
    assert_eq!(sub, SA);
}

// --- expiry, and both sides of the clock-skew allowance ---------------------

#[actix_web::test]
async fn an_expired_token_is_rejected() {
    let axiam = FakeAxiam::start().await;
    let token = axiam
        .key
        .sign(&TokenClaims::device(SA, LAKESIDE_ID).expiring_at(now() - 3_600));
    let err = verify_as(&axiam, &[(LAKESIDE_ID, LAKESIDE_SLUG)], LAKESIDE_ID, &token)
        .await
        .expect_err("an hour-stale token must be refused");
    assert!(
        err.to_lowercase().contains("exp"),
        "expiry should be named as the reason, got {err:?}"
    );
}

#[actix_web::test]
async fn the_clock_skew_allowance_is_asserted_from_both_sides() {
    // The allowance is a named, bounded, non-configurable 60 s in the SDK's
    // verifier. Asserting only the accepting side would let it silently widen.
    let axiam = FakeAxiam::start().await;

    let inside = axiam
        .key
        .sign(&TokenClaims::device(SA, LAKESIDE_ID).expiring_at(now() - (CLOCK_SKEW_SECS / 2)));
    assert!(
        verify_as(&axiam, &[(LAKESIDE_ID, LAKESIDE_SLUG)], LAKESIDE_ID, &inside)
            .await
            .is_ok(),
        "expiry within the allowance must still verify"
    );

    let outside = axiam
        .key
        .sign(&TokenClaims::device(SA, LAKESIDE_ID).expiring_at(now() - (CLOCK_SKEW_SECS * 2)));
    assert!(
        verify_as(&axiam, &[(LAKESIDE_ID, LAKESIDE_SLUG)], LAKESIDE_ID, &outside)
            .await
            .is_err(),
        "expiry beyond the allowance must be refused"
    );
}

// --- signature, algorithm, audience, tenant ---------------------------------

#[actix_web::test]
async fn a_token_signed_by_an_unpublished_key_is_rejected() {
    let axiam = FakeAxiam::start_with_foreign_key().await;
    let token = axiam.key.sign(&TokenClaims::device(SA, LAKESIDE_ID));
    assert!(
        verify_as(&axiam, &[(LAKESIDE_ID, LAKESIDE_SLUG)], LAKESIDE_ID, &token)
            .await
            .is_err(),
        "a signature from a key the organization does not publish proves nothing"
    );
}

#[actix_web::test]
async fn another_algorithm_is_rejected_before_any_key_is_looked_up() {
    // The verifier pins the algorithm from the header *before* consulting the
    // key set, so an HS-signed token bearing an EdDSA key id never reaches a
    // key lookup at all — which is what stops algorithm confusion.
    let axiam = FakeAxiam::start().await;
    let token = hs256_token(&TokenClaims::device(SA, LAKESIDE_ID));
    assert!(
        verify_as(&axiam, &[(LAKESIDE_ID, LAKESIDE_SLUG)], LAKESIDE_ID, &token)
            .await
            .is_err()
    );
    assert_eq!(
        axiam.jwks_hits().await,
        0,
        "the unexpected key type must never be tried"
    );
}

#[actix_web::test]
async fn a_token_with_another_audience_is_rejected() {
    let axiam = FakeAxiam::start().await;
    for audience in ["axiam:user", "", "axiam:m2m:other"] {
        let token = axiam
            .key
            .sign(&TokenClaims::device(SA, LAKESIDE_ID).with_audience(audience));
        assert!(
            verify_as(&axiam, &[(LAKESIDE_ID, LAKESIDE_SLUG)], LAKESIDE_ID, &token)
                .await
                .is_err(),
            "{audience:?} is not {M2M_AUDIENCE}"
        );
    }
}

#[actix_web::test]
async fn a_token_naming_another_tenant_is_rejected_despite_a_valid_signature() {
    // The cross-tenant case. The key-set endpoint is organization-wide, so the
    // signature here is genuinely valid — it just says "some tenant in this
    // organization", which is not "this tenant". Remove the tenant assertion
    // from the verifier and this is the test that notices.
    let axiam = FakeAxiam::start().await;
    let token = axiam.key.sign(&TokenClaims::device(SA, HARBOUR_ID));
    let tenants = &[(LAKESIDE_ID, LAKESIDE_SLUG), (HARBOUR_ID, HARBOUR_SLUG)];
    assert!(
        verify_as(&axiam, tenants, LAKESIDE_ID, &token)
            .await
            .is_err(),
        "a valid signature for another tenant must not pass this tenant's verifier"
    );
}

#[actix_web::test]
async fn two_tenants_verifiers_are_independent() {
    let axiam = FakeAxiam::start().await;
    let tenants = &[(LAKESIDE_ID, LAKESIDE_SLUG), (HARBOUR_ID, HARBOUR_SLUG)];
    let lakeside_token = axiam.key.sign(&TokenClaims::device(SA, LAKESIDE_ID));
    let harbour_token = axiam.key.sign(&TokenClaims::device(OTHER_SA, HARBOUR_ID));

    assert!(verify_as(&axiam, tenants, LAKESIDE_ID, &lakeside_token).await.is_ok());
    assert!(verify_as(&axiam, tenants, HARBOUR_ID, &harbour_token).await.is_ok());
    // Each one is refused by the other's verifier.
    assert!(verify_as(&axiam, tenants, HARBOUR_ID, &lakeside_token).await.is_err());
    assert!(verify_as(&axiam, tenants, LAKESIDE_ID, &harbour_token).await.is_err());
}

#[actix_web::test]
async fn a_structurally_malformed_token_is_rejected_without_a_panic() {
    let axiam = FakeAxiam::start().await;
    for garbage in [
        "",
        "not-a-token",
        "a.b",
        "a.b.c.d",
        "...",
        "!!!.???.###",
        "eyJhbGciOiJFZERTQSJ9..",
    ] {
        assert!(
            verify_as(&axiam, &[(LAKESIDE_ID, LAKESIDE_SLUG)], LAKESIDE_ID, garbage)
                .await
                .is_err(),
            "{garbage:?} must be refused, not panicked on"
        );
    }
}

// --- routing: one verifier, never each in turn ------------------------------

#[actix_web::test]
async fn a_tenant_with_no_registered_verifier_is_refused_without_one_being_built() {
    // Falling back to another tenant's verifier would make the tenant
    // assertion meaningless: a token would succeed as soon as *any* tenant
    // accepted it. The registry refuses to serve a tenant it does not know.
    let axiam = FakeAxiam::start().await;
    let reg = registry(&axiam.url(), &[(LAKESIDE_ID, LAKESIDE_SLUG)]);
    assert!(
        reg.verifier(UNREGISTERED_TENANT).is_err(),
        "an unregistered tenant must not resolve to a verifier"
    );
    assert_eq!(
        axiam.jwks_hits().await,
        0,
        "no verifier was invoked, so no key set was fetched"
    );
}

#[actix_web::test]
async fn the_connect_endpoint_denies_an_unregistered_tenant_without_touching_the_key_set() {
    let axiam = FakeAxiam::start().await;
    let token = axiam
        .key
        .sign(&TokenClaims::device(SA, UNREGISTERED_TENANT));
    let twin = Twin::new(registry(&axiam.url(), &[(LAKESIDE_ID, LAKESIDE_SLUG)]));
    let cid = cn(SA);
    let (status, body) = twin
        .post(
            "/rmq/user",
            &[
                ("username", SA),
                ("password", &token),
                ("vhost", "domo"),
                ("client_id", &cid),
            ],
        )
        .await;
    assert_eq!((status, body.as_str()), (200, "deny"));
    assert_eq!(axiam.jwks_hits().await, 0);
}

#[actix_web::test]
async fn the_registry_warms_one_verifier_per_tenant_on_disk() {
    let axiam = FakeAxiam::start().await;
    let reg = registry(
        &axiam.url(),
        &[(LAKESIDE_ID, LAKESIDE_SLUG), (HARBOUR_ID, HARBOUR_SLUG)],
    );
    assert_eq!(reg.warm().expect("both tenants must build"), 2);
    assert_eq!(reg.registered(), 2);
    // Warming builds verifiers; it does not fetch key sets.
    assert_eq!(axiam.jwks_hits().await, 0);
}

// --- what a successful CONNECT leaves behind --------------------------------

#[actix_web::test]
async fn a_successful_connect_caches_the_tenant_and_the_tokens_own_expiry() {
    let axiam = FakeAxiam::start().await;
    let claims = TokenClaims::device(SA, LAKESIDE_ID);
    let token = axiam.key.sign(&claims);
    let twin = Twin::new(registry(&axiam.url(), &[(LAKESIDE_ID, LAKESIDE_SLUG)]));
    let cid = cn(SA);

    let (status, body) = twin
        .post(
            "/rmq/user",
            &[
                ("username", SA),
                ("password", &token),
                ("vhost", "domo"),
                ("client_id", &cid),
            ],
        )
        .await;
    assert_eq!((status, body.as_str()), (200, "allow"));

    let cached = twin.sessions.get(SA).expect("the CONNECT must cache a session");
    assert_eq!(cached.tenant_id, LAKESIDE_ID);
    assert_eq!(cached.tenant_slug, LAKESIDE_SLUG);
    assert_eq!(
        cached.exp, claims.exp,
        "the cache must carry the token's own expiry, not one of its own"
    );

    // The token-less endpoints now have their evidence.
    assert_eq!(
        twin.post("/rmq/vhost", &[("username", SA), ("vhost", "domo")])
            .await,
        (200, "allow".into())
    );
}

#[actix_web::test]
async fn the_cache_stops_granting_once_the_tokens_expiry_passes() {
    let axiam = FakeAxiam::start().await;
    // Already outside the allowance, so the CONNECT itself is refused and the
    // cache is never filled — the same deny the expired entry would produce.
    let token = axiam
        .key
        .sign(&TokenClaims::device(SA, LAKESIDE_ID).expiring_at(now() - (CLOCK_SKEW_SECS * 5)));
    let twin = Twin::new(registry(&axiam.url(), &[(LAKESIDE_ID, LAKESIDE_SLUG)]));
    let cid = cn(SA);

    let (status, body) = twin
        .post(
            "/rmq/user",
            &[
                ("username", SA),
                ("password", &token),
                ("vhost", "domo"),
                ("client_id", &cid),
            ],
        )
        .await;
    assert_eq!((status, body.as_str()), (200, "deny"));
    assert!(twin.sessions.is_empty(), "a refused CONNECT caches nothing");

    assert_eq!(
        twin.post("/rmq/vhost", &[("username", SA), ("vhost", "domo")])
            .await,
        (200, "deny".into())
    );
}

// --- the routing hint, which is only ever a hint ----------------------------

#[test]
fn tenant_peek_reads_the_claim_without_verifying() {
    let key = FixtureKey::generate("kid");
    let token = key.sign(&TokenClaims::device(SA, HARBOUR_ID));
    assert_eq!(peek_tenant_id(&token).as_deref(), Some(HARBOUR_ID));
}

#[test]
fn tenant_peek_is_total_on_garbage() {
    for garbage in ["", "not-a-jwt", "a.b", "a.!!!.c", "....."] {
        assert_eq!(
            peek_tenant_id(garbage),
            None,
            "{garbage:?} must not yield a tenant"
        );
    }
}
