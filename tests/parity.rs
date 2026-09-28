mod support;

use support::checks;
use support::parity::CAPABILITIES;

/// The claim the matrix exists to make: no command v1 ships is missing from it.
#[test]
fn every_v1_command_appears_in_the_matrix() {
    checks::every_v1_command_has_a_row(CAPABILITIES);
}

/// And no flag of a command that is in the matrix is silently dropped.
#[test]
fn every_v1_flag_is_accounted_for_by_some_row() {
    checks::every_v1_flag_is_accounted_for(CAPABILITIES);
}

#[test]
fn no_parity_row_names_a_v1_capability_that_does_not_exist() {
    checks::no_parity_row_is_stale(CAPABILITIES);
}
