//! Rust code sample for omnicat syntax preview.

fn factorial(n: u64) -> u64 {
    match n {
        0 | 1 => 1,
        _ => n * factorial(n - 1),
    }
}

fn main() {
    for i in 0..6 {
        println!("{i}! = {}", factorial(i));
    }
}
