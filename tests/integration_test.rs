// Integration test for SigmaOS-bash-completion
// Verifies the crate builds and links as a dependency.
#![allow(unused_imports)]

use bash_completion as _;

#[test]
fn crate_links_and_runs() {
    assert_eq!(2 + 2, 4);
}
