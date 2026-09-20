//! Per-tenant token verification and the whole denial surface around it.

use base64::Engine as _;
use device_twin::tenants::peek_tenant_id;

#[test]
fn tenant_peek_reads_the_claim_without_verifying() {
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(br#"{"sub":"sa-1","tenant_id":"t-9"}"#);
    let jwt = format!("aGVhZGVy.{payload}.c2ln");
    assert_eq!(peek_tenant_id(&jwt).as_deref(), Some("t-9"));
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
