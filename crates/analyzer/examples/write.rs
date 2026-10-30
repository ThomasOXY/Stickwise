//! Analyse one file and write its ANLZ files: `write <audio> <dir>`.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let t = std::time::Instant::now();
    let r = analyzer::analyze(std::path::Path::new(&a[1]), None, None).unwrap();
    println!("{:?} key {:?} in {:?}", r.grid, r.key, t.elapsed());
    println!("{}", analyzer::write(&r, &a[1], std::path::Path::new(&a[2])).unwrap().display());
}
