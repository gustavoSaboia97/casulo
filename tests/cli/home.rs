//! Functional tests of `casulo home`.

use std::path::PathBuf;

use crate::support::casulo;

#[test]
fn home_uses_casulo_home_when_set() {
    let custom_home = PathBuf::from("custom").join("casulo-home");

    casulo()
        .arg("home")
        .env("CASULO_HOME", &custom_home)
        .assert()
        .success()
        .stdout(format!("{}\n", custom_home.display()));
}

#[test]
fn home_falls_back_to_user_home_when_casulo_home_is_empty() {
    let user_home = PathBuf::from("fake-user-home");

    casulo()
        .arg("home")
        .env("CASULO_HOME", "")
        // The user home comes from HOME on Unix and USERPROFILE on Windows.
        .env("HOME", &user_home)
        .env("USERPROFILE", &user_home)
        .assert()
        .success()
        .stdout(format!("{}\n", user_home.join(".casulo").display()));
}
