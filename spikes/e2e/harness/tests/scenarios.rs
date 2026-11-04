//! Every scenario of the harness against `FakeCipher`, which has no
//! cryptography. This tests the harness itself: the simulator, the log, the
//! rules, the evil server and the scenarios. The candidates run the same
//! scenarios against their own ciphers.

use e2e_harness::{fake::FakeCipher, scenarios};

e2e_harness::scenario_tests!(fake, FakeCipher);

/// `run_all` is what a candidate calls to print the numbers of its issue.
#[test]
fn run_all_reports_every_scenario_once() {
    let reports = scenarios::run_all::<FakeCipher>();
    let names: Vec<_> = reports.iter().map(|report| report.scenario).collect();
    assert_eq!(names.len(), 13, "{names:?}");
    let mut unique = names.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 13, "{names:?}");
    for report in &reports {
        assert!(
            report.events > 0 && report.bytes > 0 && report.devices > 0,
            "{report}"
        );
        assert_eq!(report.cipher, "fake");
    }
}
