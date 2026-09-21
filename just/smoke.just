# The live smoke suite: one real branch of the tree, the authorization model
# asserted against a real AXIAM, and the device identity chain proven against a
# real broker — positively and, more importantly, negatively.
#
# Same ownership split as the rest of the stack (D-34/D-35): `just` drives the
# host, the Rust tools do the AXIAM and broker work from inside the compose
# network, so the demo machine needs no cargo toolchain. Every recipe reuses
# `_stage` and `_probe`, defined in `just/stack.just`.
#
# NOTE: `set shell` is deliberately absent. `just` refuses a setting that two
# imported modules both define, and `just/stack.just` already sets it for the
# whole import tree.
#
# # Each part runs alone, on purpose
#
# `smoke` composes the parts rather than duplicating them, and every part is
# independently runnable. A failure inside a composed run is only diagnosable
# if the part can be re-run by itself in seconds instead of after a full
# rebuild.
#
# # No reset here
#
# `just demo-reset` is built in plan 01-07, which depends on this plan, so it
# does not exist at this point and nothing below may call it. `smoke-teardown`
# is this suite's clean-state mechanism: it removes only reserved-prefix
# fixtures, so `smoke-teardown` followed by `smoke` rebuilds from an emptied
# tenant. The reset-crossed form of the same property is `just phase-verify`,
# in plan 01-07.

# --- the branch ---------------------------------------------------------------

# Build the one reserved-prefix branch — site → common → building → common →
# apartment → device — with its structural groups and the probe's device
# accounts. Idempotent by probe: a second run resolves everything and creates
# nothing.
smoke-tree:
    @just _stage smoke

# Remove every reserved-prefix fixture from both tenants, and the on-disk
# fixture material with it. Touches nothing without the prefix.
smoke-teardown:
    @just _stage smoke-teardown
