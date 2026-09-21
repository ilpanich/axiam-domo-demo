//! The catalog is pure: deserialization and validation reach no network, so
//! every case below runs in-process with an inline fixture.
//!
//! These tests exist to stop three specific failures reaching a live tenant:
//! a wildcard permission (D-16 forbids one outright), a role granting an
//! action nothing defines, and a duplicate name — each of which AXIAM would
//! either accept as something subtly different or reject halfway through an
//! apply, after some of the catalog had already landed.

use domo_bootstrap::catalog;

/// A minimal well-formed catalog with one role granting four actions.
const INSTALLER: &str = r#"
version = 1
group_pattern = "{role}@{type}:{slug}"

[[permission]]
action = "device:create"
description = "Add a device to the registry"

[[permission]]
action = "device:update"
description = "Edit a device"

[[permission]]
action = "device:delete"
description = "Remove a device"

[[permission]]
action = "device:configure"
description = "Change a device's configuration"

[[role]]
name = "installer"
description = "Installs and configures devices"
permissions = ["device:create", "device:update", "device:delete", "device:configure"]
"#;

fn grants_of(manifest: &axiam_sdk::management::manifest::ManagementManifest, role: &str) -> Vec<String> {
    manifest
        .roles
        .iter()
        .find(|r| r.name == role)
        .map(|r| r.grants.iter().map(|g| g.permission.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn a_role_yields_exactly_the_grants_it_declares() {
    let catalog = catalog::parse(INSTALLER).expect("the fixture is well-formed");
    let manifest = catalog.to_manifest();

    assert_eq!(manifest.permissions.len(), 4, "four permissions declared");
    let mut grants = grants_of(&manifest, "installer");
    grants.sort();
    assert_eq!(
        grants,
        vec![
            "device:configure".to_owned(),
            "device:create".to_owned(),
            "device:delete".to_owned(),
            "device:update".to_owned(),
        ],
        "the role grants those four actions and no others"
    );
}

#[test]
fn a_wildcard_permission_is_refused_and_the_error_names_it() {
    // D-16: "there is no wildcard; `structure:*` must be expanded". A wildcard
    // would silently widen a role the day AXIAM learns to interpret one.
    let src = INSTALLER.replace(r#"action = "device:create""#, r#"action = "structure:*""#);
    let src = src.replace(r#""device:create""#, r#""structure:*""#);

    let err = catalog::parse(&src).expect_err("a wildcard must be refused");
    let msg = err.to_string();
    assert!(msg.contains("structure:*"), "error names the entry: {msg}");
}

#[test]
fn a_wildcard_inside_a_role_grant_is_refused_naming_the_role() {
    let src = format!("{INSTALLER}\npermissions = []\n")
        .replace("permissions = []\n", "")
        .replace(
            r#"permissions = ["device:create", "device:update", "device:delete", "device:configure"]"#,
            r#"permissions = ["device:*"]"#,
        );

    let err = catalog::parse(&src).expect_err("a wildcard grant must be refused");
    let msg = err.to_string();
    assert!(msg.contains("installer"), "error names the role: {msg}");
    assert!(msg.contains("device:*"), "error names the entry: {msg}");
}

#[test]
fn a_dangling_permission_key_is_refused_naming_the_key() {
    let src = INSTALLER.replace(r#""device:configure"]"#, r#""device:teleport"]"#);

    let err = catalog::parse(&src).expect_err("a dangling key must be refused");
    let msg = err.to_string();
    assert!(msg.contains("device:teleport"), "error names the key: {msg}");
    assert!(msg.contains("installer"), "error names the role: {msg}");
}

#[test]
fn a_duplicate_role_is_refused_naming_the_duplicate() {
    let src = format!(
        "{INSTALLER}\n\n[[role]]\nname = \"installer\"\ndescription = \"Again\"\npermissions = []\n"
    );

    let err = catalog::parse(&src).expect_err("a duplicate role must be refused");
    let msg = err.to_string();
    assert!(msg.contains("installer"), "error names the duplicate: {msg}");
}

#[test]
fn a_duplicate_permission_is_refused_naming_the_duplicate() {
    let src = format!(
        "{INSTALLER}\n\n[[permission]]\naction = \"device:create\"\ndescription = \"Again\"\n"
    );

    let err = catalog::parse(&src).expect_err("a duplicate permission must be refused");
    let msg = err.to_string();
    assert!(msg.contains("device:create"), "error names the duplicate: {msg}");
}

#[test]
fn an_empty_role_name_is_refused() {
    let src = INSTALLER.replace(r#"name = "installer""#, r#"name = """#);

    let err = catalog::parse(&src).expect_err("an empty role name must be refused");
    let msg = err.to_string();
    assert!(
        msg.to_lowercase().contains("empty"),
        "error says the name is empty: {msg}"
    );
}

#[test]
fn a_role_granting_nothing_is_still_created() {
    // An empty grant list is valid and means "this role exists and confers
    // nothing yet" — the state `concierge` is in before its common-area group
    // bindings are made in Phase 2.
    let src = format!(
        "{INSTALLER}\n\n[[role]]\nname = \"bystander\"\ndescription = \"Confers nothing yet\"\npermissions = []\n"
    );

    let catalog = catalog::parse(&src).expect("an empty grant list is valid");
    let manifest = catalog.to_manifest();

    assert!(
        manifest.roles.iter().any(|r| r.name == "bystander"),
        "the role is present in the manifest"
    );
    assert!(
        grants_of(&manifest, "bystander").is_empty(),
        "and it grants nothing"
    );
}

#[test]
fn group_templates_name_the_eager_roles_per_resource_type() {
    let src = format!(
        "{INSTALLER}\n\n[[group_template]]\nresource_type = \"site\"\nroles = [\"installer\"]\n"
    );

    let catalog = catalog::parse(&src).expect("the fixture is well-formed");
    assert_eq!(catalog.roles_for("site"), vec!["installer".to_owned()]);
    assert!(
        catalog.roles_for("device").is_empty(),
        "device grant groups are lazy (D-21)"
    );
}

/// The catalog actually shipped, not a fixture. Guards the eight names three
/// later phases resolve through.
const SHIPPED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../authz/catalog.toml"
));

#[test]
fn the_shipped_catalog_declares_the_eight_roles_of_d16() {
    let catalog = catalog::parse(SHIPPED).expect("the shipped catalog must be valid");

    let mut names: Vec<&str> = catalog.roles.iter().map(|r| r.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "common-device-manager",
            "common-operator",
            "concierge",
            "device-self",
            "granted-operator",
            "installer",
            "property-manager",
            "resident",
        ],
        "exactly the eight roles D-16 names"
    );

    for role in &catalog.roles {
        assert!(
            !role.description.trim().is_empty(),
            "role '{}' has a description",
            role.name
        );
        assert!(
            !role.permissions.is_empty(),
            "role '{}' enumerates its permissions",
            role.name
        );
    }
}

#[test]
fn no_staff_role_in_the_shipped_catalog_can_operate_a_device() {
    // The first of the four demo moments: no property manager, concierge or
    // installer may operate an apartment's devices. `concierge` reaches
    // `device:operate` only through `common-operator`, which is scoped to a
    // common area and can therefore never reach inside an apartment.
    let catalog = catalog::parse(SHIPPED).expect("the shipped catalog must be valid");

    for staff in ["property-manager", "installer", "concierge"] {
        let role = catalog
            .roles
            .iter()
            .find(|r| r.name == staff)
            .unwrap_or_else(|| panic!("role '{staff}' exists"));
        assert!(
            !role.permissions.iter().any(|p| p == "device:operate"),
            "'{staff}' must not grant device:operate directly"
        );
    }
}

#[test]
fn the_shipped_catalog_contains_no_wildcard_anywhere() {
    // Belt and braces with the `just` grep: this one also sees a wildcard that
    // survives inside a value the grep's comment filter might skip.
    for (n, line) in SHIPPED.lines().enumerate() {
        let code = line.split('#').next().unwrap_or("");
        assert!(
            !code.contains('*'),
            "line {} of authz/catalog.toml contains a wildcard: {line}",
            n + 1
        );
    }
}

#[test]
fn the_shipped_group_templates_match_d21() {
    let catalog = catalog::parse(SHIPPED).expect("the shipped catalog must be valid");

    assert_eq!(catalog.roles_for("portfolio"), vec!["property-manager"]);
    assert_eq!(catalog.roles_for("site"), vec!["installer", "concierge"]);
    assert_eq!(
        catalog.roles_for("common"),
        vec!["common-operator", "common-device-manager"]
    );
    assert_eq!(catalog.roles_for("apartment"), vec!["resident"]);
    assert!(
        catalog.roles_for("device").is_empty(),
        "a device's grant group is created lazily in Phase 2 (D-21)"
    );
    assert!(
        catalog.roles_for("building").is_empty(),
        "a building's devices live under its common node; the site binding cascades"
    );
}

#[test]
fn a_group_template_naming_an_undefined_role_is_refused() {
    let src = format!(
        "{INSTALLER}\n\n[[group_template]]\nresource_type = \"site\"\nroles = [\"plumber\"]\n"
    );

    let err = catalog::parse(&src).expect_err("an undefined role must be refused");
    let msg = err.to_string();
    assert!(msg.contains("plumber"), "error names the role: {msg}");
}
