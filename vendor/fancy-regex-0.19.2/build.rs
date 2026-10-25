#[path = "../turbo-source-identity.rs"]
#[rustfmt::skip]
mod source_identity;

fn main() {
    source_identity::build();
}
