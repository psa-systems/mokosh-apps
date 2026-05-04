fn main() {
    println!("cargo:rerun-if-env-changed=ADMIN_EMAIL");
    println!("cargo:rerun-if-env-changed=ADMIN_PASSWORD");
}
