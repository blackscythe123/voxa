// Fixture for tools/lint (update-gate): not compiled.
fn launch_installer(st: &State) -> std::io::Result<()> {
    voxa_update::policy::can_install_now(&st.inputs())?;     // Good: the gate first
    std::process::Command::new(&st.path).args(installer_args("1.0.0", "1.0.1", &st.log)).spawn()?;   // Bad: no verify_file before it
    Ok(())
}

fn sneaky_install(path: &Path) {
    let _ = std::process::Command::new(path).arg("/VERYSILENT").spawn();   // Bad: only launch_installer starts the installer
}

fn open_notes(url: &str) {
    let _ = std::process::Command::new("explorer").arg(url).spawn();       // Good: not the installer
}

fn guarded(st: &State) -> std::io::Result<()> {
    can_install_now(&st.inputs())?;
    verify_file(&st.path, &st.offer)?;
    let _ = Command::new("x").args(installer_args("a", "b", "c")).spawn();  // Bad: right checks, wrong function name
    Ok(())
}

fn check_feed() {
    let c = voxa_update::fetch::Client::new(Origin::github(Channel::Desktop), V, "Windows");   // Bad: update-isolation, no test copy check
}

fn check_feed_guarded() {
    let origin = if test_hooks() { test_origin() } else { Origin::github(Channel::Desktop) };
    let c = voxa_update::fetch::Client::new(origin, V, "Windows");                             // Good
}
