//! Every scenario of the harness against the plain group key.

use e2e_harness::scenarios;
use e2e_plain_key::PlainKey;

e2e_harness::scenario_tests!(plain_key, PlainKey);

/// `run_all` is what prints the numbers of the issue.
#[test]
fn run_all_reports_every_scenario_once() {
    let reports = scenarios::run_all::<PlainKey>();
    assert_eq!(reports.len(), 13);
    for report in &reports {
        assert!(
            report.events > 0 && report.bytes > 0 && report.devices > 0,
            "{report}"
        );
        assert_eq!(report.cipher, "plain-key");
        println!("{report}");
    }
}
