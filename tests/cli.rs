use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_gpx-trim"))
}

fn dir(name: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-tmp")
        .join(name);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn command_prints_two_lines_and_writes_gpx() {
    let dir = dir("cli-ok");
    let input = dir.join("in.gpx");
    let output = dir.join("out.gpx");
    std::fs::write(
        &input,
        r#"<gpx><trk><trkseg>
<trkpt lat="0" lon="0"><ele>10</ele></trkpt>
<trkpt lat="0" lon="1"><ele>15.5</ele></trkpt>
</trkseg></trk></gpx>"#,
    )
    .unwrap();
    let result = bin()
        .args([
            "--input",
            input.to_str().unwrap(),
            "--epsilon",
            "10",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        "distance_m 111194.9\nelevation_gain_m 5.5\n"
    );
    assert!(result.stderr.is_empty());
    let written = std::fs::read_to_string(&output).unwrap();
    assert!(written.contains("lat=\"0.000000\""));
    assert!(written.contains("lon=\"1.000000\""));
    assert!(written.contains("<ele>10.0</ele>"));
    assert!(written.contains("<ele>15.5</ele>"));
}

#[test]
fn fewer_than_two_points_exits_2() {
    let dir = dir("cli-few");
    let input = dir.join("in.gpx");
    let output = dir.join("out.gpx");
    std::fs::write(&input, "<gpx></gpx>").unwrap();
    let result = bin()
        .args([
            "--input",
            input.to_str().unwrap(),
            "--epsilon",
            "10",
            "--output",
            output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(!output.exists());
}
