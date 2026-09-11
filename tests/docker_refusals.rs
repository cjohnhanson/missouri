//! What `docker: true` refuses, and how. The Docker backend runs a
//! transition through the Docker API and builds no host command, so an
//! assertion, a comparator, and a service have nowhere to run. Each
//! reached an `unreachable!()` and panicked; a refusal names the cause.
//! The comparator and the service are unit-tested against the backend
//! value. An assertion needs a graph, so this drives the binary.

use std::fs;
use std::process::Command;

#[test]
fn an_assertion_under_docker_is_refused_not_panicked() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = tmp.path();
    fs::create_dir_all(root.join(".missouri")).expect("root config dir");
    fs::write(root.join(".missouri/missouri.yml"), "docker: true\n").expect("root config");
    fs::create_dir_all(root.join("a/.missouri")).expect("state a");
    fs::write(
        root.join("a/.missouri/missouri.yml"),
        "assertions:\n  - name: \"true runs\"\n    command: \"true\"\n\
         transitions:\n  - name: \"nothing\"\n    command: \"true\"\n    target: ../b\n",
    )
    .expect("state a config");
    fs::create_dir_all(root.join("b/.missouri")).expect("state b");
    fs::write(root.join("b/.missouri/missouri.yml"), "{}\n").expect("state b config");

    let out = Command::new(env!("CARGO_BIN_EXE_missouri"))
        .arg("run")
        .current_dir(root)
        .output()
        .expect("missouri runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!text.contains("panicked"), "the run panicked: {text}");
    assert!(
        text.contains("cannot run with docker: true"),
        "the assertion should be refused by name: {text}"
    );
}
