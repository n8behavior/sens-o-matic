//! Every scenario of the harness against MLS with OpenMLS. The names of the
//! tests are the names of the scenarios, in the module `openmls`.

use e2e_harness::scenarios;
use e2e_openmls::OpenMls;

e2e_harness::scenario_tests!(openmls, OpenMls);

/// `run_all` is what the numbers of the issue are printed from.
#[test]
fn run_all_reports_every_scenario_once() {
    let reports = scenarios::run_all::<OpenMls>();
    assert_eq!(reports.len(), 13);
    for report in &reports {
        assert_eq!(report.cipher, "openmls");
        assert!(
            report.events > 0 && report.bytes > 0 && report.devices > 0,
            "{report}"
        );
    }
}
