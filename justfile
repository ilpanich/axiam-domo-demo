# AXIAM Domo Demo — root task dispatcher.
#
# This file stays thin on purpose (D-28). Later plans add recipes by adding
# their own module under `just/`, never by editing this file. Every import is
# optional (`import?`), so the repository is usable at any point in the phase
# even when a module has not landed yet.

import? 'just/pki.just'
import? 'just/stack.just'
import? 'just/edge.just'
import? 'just/authz.just'
import? 'just/twin.just'
import? 'just/smoke.just'
import? 'just/verify.just'

# List the available recipes.
default:
    @just --list --unsorted
