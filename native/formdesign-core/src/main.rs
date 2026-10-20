mod core;
mod damage;
mod find;
mod preflight;
mod proto;
mod slots;
mod spell;
mod textloc;
mod thumb;
mod web;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    proto::init_logger();
    let mut c = core::Core::new();
    c.load_fonts_from_env();
    if args.iter().any(|a| a == "--selfcheck") {
        let ok = c.selfcheck();
        println!("formdesign-core {} selfcheck {}", env!("CARGO_PKG_VERSION"), if ok { "ok" } else { "failed" });
        std::process::exit(if ok { 0 } else { 1 });
    }
    proto::serve(&mut c);
}
