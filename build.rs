fn main() {
    println!("cargo:rustc-link-arg-tests=-Wl,--no-such-flag-zzz");
}
