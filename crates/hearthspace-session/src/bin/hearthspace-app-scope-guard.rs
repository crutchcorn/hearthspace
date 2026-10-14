#[path = "../app_scopes/guard.rs"]
mod guard;

fn main() -> std::io::Result<()> {
    guard::run(&std::env::args().skip(1).collect::<Vec<_>>())
}
