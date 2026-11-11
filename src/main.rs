fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match gpx_trim::run(&args) {
        Ok(stdout) => print!("{stdout}"),
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(err.code);
        }
    }
}
