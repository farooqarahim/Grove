#[test]
fn crate_compiles_and_exports_run() {
    let _: fn() = || {
        let _ = grove_daemon::run;
    };
}
